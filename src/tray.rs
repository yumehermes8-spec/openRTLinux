//! System-tray application shell — Windows only.
//!
//! Hosts the keyboard hook on a thread that pumps a Win32 message loop (a
//! low-level hook requires one) and gives the user a tray icon + menu to enable/
//! disable RightType, switch Auto/Manual, and quit. The whole app is windowless
//! (`windows_subsystem = "windows"` in `main.rs`) so there's no console — it lives
//! quietly in the tray.

use std::rc::Rc;

use native_windows_gui as nwg;
use windows::Win32::Foundation::HWND;

use crate::{config, focus, hook, learn, overlay, session, settings, startup, stats};
use righttype::i18n::{tr, trf, T};
use righttype::per_app::AppMode;

thread_local! {
    /// Last tooltip pushed to the shell, so the timer refresh is a no-op
    /// unless something actually changed.
    static TIP: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    /// Which icon the tray shows (0 logo, 1 off, 2 TH, 3 EN), for the same
    /// reason.
    static SHOWING: std::cell::Cell<Option<u8>> = const { std::cell::Cell::new(None) };
    /// Language the menu was last labelled in.
    static MENU_LANG: std::cell::Cell<Option<righttype::i18n::Lang>> = const { std::cell::Cell::new(None) };
}

/// The tray icons, embedded so the binary stays portable (no external file):
/// blue while RightType is on, grey while it is off.
static ICON_BYTES: &[u8] = include_bytes!("../assets/icon.ico");
static ICON_OFF_BYTES: &[u8] = include_bytes!("../assets/icon_off.ico");
static ICON_TH_BYTES: &[u8] = include_bytes!("../assets/icon_th.ico");
static ICON_EN_BYTES: &[u8] = include_bytes!("../assets/icon_en.ico");

/// The tray icon shows the keyboard in use (TH / EN) instead of the logo
/// (opt-in, from the palette).
static SHOWS_LANGUAGE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn shows_language() -> bool {
    SHOWS_LANGUAGE.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn set_shows_language(on: bool) {
    SHOWS_LANGUAGE.store(on, std::sync::atomic::Ordering::Relaxed);
}

struct Tray {
    window: nwg::MessageWindow,
    icon: nwg::Icon,
    icon_off: nwg::Icon,
    icon_th: nwg::Icon,
    icon_en: nwg::Icon,
    _tray: nwg::TrayNotification,
    menu: nwg::Menu,
    m_enabled: nwg::MenuItem,
    pause: nwg::Menu,
    m_pause: [nwg::MenuItem; 3],
    m_resume: nwg::MenuItem,
    m_auto: nwg::MenuItem,
    m_manual: nwg::MenuItem,
    m_suggest: nwg::MenuItem,
    this_app: nwg::Menu,
    /// The app the submenu applies to (disabled; its name as the label).
    m_app_name: nwg::MenuItem,
    m_app_default: nwg::MenuItem,
    /// Auto, Suggest, Manual, Off.
    m_app_modes: [nwg::MenuItem; 5],
    m_learn: nwg::MenuItem,
    m_startup: nwg::MenuItem,
    m_fix: nwg::MenuItem,
    m_clean: nwg::MenuItem,
    m_keytest: nwg::MenuItem,
    m_settings: nwg::MenuItem,
    m_stats: nwg::MenuItem,
    m_help: nwg::MenuItem,
    m_tour: nwg::MenuItem,
    m_report: nwg::MenuItem,
    _sep: nwg::MenuSeparator,
    m_quit: nwg::MenuItem,
}

/// Pause lengths offered in the tray menu, in minutes.
const PAUSE_MINUTES: [u32; 3] = [10, 30, 60];
/// The per-app choices in the "In this app" submenu, in menu order.
const APP_MODES: [AppMode; 5] = [
    AppMode::Auto,
    AppMode::Suggest,
    AppMode::Manual,
    AppMode::Code,
    AppMode::Off,
];

fn item(parent: &nwg::Menu, text: &str) -> nwg::MenuItem {
    let mut item = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(text)
        .parent(parent)
        .build(&mut item)
        .expect("menu item");
    item
}

fn submenu(parent: &nwg::Menu, text: &str) -> nwg::Menu {
    let mut menu = nwg::Menu::default();
    nwg::Menu::builder()
        .text(text)
        .parent(parent)
        .build(&mut menu)
        .expect("submenu");
    menu
}

fn separator(parent: &nwg::Menu) {
    let mut sep = nwg::MenuSeparator::default();
    nwg::MenuSeparator::builder()
        .parent(parent)
        .build(&mut sep)
        .expect("sep");
}

/// An app mode's short name.
pub fn app_mode_name(mode: AppMode) -> T {
    match mode {
        AppMode::Auto => T::ModeAuto,
        AppMode::Suggest => T::ModeSuggest,
        AppMode::Manual => T::ModeManual,
        AppMode::Code => T::ModeCode,
        AppMode::Off => T::ModeOff,
    }
}

/// The label of an app mode in the "In this app" submenu.
fn app_mode_label(mode: AppMode) -> &'static str {
    tr(match mode {
        AppMode::Auto => T::TrayAuto,
        AppMode::Suggest => T::TraySuggest,
        AppMode::Manual => T::TrayManual,
        AppMode::Code => T::TrayCode,
        AppMode::Off => T::TrayAppOff,
    })
}

