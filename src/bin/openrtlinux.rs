//! openrtlinux - openRTLinux's daemon: RightType's detection core, wired to a Linux desktop.
//!
//! Run it from a terminal: it prints what it captured, what the core decided,
//! and (unless `--dry-run`) what it injected.
//!
//! ```text
//! openrtlinux --dry-run     # watch only: never types, never switches layout
//! openrtlinux               # fix words: switch the layout, then type
//! ```
//!
//! Everything the Windows app does at the text cursor (palette, ghost
//! suggestions, at-caret lists, statistics, tray) is not here yet.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

use righttype::layout::{self as core_layout};
use righttype::linuxos::layout::{self as os_layout};
use righttype::linuxos::{capture as kb_capture, config, control, inject, keytable, live};
use righttype::policy::InputLayout;
use evdev::KeyCode;

fn table_for(layout: InputLayout) -> &'static dyn core_layout::Layout {
    live::table_for(layout)
}

/// The character the physical key makes in the active layout.
fn produced(code: u16, shift: bool, layout: InputLayout) -> Option<char> {
    let us = keytable::us_char(code, shift)?;
    Some(table_for(layout).char_of(us).unwrap_or(us))
}

fn boundary_char(code: u16, shift: bool) -> Option<char> {
    match code {
        c if c == KeyCode::KEY_SPACE.0 => Some(' '),
        c if c == KeyCode::KEY_ENTER.0 || c == KeyCode::KEY_KPENTER.0 => Some('\n'),
        c if c == KeyCode::KEY_TAB.0 => Some('\t'),
        _ => keytable::us_char(code, shift).filter(|c| *c == ' '),
    }
}

fn is_reset(code: u16) -> bool {
    [
        KeyCode::KEY_ESC,
        KeyCode::KEY_LEFT,
        KeyCode::KEY_RIGHT,
        KeyCode::KEY_UP,
        KeyCode::KEY_DOWN,
        KeyCode::KEY_HOME,
        KeyCode::KEY_END,
        KeyCode::KEY_PAGEUP,
        KeyCode::KEY_PAGEDOWN,
        KeyCode::KEY_DELETE,
    ]
    .iter()
    .any(|k| k.code() == code)
}

struct Args {
    /// None = whatever the config file says (the config is safe by default).
    dry_run: Option<bool>,
    current_command: Option<String>,
    switch_command: Option<String>,
    layout: Option<String>,
    mode: Option<String>,
    config: Option<String>,
    debug: bool,
}

fn parse_args() -> Args {
    let mut args = Args {
        dry_run: None,
        current_command: None,
        switch_command: None,
        layout: None,
        mode: None,
        config: None,
        debug: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--dry-run" => args.dry_run = Some(true),
            "--fix" => args.dry_run = Some(false),
            "--current-command" => args.current_command = Some(it.next().unwrap_or_default()),
            "--switch-command" => args.switch_command = Some(it.next().unwrap_or_default()),
            "--layout" => args.layout = it.next(),
            "--mode" => args.mode = it.next(),
            "--config" => args.config = it.next(),
            "--debug" => args.debug = true,
            "-h" | "--help" => {
                println!(
                    "openrtlinux [--dry-run] [--layout thai|us] \
                     [--current-command 'CMD'] [--switch-command 'CMD']"
                );
                std::process::exit(0);
            }
            other => eprintln!("unknown argument: {other}"),
        }
    }
    args
}

/// The desktop's active layout, read in the background.
///
/// Reading it means spawning `gsettings`, which costs tens of milliseconds and
/// occasionally more. Doing that inside the key loop stalls the very moments the
/// typist is waiting on us, so the probe runs on its own thread and the hot path
/// only ever locks a small slot. [`Probe::refresh`] asks for a fresh read right
/// after we press the desktop's switch key.
struct Probe {
    layout: Arc<Mutex<Option<InputLayout>>>,
    ask: Arc<AtomicBool>,
}