/// Build the tray UI, install the hook, and run the event loop until Quit.
pub fn run() {
    // Before any RightType window can take focus: the keyboard of the app
    // the user is in, for the English keyboard in use (see hook.rs).
    hook::remember_startup_layout();
    nwg::init().expect("Failed to init Native Windows GUI");
    crate::ui::load_fonts();
    crate::ui::refresh();

    // The overlay (status pill) lives on this thread; its window is created on
    // first use (see overlay.rs for why not now).
    overlay::register_ui_thread();

    // Load the learned-words dictionary, then restore saved settings (enabled +
    // mode + learn) before building the menu so its checkmarks reflect them.
    learn::load();
    config::apply(&config::load());
    // Build the dictionaries and compound tables now: built lazily they cost
    // tens of milliseconds on the first keystroke, inside the keyboard hook.
    righttype::english::warm();

    let mut window = nwg::MessageWindow::default();
    nwg::MessageWindow::builder()
        .build(&mut window)
        .expect("window");

    let mut icon = nwg::Icon::default();
    nwg::Icon::builder()
        .source_bin(Some(ICON_BYTES))
        .build(&mut icon)
        .expect("icon");
    let mut icon_off = nwg::Icon::default();
    nwg::Icon::builder()
        .source_bin(Some(ICON_OFF_BYTES))
        .build(&mut icon_off)
        .expect("icon");
    let mut icon_th = nwg::Icon::default();
    nwg::Icon::builder()
        .source_bin(Some(ICON_TH_BYTES))
        .build(&mut icon_th)
        .expect("icon");
    let mut icon_en = nwg::Icon::default();
    nwg::Icon::builder()
        .source_bin(Some(ICON_EN_BYTES))
        .build(&mut icon_en)
        .expect("icon");

    let mut tray = nwg::TrayNotification::default();
    nwg::TrayNotification::builder()
        .parent(&window)
        .icon(Some(&icon))
        .tip(Some("RightType"))
        .build(&mut tray)
        .expect("tray");

    let mut menu = nwg::Menu::default();
    nwg::Menu::builder()
        .popup(true)
        .parent(&window)
        .build(&mut menu)
        .expect("menu");

    let mut m_enabled = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(tr(T::TrayEnabled))
        .parent(&menu)
        .build(&mut m_enabled)
        .expect("enabled item");

    let pause = submenu(&menu, tr(T::TrayPause));
    let m_pause = [
        item(&pause, tr(T::TrayPause10)),
        item(&pause, tr(T::TrayPause30)),
        item(&pause, tr(T::TrayPause60)),
    ];
    separator(&pause);
    let m_resume = item(&pause, tr(T::TrayResume));
    separator(&menu);

    let mut m_auto = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(tr(T::TrayAuto))
        .parent(&menu)
        .build(&mut m_auto)
        .expect("auto item");

    let mut m_manual = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(tr(T::TrayManual))
        .parent(&menu)
        .build(&mut m_manual)
        .expect("manual item");

    let mut m_suggest = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(tr(T::TraySuggest))
        .parent(&menu)
        .build(&mut m_suggest)
        .expect("suggest item");

    let this_app = submenu(&menu, tr(T::TrayThisApp));
    let m_app_name = item(&this_app, tr(T::TrayNoApp));
    m_app_name.set_enabled(false);
    separator(&this_app);
    let m_app_default = item(&this_app, tr(T::TrayAppDefault));
    let m_app_modes = APP_MODES.map(|mode| item(&this_app, app_mode_label(mode)));
    separator(&menu);

    let mut m_learn = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(tr(T::TrayLearn))
        .parent(&menu)
        .build(&mut m_learn)
        .expect("learn item");

    let mut m_startup = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(tr(T::TrayStartup))
        .parent(&menu)
        .build(&mut m_startup)
        .expect("startup item");

    let m_fix = item(&menu, tr(T::TrayFix));
    let m_clean = item(&menu, tr(T::PaletteClean));
    let m_keytest = item(&menu, tr(T::PaletteKeyTest));

    let mut m_settings = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(tr(T::TraySettings))
        .parent(&menu)
        .build(&mut m_settings)
        .expect("settings item");

    let mut m_stats = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(tr(T::TrayStats))
        .parent(&menu)
        .build(&mut m_stats)
        .expect("stats item");

    let mut m_help = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(tr(T::TrayHelp))
        .parent(&menu)
        .build(&mut m_help)
        .expect("help item");

    let mut m_tour = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(tr(T::TrayTour))
        .parent(&menu)
        .build(&mut m_tour)
        .expect("tour item");

    let mut m_report = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(tr(T::TrayReport))
        .parent(&menu)
        .build(&mut m_report)
        .expect("report item");

    let mut sep = nwg::MenuSeparator::default();
    nwg::MenuSeparator::builder()
        .parent(&menu)
        .build(&mut sep)
        .expect("sep");

    let mut m_quit = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(tr(T::TrayQuit))
        .parent(&menu)
        .build(&mut m_quit)
        .expect("quit item");

    // Reflect current state in the menu checkmarks.
    m_enabled.set_checked(hook::is_enabled());
    m_auto.set_checked(hook::mode() == hook::Mode::Auto);
    m_manual.set_checked(hook::mode() == hook::Mode::Manual);
    m_suggest.set_checked(hook::mode() == hook::Mode::Suggest);
    m_learn.set_checked(learn::is_enabled());
    m_startup.set_checked(startup::is_enabled());

    let ui = Rc::new(Tray {
        window,
        icon,
        icon_off,
        icon_th,
        icon_en,
        _tray: tray,
        menu,
        m_enabled,
        pause,
        m_pause,
        m_resume,
        m_auto,
        m_manual,
        m_suggest,
        this_app,
        m_app_name,
        m_app_default,
        m_app_modes,
        m_learn,
        m_startup,
        m_fix,
        m_clean,
        m_keytest,
        m_settings,
        m_stats,
        m_help,
        m_tour,
        m_report,
        _sep: sep,
        m_quit,
    });

    let ui_h = ui.clone();
    let handler = nwg::full_bind_event_handler(&ui.window.handle, move |evt, _data, handle| {
        use nwg::Event as E;
        match evt {
            // Either click on the tray icon opens the menu at the cursor — a
            // left click used to do nothing, which read as "the app is not
            // responding". Refresh every checkmark first: hotkeys (mode
            // toggle, panic switch) change state without going through the
            // menu, so it can be stale.
            E::OnContextMenu | E::OnMousePress(nwg::MousePressEvent::MousePressLeftUp) => {
                sync_state(&ui_h);
                let (x, y) = nwg::GlobalCursor::position();
                ui_h.menu.popup(x, y);
            }
            E::OnMenuItemSelected => {
                if handle == ui_h.m_quit.handle {
                    nwg::stop_thread_dispatch();
                } else if handle == ui_h.m_enabled.handle {
                    let on = !hook::is_enabled();
                    hook::set_enabled(on);
                    ui_h.m_enabled.set_checked(on);
                    overlay::show(tr(if on { T::ToastOn } else { T::ToastOff }));
                    config::persist();
                } else if handle == ui_h.m_auto.handle {
                    hook::set_mode(hook::Mode::Auto);
                    ui_h.m_auto.set_checked(true);
                    ui_h.m_manual.set_checked(false);
                    ui_h.m_suggest.set_checked(false);
                    overlay::show(tr(T::ToastModeAuto));
                    config::persist();
                } else if handle == ui_h.m_manual.handle {
                    hook::set_mode(hook::Mode::Manual);
                    ui_h.m_auto.set_checked(false);
                    ui_h.m_manual.set_checked(true);
                    ui_h.m_suggest.set_checked(false);
                    overlay::show(tr(T::ToastModeManual));
                    config::persist();
                } else if handle == ui_h.m_suggest.handle {
                    hook::set_mode(hook::Mode::Suggest);
                    ui_h.m_auto.set_checked(false);
                    ui_h.m_manual.set_checked(false);
                    ui_h.m_suggest.set_checked(true);
                    overlay::show(tr(T::ToastModeSuggest));
                    config::persist();
                } else if let Some(i) = ui_h.m_pause.iter().position(|m| handle == m.handle) {
                    let minutes = PAUSE_MINUTES[i];
                    session::pause(minutes);
                    if session::is_paused() {
                        overlay::show(&trf(T::ToastPaused, &[("n", &minutes.to_string())]));
                    }
                } else if handle == ui_h.m_resume.handle {
                    if session::is_paused() {
                        session::resume();
                        overlay::show(tr(T::ToastOn));
                    }
                } else if handle == ui_h.m_app_default.handle
                    || ui_h.m_app_modes.iter().any(|m| handle == m.handle)
                {
                    if let Some(app) = crate::apps::last_app() {
                        let mode = ui_h
                            .m_app_modes
                            .iter()
                            .position(|m| handle == m.handle)
                            .map(|i| APP_MODES[i]);
                        crate::apps::set(&app, mode);
                        let label = match mode {
                            Some(mode) => app_mode_label(mode),
                            None => tr(T::TrayAppDefault),
                        };
                        overlay::show(&trf(T::ToastAppMode, &[("mode", label), ("app", &app)]));
                        config::persist();
                    }
                } else if handle == ui_h.m_learn.handle {
                    let on = !learn::is_enabled();
                    learn::set_enabled(on);
                    ui_h.m_learn.set_checked(on);
                    overlay::show(tr(if on {
                        T::ToastLearnOn
                    } else {
                        T::ToastLearnOff
                    }));
                    config::persist();
                } else if handle == ui_h.m_startup.handle {
                    let on = !startup::is_enabled();
                    startup::set_enabled(on);
                    ui_h.m_startup.set_checked(on);
                } else if handle == ui_h.m_fix.handle {
                    crate::fixer::open();
                } else if handle == ui_h.m_clean.handle {
                    crate::clean::request_open(crate::clean::Mode::Clean);
                } else if handle == ui_h.m_keytest.handle {
                    crate::clean::request_open(crate::clean::Mode::Test);
                } else if handle == ui_h.m_settings.handle {
                    settings::open();
                } else if handle == ui_h.m_stats.handle {
                    stats::open();
                } else if handle == ui_h.m_help.handle {
                    crate::onboard::show(false);
                } else if handle == ui_h.m_tour.handle {
                    crate::tour::start(crate::tour::Chapter::ALL[0]);
                } else if handle == ui_h.m_report.handle {
                    crate::report::save();
                }
                sync_state(&ui_h);
            }
            _ => {}
        }
    });

    // Session resilience: reinstall the hook across sleep/resume + lock/unlock by
    // watching raw power/session messages on this window (Bug 1).
    let hwnd = ui.window.handle.hwnd().map(|h| HWND(h as _));
    if let Some(h) = hwnd {
        TRAY_HWND.store(h.0 as isize, std::sync::atomic::Ordering::Release);
    }
    sync_state(&ui);
    if !config::onboarded() {
        crate::onboard::show(true);
    }
    #[cfg(debug_assertions)]
    if let Ok(what) = std::env::var("RIGHTTYPE_SHOW") {
        match what.as_str() {
            "settings" => settings::open(),
            "settings-hotkeys" => settings::open_page(2),
            "settings-learned" => settings::open_page(3),
            "settings-blocked" => settings::open_page(4),
            "settings-about" => settings::open_page(5),
            "settings-snippets" => settings::open_page(6),
            "settings-keyboard" => settings::open_page(7),
            "settings-tools" => settings::open_page(8),
            "settings-sponsor" => settings::open_page(9),
            "settings-activity" => settings::open_page(10),
            // One pill kept on screen, to look at: pill-done, -undo, -info,
            // -long, -attention, -problem.
            pill if pill.starts_with("pill-") => {
                use righttype::i18n::{trf, T};
                let text = match &pill[5..] {
                    "undo" => trf(T::ToastSpellingKept, &[("word", "kubernetes")]),
                    "info" => trf(T::ToastAppMode, &[("mode", "Auto"), ("app", "Chrome")]),
                    "long" => trf(T::ToastModeCode, &[]),
                    "attention" => trf(T::ToastEnterHeld, &[]),
                    "problem" => trf(T::ToastHookLost, &[]),
                    _ => trf(T::ToastFixedWords, &[("n", "3")]),
                };
                // The first call is on this (the UI) thread, which creates
                // the pill's window; the thread then keeps it on screen.
                overlay::show(&text);
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    overlay::show(&text);
                });
            }
            // Several different messages, then the Recent messages page.
            "activity-demo" => {
                use righttype::i18n::{trf, T};
                for text in [
                    trf(T::ToastModeAuto, &[]),
                    trf(T::ToastFixedWords, &[("n", "3")]),
                    trf(T::ToastSpellingKept, &[("word", "kubernetes")]),
                    trf(T::ToastEnterHeld, &[]),
                    trf(T::ToastHookLost, &[]),
                    trf(T::ToastHookBack, &[]),
                ] {
                    overlay::show(&text);
                }
                settings::open_page(10);
            }
            // tour-a ... tour-d, optionally ":<step>" (counting from 1)
            tour if tour.starts_with("tour-") => {
                let (name, step) = tour[5..].split_once(':').unwrap_or((&tour[5..], "1"));
                let chapter = match name {
                    "b" => crate::tour::Chapter::Yours,
                    "c" => crate::tour::Chapter::Keyboard,
                    "d" => crate::tour::Chapter::Privacy,
                    _ => crate::tour::Chapter::Fixing,
                };
                crate::tour::start(chapter);
                crate::tour::go_to(step.parse::<usize>().unwrap_or(1).saturating_sub(1));
            }

            "clean" => crate::clean::request_open(crate::clean::Mode::Clean),
            "keytest" => crate::clean::request_open(crate::clean::Mode::Test),
            "sheet" => crate::sheet::request_open(),
            "practice" => crate::practice::request_open(),
            "practice-game" => crate::practice::open_game_demo(),
            "stats" => stats::open(),
            "palette" => crate::palette::request_open(),
            "fixer" => crate::fixer::open_demo(
                "l;ylfu8iy[ hello \u{e41}\u{e19}\u{e1e}\u{e1e}\u{e33}\u{e41}\u{e30} answer PyThaiNLP lj'wa]N,k.shsojvp",
            ),
            "welcome" => crate::onboard::show(true),
            "help" => crate::onboard::show(false),
            "overlay" => overlay::show(righttype::i18n::tr(righttype::i18n::T::ToastModeAuto)),
            "keymap" => crate::keymap::request_toggle(),
            "by-app" => {
                use righttype::app_quality::Event;
                for (exe, fixed, right, wrong, undone) in [
                    ("notepad.exe", 42, 40, 0, 1),
                    ("chrome.exe", 31, 22, 0, 9),
                    ("winword.exe", 12, 3, 4, 0),
                    ("line.exe", 3, 0, 0, 0),
                ] {
                    for (e, n) in [
                        (Event::Fixed, fixed),
                        (Event::ShownRight, right),
                        (Event::ShownWrong, wrong),
                        (Event::Undone, undone),
                    ] {
                        for _ in 0..n {
                            crate::stats::app_event(exe, e);
                        }
                    }
                }
                crate::by_app::open();
            }
            "caret-list" => crate::caretlist::open_demo("arrow"),
            "caret-list-th" => crate::caretlist::open_demo("ลูกศร"),
            "caret-latex" => crate::caretlist::open_demo(r"\sum_{i=1}^{n} x_i^2 \leq \frac{1}{2}"),
            "caret-latex-bad" => crate::caretlist::open_demo(r"\frac{\frac{1}{2}}{3}"),
            "caret-grid" => crate::caretlist::open_demo(""),
            "caret-commands" => crate::caretlist::open_demo("window"),
            "caret-commands-armed" => crate::caretlist::open_demo_armed("close"),
            "caret-latex-cmd" => crate::caretlist::open_demo(r"\al"),
            "caret-math-short" => crate::caretlist::open_demo(
                "x^2 + (2x)/8 + x/2 +x^(3/2) +pi + sigma + Sigma_(i=1)^(n) X_i",
            ),
            "caret-math-word" => crate::caretlist::open_demo_math_app(
                "x^2 + (2x)/8 + x/2 +x^(3/2) +pi + sigma + Sigma_(i=1)^(n) X_i",
            ),
            "caret-macro" => crate::caretlist::open_demo(";rep"),
            "caret-math-words" => crate::caretlist::open_demo("x squared plus y squared"),
            "caret-math-thai" => crate::caretlist::open_demo("รากที่สองของ x บวก 1 ส่วน 2"),
            "ghost" => crate::overlay::offer_at(
                &hook::ghost_hint("!=", "≠"),
                crate::overlay::Anchor::Near(windows::Win32::Foundation::RECT {
                    left: 330,
                    top: 205,
                    right: 331,
                    bottom: 225,
                }),
            ),
            "ghost-words" => crate::overlay::offer_at(
                &hook::ghost_hint("x ยกกำลังสองบวก 1", "x² + 1"),
                crate::overlay::Anchor::Near(windows::Win32::Foundation::RECT {
                    left: 330,
                    top: 205,
                    right: 331,
                    bottom: 225,
                }),
            ),
            "keys" => {
                crate::onscreen::set_enabled(true);
                for k in ["Ctrl+C", "Ctrl+V", "Ctrl+V", "Alt+Tab"] {
                    crate::onscreen::show(k.to_string());
                    // One at a time: each waits for the message loop.
                    crate::onscreen::flush();
                }
            }
            "keymap-learnt" => {
                crate::practice::seed_demo();
                crate::keymap::request_toggle();
            }
            "badge" => overlay::badge_at(
                "TH",
                windows::Win32::Foundation::RECT {
                    left: 300,
                    top: 200,
                    right: 302,
                    bottom: 220,
                },
            ),
            "overlay-near" => overlay::show_at(
                "สวัสดี hello",
                overlay::Anchor::Near(windows::Win32::Foundation::RECT {
                    left: 300,
                    top: 200,
                    right: 302,
                    bottom: 220,
                }),
            ),
            _ => {}
        }
    }
    let ui_t = ui.clone();
    let raw = nwg::bind_raw_event_handler(&ui.window.handle, 0x5254_0001, move |_h, msg, w, _l| {
        unsafe { session::on_message(msg, w) };
        if msg == WM_ON_UI {
            match w {
                UI_REVIEW => crate::palette::open_review(),
                UI_FIX_WINDOW => crate::fixer::open(),
                UI_WELCOME_DONE => overlay::show(tr(T::WelcomeInTray)),
                _ => {}
            }
        }
        if msg == crate::instance::WM_INSTANCE {
            match w {
                crate::instance::HELLO_MSG => overlay::show(tr(T::ToastAlreadyRunning)),
                crate::instance::QUIT_MSG => nwg::stop_thread_dispatch(),
                _ => {}
            }
        }
        // The session retry timer doubles as a cheap refresh, so the tooltip
        // follows hotkey and Settings changes without waiting for the menu.
        if msg == WM_TIMER {
            sync_state(&ui_t);
            crate::rest::tick();
        }
        None
    })
    .ok();
    // The hook lives on this (message-pumping) thread.
    // Debug preview (RIGHTTYPE_PREVIEW): no keyboard hook, so a second copy
    // never touches what is typed.
    let preview = cfg!(debug_assertions) && std::env::var_os("RIGHTTYPE_PREVIEW").is_some();
    let installed = if preview {
        Ok(())
    } else {
        unsafe { hook::install() }
    };
    if let Err(e) = installed {
        nwg::modal_error_message(
            &ui.window.handle,
            "RightType",
            &format!("Failed to install the keyboard hook:\n{e}"),
        );
        return;
    }
    if let Some(hwnd) = hwnd {
        unsafe { session::arm(hwnd) };
        focus::set_notify_window(hwnd.0 as isize);
        crate::instance::listen(hwnd.0 as isize);
    }
    if let Some(notice) = crate::instance::take_notice() {
        overlay::show(&notice);
    }
    // UIA focus hook for password-field detection (incl. browsers).
    unsafe { focus::arm() };

    nwg::dispatch_thread_events();

    unsafe { focus::disarm() };
    crate::habits::save();
    crate::stats::save();
    if let Some(hwnd) = hwnd {
        unsafe { session::disarm(hwnd) };
    }
    unsafe { hook::uninstall() };
    if let Some(h) = raw {
        let _ = nwg::unbind_raw_event_handler(&h);
    }
    nwg::unbind_event_handler(&handler);
}

const WM_TIMER: u32 = 0x0113;

/// Posted to the tray window to have its thread (the one with the windows)
/// do something a worker cannot: `wparam` is one of the `UI_` values.
const WM_ON_UI: u32 = 0x8000 + 0x580;
/// Open the palette on the review waiting in `palette::request_review`.
pub const UI_REVIEW: usize = 1;
/// Open the Fix text window.
pub const UI_FIX_WINDOW: usize = 2;
/// Say RightType is running (the welcome window has just flown into the tray).
pub const UI_WELCOME_DONE: usize = 3;
static TRAY_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// The tray window's handle (0 before the tray is up), for finding its icon.
pub fn tray_hwnd() -> isize {
    TRAY_HWND.load(std::sync::atomic::Ordering::Acquire)
}

/// Ask the tray window's thread to do `what` (a `UI_` value), from any thread.
pub fn on_ui(what: usize) {
    let hwnd = TRAY_HWND.load(std::sync::atomic::Ordering::Acquire);
    if hwnd != 0 {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                HWND(hwnd as *mut _),
                WM_ON_UI,
                windows::Win32::Foundation::WPARAM(what),
                windows::Win32::Foundation::LPARAM(0),
            );
        }
    }
}

/// Refresh every state-bearing surface: the tray tooltip and the mode/enable/
/// learn checkmarks. Called before the menu opens, after any menu action, and
/// on the session timer.
fn sync_state(ui: &Rc<Tray>) {
    let enabled = hook::is_enabled();
    let healthy = session::is_healthy();
    let paused = session::pause_minutes_left();
    let state = if !healthy {
        tr(T::StateHookLost).to_string()
    } else if let Some(minutes) = paused {
        trf(T::StatePaused, &[("n", &minutes.max(1).to_string())])
    } else if enabled {
        tr(match hook::mode() {
            hook::Mode::Auto => T::ModeAuto,
            hook::Mode::Suggest => T::ModeSuggest,
            hook::Mode::Manual => T::ModeManual,
            hook::Mode::Code => T::ModeCode,
        })
        .to_string()
    } else {
        tr(T::StateOff).to_string()
    };
    let tip = format!("RightType — {state}");
    if TIP.with(|t| t.replace(tip.clone())) != tip {
        ui._tray.set_tip(&tip);
    }
    let working = enabled && healthy;
    let which = if !working {
        1
    } else if shows_language() {
        match hook::current_language() {
            Some(righttype::policy::InputLayout::ThaiKedmanee) => 2,
            Some(righttype::policy::InputLayout::UsQwerty) => 3,
            None => 0,
        }
    } else {
        0
    };
    if SHOWING.with(|c| c.replace(Some(which))) != Some(which) {
        ui._tray.set_icon(match which {
            1 => &ui.icon_off,
            2 => &ui.icon_th,
            3 => &ui.icon_en,
            _ => &ui.icon,
        });
    }
    let lang = righttype::i18n::lang();
    if MENU_LANG.with(|c| c.replace(Some(lang))) != Some(lang) {
        for (item, key) in [
            (&ui.m_enabled, T::TrayEnabled),
            (&ui.m_auto, T::TrayAuto),
            (&ui.m_manual, T::TrayManual),
            (&ui.m_suggest, T::TraySuggest),
            (&ui.m_learn, T::TrayLearn),
            (&ui.m_startup, T::TrayStartup),
            (&ui.m_fix, T::TrayFix),
            (&ui.m_clean, T::PaletteClean),
            (&ui.m_keytest, T::PaletteKeyTest),
            (&ui.m_settings, T::TraySettings),
            (&ui.m_stats, T::TrayStats),
            (&ui.m_help, T::TrayHelp),
            (&ui.m_tour, T::TrayTour),
            (&ui.m_report, T::TrayReport),
            (&ui.m_quit, T::TrayQuit),
            (&ui.m_pause[0], T::TrayPause10),
            (&ui.m_pause[1], T::TrayPause30),
            (&ui.m_pause[2], T::TrayPause60),
            (&ui.m_resume, T::TrayResume),
            (&ui.m_app_default, T::TrayAppDefault),
        ] {
            set_menu_text(item, tr(key));
        }
        for (item, mode) in ui.m_app_modes.iter().zip(APP_MODES) {
            set_menu_text(item, app_mode_label(mode));
        }
        set_submenu_text(&ui.pause, tr(T::TrayPause));
        set_submenu_text(&ui.this_app, tr(T::TrayThisApp));
    }
    ui.m_resume.set_enabled(paused.is_some());
    for item in &ui.m_pause {
        item.set_enabled(enabled || paused.is_some());
    }
    // "In this app": the app last typed in, and its mode.
    let app = crate::apps::last_app();
    let own = app.as_deref().and_then(crate::apps::lookup);
    set_menu_text(
        &ui.m_app_name,
        app.as_deref().unwrap_or_else(|| tr(T::TrayNoApp)),
    );
    ui.m_app_default.set_enabled(app.is_some());
    ui.m_app_default.set_checked(app.is_some() && own.is_none());
    for (item, mode) in ui.m_app_modes.iter().zip(APP_MODES) {
        item.set_enabled(app.is_some());
        item.set_checked(own == Some(mode));
    }
    ui.m_learn.set_checked(learn::is_enabled());
    ui.m_enabled.set_checked(enabled);
    ui.m_auto.set_checked(hook::mode() == hook::Mode::Auto);
    ui.m_manual.set_checked(hook::mode() == hook::Mode::Manual);
    ui.m_suggest
        .set_checked(hook::mode() == hook::Mode::Suggest);
}

/// Relabel a menu item (nwg has no setter): used when the language changes.
fn set_menu_text(item: &nwg::MenuItem, text: &str) {
    let Some((hmenu, id)) = item.handle.hmenu_item() else {
        return;
    };
    set_item_text(hmenu as _, id, text);
}

/// Relabel a submenu's entry in its parent menu. A popup entry's command id is
/// its submenu handle.
fn set_submenu_text(menu: &nwg::Menu, text: &str) {
    let Some((parent, sub)) = menu.handle.hmenu() else {
        return;
    };
    set_item_text(parent as _, sub as usize as u32, text);
}

fn set_item_text(hmenu: *mut core::ffi::c_void, id: u32, text: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetMenuItemInfoW, HMENU, MENUITEMINFOW, MIIM_STRING,
    };
    let mut wide: Vec<u16> = format!("{text}\0").encode_utf16().collect();
    let info = MENUITEMINFOW {
        cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
        fMask: MIIM_STRING,
        dwTypeData: windows::core::PWSTR(wide.as_mut_ptr()),
        ..Default::default()
    };
    unsafe {
        let _ = SetMenuItemInfoW(HMENU(hmenu as _), id, false, &info);
    }
}