impl Probe {
    fn start(command: String) -> Self {
        let probe = Self {
            layout: Arc::new(Mutex::new(os_layout::current(&command))),
            ask: Arc::new(AtomicBool::new(false)),
        };
        let slot = Arc::clone(&probe.layout);
        let ask = Arc::clone(&probe.ask);
        std::thread::spawn(move || {
            let mut last = Instant::now() - Duration::from_secs(1);
            let mut last_seen: Option<InputLayout> = *slot.lock().unwrap();
            loop {
                let asked = ask.swap(false, Ordering::Relaxed);
                if asked || last.elapsed() > Duration::from_millis(400) {
                    last = Instant::now();
                    if let Some(now) = os_layout::current(&command) {
                        last_seen = Some(now);
                        if let Ok(mut guard) = slot.lock() {
                            *guard = Some(now);
                        }
                    } else {
                        let _ = last_seen;
                    }
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        probe
    }

    fn get(&self) -> Option<InputLayout> {
        self.layout.lock().ok().and_then(|guard| *guard)
    }

    fn refresh(&self) {
        self.ask.store(true, Ordering::Relaxed);
    }
}

/// Press the desktop's switch key and wait until the probe agrees it landed.
///
/// The desktop switches on its own schedule, so this is the one place a fix
/// genuinely has to wait for something outside us. 900 ms is generous: mutter
/// usually reports the new layout within ~50 ms.
fn switch_verified(
    target: InputLayout,
    chord: &[u16],
    injector: &mut inject::Injector,
    probe: &Probe,
    settle: Duration,
) -> Result<(), String> {
    for attempt in 0..2 {
        injector.chord(chord).map_err(|e| e.to_string())?;
        let deadline = Instant::now() + settle;
        while Instant::now() < deadline {
            // ask again *every* turn of the loop: a refresh that was already in
            // flight still carries the layout from before the switch, and waiting
            // for the next scheduled read costs up to the whole poll interval
            probe.refresh();
            if probe.get() == Some(target) {
                return Ok(());
            }
            sleep(Duration::from_millis(5));
        }
        if attempt == 0 && probe.get() == Some(target) {
            return Ok(());
        }
    }
    Err(format!(
        "the desktop did not switch to {target:?} (is the switch key still set?)"
    ))
}

/// Run a plan and say how long it took, so slowness can be measured, not guessed.
fn apply_plan(
    committed: &live::Committed,
    injector: Option<&mut inject::Injector>,
    chord: &[u16],
    probe: &Probe,
    dry_run: bool,
    debug: bool,
) {
    let started = Instant::now();
    match run_plan(&committed.steps, injector, chord, probe, dry_run) {
        // always say how long it took: "slow" is easier to fix with a number,
        // and the journal keeps the number for every fix
        Ok(()) => println!("  applied in {} ms", started.elapsed().as_millis()),
        Err(why) => eprintln!("  could not apply it: {why}"),
    }
    let _ = debug;
}

fn main() {
    let args = parse_args();
    let config_path = args
        .config
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(config::Config::path);
    let mut cfg = config::Config::load_from(&config_path);
    if let Some(mode) = args.mode.as_deref().and_then(config::Mode::parse) {
        cfg.mode = mode;
    }
    if let Some(dry_run) = args.dry_run {
        cfg.dry_run = dry_run;
    }
    if let Some(current) = args.current_command.clone() {
        cfg.current_command = current;
    }
    if let Some(switch) = args.switch_command.clone() {
        cfg.switch_command = switch;
    }
    if let Some(layout) = args.layout.clone() {
        cfg.layout = layout;
    }
    let mut capture = match kb_capture::Capture::open() {
        Ok(c) => c,
        Err(why) => {
            eprintln!("cannot watch the keyboard: {why}");
            std::process::exit(2);
        }
    };
    let probe = Probe::start(cfg.current_command.clone());
    let active = cfg
        .layout_override()
        .or_else(|| probe.get())
        .unwrap_or(InputLayout::UsQwerty);

    let mut chord = os_layout::switch_chord(&cfg.switch_command);
    let mut fix_chord = cfg.fix_chord();
    let mut injector = if cfg.dry_run {
        None
    } else {
        match inject::Injector::new() {
            Ok(inj) => Some(inj),
            Err(why) => {
                eprintln!("cannot create the virtual keyboard: {why}");
                eprintln!("(needs write access to /dev/uinput - uaccess ACL or the input group)");
                std::process::exit(2);
            }
        }
    };

    println!("openRTLinux {}: {}", env!("CARGO_PKG_VERSION"), capture.describe());
    println!("  fix chord: {}", fix_chord.label());
    println!(
        "  {}",
        os_layout::describe(&cfg.current_command, &cfg.switch_command)
    );

    let mut live = live::Live::new(active);
    live.mode = cfg.mode;
    live.recall_ms = config::window_ms(cfg.recall_seconds);
    live.undo_ms = config::window_ms(cfg.undo_seconds);

    // The tray and the settings window talk to us here.
    let control = match control::Control::start(control::Status {
        mode: cfg.mode,
        dry_run: cfg.dry_run,
        layout: live.layout(),
        committed: 0,
        skipped: 0,
        last: String::new(),
        history: Vec::new(),
        can_undo: false,
        backend: format!("evdev · {}", if cfg.dry_run { "dry-run" } else { "fix" }),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }) {
        Ok(control) => control,
        Err(why) => {
            eprintln!(
                "cannot open the control socket ({}): {why}",
                control::socket_path().display()
            );
            std::process::exit(2);
        }
    };
    println!("  socket: {}", control::socket_path().display());
    println!(
        "  mode: {} · {}{}",
        cfg.mode.as_str(),
        if cfg.dry_run { "dry-run (never types)" } else { "fix" },
        if chord.is_empty() { " · no switch chord found" } else { "" }
    );
    println!(
        "  recall: {} after a word is closed · undo: {} after a fix",
        describe_window(cfg.recall_seconds),
        describe_window(cfg.undo_seconds)
    );

    let mut last_probe = Instant::now();
    let mut last_publish = Instant::now();
    let mut pending: Vec<kb_capture::RawKey> = Vec::new();
    let mut last_backspace: Option<Instant> = None;
    // A layout change the probe reports that is not ours yet: kept until it
    // persists, because a fix of ours switches the layout too.
    let mut mismatch: Option<(InputLayout, Instant)> = None;
    // The recall windows are measured against this, never against wall time: a
    // clock step (or a suspended laptop) must not expire a word early.
    let clock = Instant::now();
    'daemon: loop {
        if control.quitting() {
            println!("openrtlinux: asked to quit");
            break 'daemon;
        }
        if control.take_reload() {
            cfg = config::Config::load_from(&config_path);
            live.recall_ms = config::window_ms(cfg.recall_seconds);
            live.undo_ms = config::window_ms(cfg.undo_seconds);
            chord = os_layout::switch_chord(&cfg.switch_command);
            if cfg.fix_chord() != fix_chord {
                fix_chord = cfg.fix_chord();
                println!("  fix chord is now: {}", fix_chord.label());
            }
            // The socket state is what the tray reads, so the reload has to land
            // there too - otherwise the next loop turn would put the old mode back.
            control.update(|status| {
                status.mode = cfg.mode;
                status.dry_run = cfg.dry_run;
                status.backend = format!(
                    "evdev · {}",
                    if cfg.dry_run { "dry-run" } else { "fix" }
                );
            });
            println!(
                "openrtlinux: config reloaded (mode {}, dry_run {}, recall {}, undo {})",
                cfg.mode.as_str(),
                cfg.dry_run,
                describe_window(cfg.recall_seconds),
                describe_window(cfg.undo_seconds)
            );
        }
        let live_status = control.status();
        live.mode = live_status.mode;
        let dry_run = live_status.dry_run;
        // before any decision this turn: how old the memories are now
        live.set_now(clock.elapsed().as_millis() as u64);

        let mut batch: Vec<kb_capture::RawKey> = pending.drain(..).collect();
        batch.extend(capture.poll());
        for key in batch {
            if key.value != 1 {
                continue; // presses only, like the hook
            }
            let mods = key.mods;
            if args.debug {
                println!(
                    "key code={} value={} shift={} caps={} buf={:?} mode={}",
                    key.code,
                    key.value,
                    mods.shift,
                    mods.caps,
                    live.current_word(),
                    live.mode.as_str()
                );
            }
            if mods.ctrl || mods.alt || mods.meta {
                continue; // a shortcut, not typing
            }
            if chord.contains(&key.code) {
                // the desktop's own layout switch key: mutter grabs it, but the
                // kernel event still reaches us. It is not a character - and a
                // stray one in the buffer ("`fvdw,h") would stop a fix working.
                live.on_reset();
                continue;
            }
            if kb_capture::is_modifier_code(key.code) {
                // A shift landing just after a Backspace is the same chord, the
                // other way round: the typist is fast, the kernel is not ordered
                // the way we would like.
                if fix_chord == config::FixChord::ShiftBackspace
                    && (key.code == KeyCode::KEY_LEFTSHIFT.0
                        || key.code == KeyCode::KEY_RIGHTSHIFT.0)
                {
                    if let Some(at) = last_backspace {
                        if at.elapsed() < Duration::from_millis(120) {
                            if let Some(committed) = live.on_fix_hotkey_after_backspace() {
                                pending.extend(
                                    capture.wait_for_modifier_release(Duration::from_millis(600)),
                                );
                                if !capture.mods().any_held() {
                                    report(&committed);
                                    apply_plan(
                                        &committed,
                                        injector.as_mut(),
                                        &chord,
                                        &probe,
                                        dry_run,
                                        args.debug,
                                    );
                                }
                            }
                        }
                    }
                }
                continue; // modifiers are never characters
            }
            if key.code == KeyCode::KEY_BACKSPACE.0 {
                if mods.shift && fix_chord == config::FixChord::ShiftBackspace {
                    // the fix hotkey: the application already deleted one character
                    if let Some(committed) = live.on_fix_hotkey(1) {
                        // Shift is still down - injecting now would arrive shifted
                        // (capitals in Latin, upper-row glyphs in Thai). Wait for
                        // the typist to let go, and keep what arrives meanwhile.
                        pending.extend(capture.wait_for_modifier_release(Duration::from_millis(600)));
                        if capture.mods().any_held() {
                            // still holding it: refuse rather than mangle the word.
                            // Nothing has been deleted yet, so the text is intact.
                            println!(
                                "{} -> {}: holding the modifier, not typing (press again)",
                                committed.shown, committed.corrected
                            );
                            live.skipped += 1;
                            continue;
                        }
                        report(&committed);
                        apply_plan(
                            &committed,
                            injector.as_mut(),
                            &chord,
                            &probe,
                            dry_run,
                            args.debug,
                        );
                    }
                } else {
                    live.note_backspace();
                    live.on_backspace();
                    last_backspace = Some(Instant::now());
                }
                continue;
            }
            if fix_chord == config::FixChord::ShiftSpace
                && key.code == KeyCode::KEY_SPACE.0
                && mods.shift
            {
                // the chosen chord is on Space, so a shifted space is not a word
                // boundary: it is the fix. The application has already typed the
                // space - the plan deletes it along with the word.
                if let Some(committed) = live.on_fix_hotkey_via_space() {
                    pending.extend(capture.wait_for_modifier_release(Duration::from_millis(600)));
                    if capture.mods().any_held() {
                        println!(
                            "{} -> {}: holding the modifier, not typing (press again)",
                            committed.shown, committed.corrected
                        );
                        live.skipped += 1;
                        continue;
                    }
                    report(&committed);
                    apply_plan(
                        &committed,
                        injector.as_mut(),
                        &chord,
                        &probe,
                        dry_run,
                        args.debug,
                    );
                } else {
                    // nothing to fix: leave the space the application typed alone
                }
                continue;
            }
            if is_reset(key.code) {
                live.on_reset();
                continue;
            }
            if let Some(boundary) = boundary_char(key.code, mods.shift) {
                if let Some(committed) = live.on_boundary(boundary) {
                    report(&committed);
                    apply_plan(
                        &committed,
                        injector.as_mut(),
                        &chord,
                        &probe,
                        dry_run,
                        args.debug,
                    );
                }
                continue;
            }
            if let Some(ch) = produced(key.code, mods.shift, live.layout()) {
                live.on_char(ch);
            }
        }

        // The typist may switch layout with their own keys at any time - and
        // this also forgets the word they were about to fix, because the
        // decision for it belongs to the layout it was typed in.
        //
        // A fix of ours switches the layout too, and the probe thread only
        // re-reads the desktop every 400 ms: right after our own switch it can
        // still be holding the old value, and believing that would throw the
        // recalled word away after a perfectly good fix. So a disagreement has
        // to *persist* across two checks before it counts as theirs.
        if last_probe.elapsed() > Duration::from_millis(200) {
            last_probe = Instant::now();
            match probe.get() {
                Some(now) if now != live.layout() => {
                    let persisted = matches!(mismatch, Some((seen, since))
                        if seen == now && since.elapsed() >= Duration::from_millis(250));
                    if persisted {
                        mismatch = None;
                        live.note_user_switch(now);
                    } else if !matches!(mismatch, Some((seen, _)) if seen == now) {
                        mismatch = Some((now, Instant::now()));
                    }
                }
                _ => mismatch = None,
            }
        }
        if control.take_undo() {
            match live.undo_last() {
                Some(committed) => {
                    report(&committed);
                    apply_plan(&committed, injector.as_mut(), &chord, &probe, dry_run, args.debug);
                }
                None => println!("undo: nothing to put back (the word has moved on)"),
            }
        }
        if last_publish.elapsed() > Duration::from_millis(400) {
            last_publish = Instant::now();
            control.update(|status| {
                status.layout = live.layout();
                status.committed = live.committed;
                status.skipped = live.skipped;
                status.can_undo = live.can_undo();
                status.history = live
                    .history
                    .iter()
                    .map(|fixed| control::HistoryItem {
                        at: fixed.at,
                        shown: fixed.shown.clone(),
                        corrected: fixed.corrected.clone(),
                        kind: fixed.kind.to_string(),
                    })
                    .collect();
            });
        }
        sleep(Duration::from_millis(2));
    }
    control.stop();
}

/// `15 s`, or `never` when 0 turns the window off.
fn describe_window(seconds: u64) -> String {
    if seconds == 0 {
        "never".to_string()
    } else {
        format!("{seconds} s")
    }
}

fn report(committed: &live::Committed) {
    println!(
        "{} -> {}  ({} step(s))",
        committed.shown,
        committed.corrected,
        committed.steps.len()
    );
}

fn run_plan(
    steps: &[live::Plan],
    mut injector: Option<&mut inject::Injector>,
    chord: &[u16],
    probe: &Probe,
    dry_run: bool,
) -> Result<(), String> {
    // Nothing is deleted until we know the replacement can be typed.
    for step in steps {
        if let live::Plan::Type(text) = step {
            let table = table_for(live::layout_of(text));
            inject::Injector::can_type(text, table)
                .map_err(|(ch, why)| format!("cannot type {ch:?}: {why}"))?;
        }
    }
    for step in steps {
        let step_started = Instant::now();
        match step {
            live::Plan::Switch(target) => {
                if dry_run {
                    println!("  would switch the layout to {target:?}");
                    continue;
                }
                let Some(inj) = injector.as_deref_mut() else { continue };
                switch_verified(*target, chord, inj, probe, Duration::from_millis(900))?;
            }
            live::Plan::Backspaces(count) => {
                if dry_run {
                    println!("  would press Backspace {count}x");
                    continue;
                }
                if let Some(inj) = injector.as_deref_mut() {
                    inj.backspaces(*count).map_err(|e| e.to_string())?;
                }
            }
            live::Plan::Type(text) => {
                if dry_run {
                    println!("  would type {text:?}");
                    continue;
                }
                if let Some(inj) = injector.as_deref_mut() {
                    let target = live::layout_of(text);
                    injector_target(inj, text, target)?;
                }
            }
        }
        if std::env::var("RTLINUX_TIMINGS").is_ok() {
            eprintln!("    {:?} took {} ms", step, step_started.elapsed().as_millis());
        }
    }
    Ok(())
}

fn injector_target(
    injector: &mut inject::Injector,
    text: &str,
    target: InputLayout,
) -> Result<(), String> {
    let table = table_for(target);
    injector
        .type_in_layout(text, table)
        .map_err(|(ch, why)| format!("{why} (character {ch:?})"))
}
