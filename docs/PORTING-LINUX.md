# RightType on Linux — porting notes and status

This tree is a **fork of upstream `k-affan-th/RightType`** (MIT OR Apache-2.0) with a
Linux OS layer added. Upstream's design is the reason this is possible at all: the
detection core is an OS-free library, so it compiles and passes its own tests here
untouched.

## Build and run

```bash
# rustup installs into $HOME; no root needed
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
export PATH="$HOME/.cargo/bin:$PATH"

cargo test  --lib --features linuxos          # 287 upstream tests + the Linux ones
cargo build --features linuxos --bin openrtlinux

./target/debug/openrtlinux --dry-run              # watch only: never types, never switches
./target/debug/openrtlinux                        # fix: switch the layout, then type
```

Useful flags: `--layout thai|us` (override the detected layout),
`--current-command 'CMD'`, `--switch-command 'CMD'`.

Requires read access to `/dev/input/event*` and write access to `/dev/uinput`
(`input` group or a uaccess ACL).

## What the Linux layer is

| Windows (upstream) | here |
| --- | --- |
| low-level keyboard hook (`hook.rs`) | `src/linuxos/capture.rs` — evdev |
| `SendInput` (`inject.rs`) | `src/linuxos/inject.rs` — uinput |
| `HKL` / `ActivateKeyboardLayout` (`policy` + `hook`) | `src/linuxos/layout.rs` — the desktop's own switch key + `mru-sources` |
| `GetForegroundWindow`, UI Automation, tray, at-caret UI | **not ported** (see below) |

The pipeline is `live.rs`: `WordBuffer` → `policy::detect_token` → a plan of
`Switch` / `Backspaces` / `Type` steps, which the binary executes through the
injector. `tools/linux/type_keys.py` and `tools/linux/watch_keys.py` are the test
harness (synthetic typing and a kernel-level view of what was injected).

## Tray and settings window

`openrtlinux-tray` (Python 3 + GTK3 + libayatana-appindicator — all already on the X13;
GTK *bindings* in Rust would need `libgtk-3-dev`, which needs a password here).
It drives the daemon over `/run/user/$UID/openrtlinux.sock`: one request per line, one
JSON reply per line.

* **Tray**: mode (auto / manual / off), the dry-run switch, "start daemon" when the
  socket is missing, the settings window, the journal, and stop/quit. The icon is
  `openrtlinux` normally and `openrtlinux-off` when the mode is off or the daemon is gone.
* **Settings window**: language, modes, dry-run, the fix hotkey, autostart, the undo
  button, the history, plus a live status line. Saving writes
  `~/.config/openrtlinux/config.toml` and asks the daemon to `reload` (mode and dry_run
  are also pushed, so a running daemon follows immediately).
* The desktop's own **switch key** and the **command that reads the active layout** are
  *not* on that screen (Suvasin, 2026-11-10: a typist never touches them). Their widgets
  are still built, loaded and saved, so `config.toml` keeps the values and Save cannot
  blank them - they are simply never attached to the grid, so `show_all()` cannot reveal
  them either. `--selftest` prints `hidden rows: switch_key, current_command` by checking
  each one has no parent, which is how this is verified over SSH; the values themselves
  are read back from the entries after `load_config()`.
* `openrtlinux-tray --report` prints the daemon's status (how to see what the tray sees
  over SSH); `--selftest` builds the widgets and the indicator and exits.
* GNOME has no tray of its own: `install.sh` turns on
  `ubuntu-appindicators@ubuntu.com`, otherwise nothing appears. Verify with the
  watcher, not by eye: `gdbus call --session --dest org.kde.StatusNotifierWatcher
  --object-path /StatusNotifierWatcher --method org.freedesktop.DBus.Properties.Get
  org.kde.StatusNotifierWatcher RegisteredStatusNotifierItems` should list
  `.../org/ayatana/NotificationItem/openrtlinux`.
* The config ships **`dry_run = true`**: a fresh install only watches and logs until
  the user turns dry-run off in the tray. Keep it that way - a first run that types
  would be a surprise nobody forgives.

## How it runs on the X13 (systemd user units)

`install.sh` installs `packaging/linux/openrtlinux.service` and `openrtlinux-tray.service`
into `~/.config/systemd/user/` and enables the daemon, so it is supervised and comes
back with the session instead of being an orphan of whatever shell started it:

```bash
systemctl --user status openrtlinux.service
journalctl --user -u openrtlinux -n 30
systemctl --user start|stop|restart openrtlinux.service
```

The tray asks systemd to start the daemon (`systemctl --user start openrtlinux.service`)
and only falls back to spawning it directly - spawning it left an unreaped child
behind (and an orphan that kept typing when the tray died).

## Verified so far

- `cargo test --lib` — **287 upstream tests pass on Linux**, untouched, plus the
  Linux-layer tests (292 with `--features linuxos`).
- Capture and decision, end to end on the target machine in `--dry-run`:
  physical `fvdw,h` + space was captured and the core answered
  `fvdw,h -> ดอกไม้` with the plan *switch to Thai → 7 backspaces → type "ดอกไม้ "*.
- Layout switching works from the daemon's own injector: the desktop's input
  source really moves (`gsettings ... mru-sources` went `us` → `th`), by pressing
  the user's own switch key (`grave` on this box). Writing the legacy
  `input-sources current` key does **not** work on GNOME 45+.
- The injector's synthetic input reaches the kernel (verified with
  `tools/linux/watch_keys.py`).

## Open, in order of importance

1. **The long-running binary did not see keys in the last live run** — the same
   capture code saw them in `--dry-run`, so this needs isolating (device list
   drift, non-blocking handling, or a stale binary). Until this is fixed, treat
   `--dry-run` as the trustworthy mode.
2. **Only boundary commits are implemented.** Upstream's best behaviour — the
   mid-word EN→TH anchor with its commit horizon (`policy::COMMIT_HORIZON`) and
   automatic withdrawal — needs the run read back out of the application, which
   is the hardest part of the port.
3. **No per-app rules and no at-caret features.** Wayland gives no foreground
   window to a normal client; this needs a GNOME Shell extension or the
   RemoteDesktop portal. Suggest mode, the palette, ghost suggestions,
   statistics, practice and the tray are not ported.
4. **Icons and a menu entry exist now** (`packaging/linux/`): `make_icons.py`
   draws 16-512 px PNGs (per size, never scaled down) plus `openrtlinux.svg` and the
   grey `--variant off` tile, `openrtlinux.desktop` names the app in Thai and English,
   and `install.sh` copies both states of every size into `~/.local/share/icons`
   and the entries into `~/.local/share/applications`. The tray and the settings
   window came later and are described above.

## The icon (v0.3.0)

The mark is the **key the program lives behind**: a light keycap with a filled shift
arrow on the rounded blue tile. It replaced the old `A` + `ก` + exchange-arc tile,
which was three elements at 16 px - the arrow and one glyph dissolved, and the two
glyphs that were left read as text, not as an app.

Four candidates were drawn and rendered natively at every size before choosing (a
paired mark, the keycap, a word-with-caret, a language toggle); the keycap won on the
panel sizes, where a silhouette beats a word. Notes that cost time:

* the arrow is a **filled polygon**, not the `⇧` font glyph - a glyph of that shape
  turns to mush below ~24 px, the polygon stays a shape;
* sizes <= 20 px get their own drawing (`tile()`): no key lip, a bigger cap, a
  thicker arrow;
* the generator draws the `off` (grey) state too, and `install.sh` installs *both*
  names - the tray asks for `openrtlinux-off` the moment the daemon stops, and a
  missing icon is a blank square in the panel (it was missing until v0.3.0);
* `docs/img/icon.png` (the README's) is written by the same generator, so the site
  cannot drift from the app;
* candidates and the contact sheet live in `~/Documents/openrtlinux-icons-4/`.

## Traps already paid for (do not rediscover)

- **One blocking device stalls all of them.** `fetch_events` on a device that is
  still in blocking mode waits for that device only, so every other keyboard goes
  unread. Set `set_nonblocking(true)` on each device.
- **Keyboards appear while you run.** A test tool (or any remapper) creates a
  virtual device after start-up; re-scan periodically or you will silently ignore
  it.
- **Do not require `KEY_A` to call something a keyboard.** A virtual device may
  declare only what it sends. Require space + enter plus a real alphabet instead.
- **Whitespace has no layout entry.** `Layout::key_of(' ')` is `None` — the core
  tables only map remapped keys, so resolve space/enter/tab separately.
- **Validate the whole replacement before deleting anything.** A plan that
  deletes the user's word and then cannot type the replacement destroys their
  text; `Injector::can_type` runs over every `Type` step first.
- **Never leave the fix-mode daemon running on someone's live session.** This box
  is used by a person; a daemon that types is a hazard, not a background service.
- **A line protocol needs a line.** The Python clients must send a trailing `\n`:
  the server reads with `lines()`, so a request without it just blocks until the
  client times out (symptom: every `rtctl status` "timed out" while the daemon is
  perfectly healthy).
- **GTK3 `GtkLabel` has no `xmin` property** (that is GTK4/Vala talk) - build a
  plain label and `set_xalign(0)` / `set_halign(START)`.
- **Installed launchers are copies, not links.** `~/.local/bin/openrtlinux-tray` is a
  copy of `gui/openrtlinux_tray.py`, so editing the repo changes nothing until
  `install.sh --no-build` runs again (a stale copy kept the old GTK bug alive
  through a whole test cycle).
- **`rsync -a` from a UTC box makes every source file look older than the build.**
  The droplet's clock is UTC and the X13's is UTC+7, so `-a` (which keeps mtimes)
  sends files that appear to predate `target/`, and **cargo silently skips the
  rebuild** - you then test a stale binary and chase bugs that were already fixed.
  Touch the sources after every rsync (`find src -name '*.rs' -exec touch {} +`) or
  rsync with `-I`.
- **Killing the local SSH job does not kill the remote daemon.** An
  `ssh host './openrtlinux'` started as a background job leaves an orphaned `openrtlinux`
  on the far side when the local process is killed — it keeps typing. Stop it on
  the remote side (`ssh host 'pkill -f "[r]tlinux"'`) and verify with `ps` before
  walking away.

## Behaviour Suvasin confirmed (2026-10-30)

* **Auto mode is the wanted behaviour**: type `fvdw,h` with the English layout on,
  press space, get `ดอกไม้ `. Confirmed working on the X13 in fix mode - keep auto as
  the shipped default.
* Manual mode (Shift+Backspace only) stays available for people who want to decide
  when a word is fixed; `off` and the dry-run switch stay for testing.

## The manual hotkey is a *modified* chord (Shift+Backspace)

Four separate faults made "fix the last word with Shift+Backspace" produce capitals
and scrambled letters. Each has a unit test now:

1. **Shift is still physically down when we inject.** Anything typed then arrives
   shifted: Latin in capitals, Thai as its upper-row glyphs. `Injector` therefore
   waits (up to 600 ms) for the modifier to be released
   (`Capture::wait_for_modifier_release`), and **refuses to type at all** if it is
   still held - nothing has been deleted yet at that point, so the word survives and
   the user can press the hotkey again.
2. **A virtual device only ever sends the codes it announced.** A test tool that
   writes an undeclared `KEY_LEFTSHIFT` sends nothing at all (the modifier never
   arrives, the chord looks like a plain Backspace). Declare modifiers in the uinput
   device, not just the keys you intend to press.
3. **The word is gone by the time the hotkey is pressed.** In manual mode the typist
   presses space first (which closes the word) and only then decides to fix it. The
   boundary used to empty the buffer, so the hotkey had nothing to look at; `Live`
   now remembers `(word, separator)` for exactly that, and the plan deletes the word
   plus the boundary's own Backspace and types the replacement with its space back.
4. **`/dev/input/eventN` is reused.** A device that goes away and comes back (a
   replug, a tool re-run) usually lands on the same path, and the daemon kept the
   **stale file descriptor** for it - reading nothing, forever, while the log looked
   perfectly healthy. `Capture` now compares the node's inode, replaces the handle
   when it changes, and drops devices that report ENODEV.

The tests live in `src/linuxos/capture.rs` (`any_held`, modifier recognition),
`src/linuxos/inject.rs` (an application-side simulator: typing with Shift held
scrambles the text, released it does not, whitespace is typeable, mixed scripts are
refused before anything is deleted) and `src/linuxos/live.rs` (the hotkey after a
space, the hotkey mid-word, and that a second press has nothing left to do).

## No space needed, and either key order works

Manual mode fixes the word in the buffer **without** a boundary: type `fvdw,h`, press
Shift+Backspace, get `ดอกไม้`. A space is only needed if the word was already
committed to the line (then the hotkey fixes that word, with its space typed back).

A fast chord can reach the kernel either way round - `Shift` then `Backspace`, or
`Backspace` then `Shift` (a few milliseconds apart, which is what a quick hand
produces). The second shape used to look like a plain Backspace: the character was
deleted and the chord's intent lost. `Live::note_backspace` remembers the word as it
was before that Backspace, and a Shift arriving within 120 ms runs
`on_fix_hotkey_after_backspace`, so both orders fix the word. Tested live on the X13:
both orders produced `fvdw,h -> ดอกไม้`.

## Speed: where a fix actually spends its time

Measured on the X13 with the daemon's own `applied in N ms` line (printed for every
fix, so the journal carries the number):

| build | word | time |
| --- | --- | --- |
| before | `-yhovyoouh -> ขั้นอันนี้` (7 backspaces + 9 Thai characters) | **434 ms** |
| after | `fvdw,h -> ดอกไม้` (6 backspaces + 6 Thai characters) | **27 ms** |

Three things were eating that time, none of them the detection:

1. **The layout probe ran inside the key loop.** Reading the active layout means
   spawning `gsettings`, tens of milliseconds each time, and it ran on the same
   thread that has to answer the typist. It now lives on its own thread
   (`Probe::start`) and the hot path only locks a small slot.
2. **The switch was verified once, not repeatedly.** `switch_verified` asked for a
   fresh read before the loop and then polled a value that only refreshed on the
   probe's own 400 ms schedule - so a switch that landed in 60 ms could still cost
   most of half a second. It now asks again every turn of the loop, which is what
   took 434 ms down to 27 ms.
3. **Injection was chatty.** Each key was two `emit` calls (press, release) with a
   3 ms sleep after it. Now one `emit` carries the press and the release, and the
   gap is 1 ms - still slow enough that an application's key-repeat logic sees every
   press. The daemon is also installed from the **release** build now (`--release`),
   not the debug one.

What is left is the wait for the typist: if Shift is still held when the fix is
ready, we wait for it to come up (up to 600 ms) and refuse rather than type shifted
characters. Holding Shift for a long time before releasing it is the one part of the
latency that belongs to the keyboard, not to us.

## One character is a fix too

Type `d` with the Thai layout selected and the screen shows `ก`; Shift+Backspace turns
it into `d` (and the other way round). Verified live on the X13:

```
ก -> d  (3 step(s))   applied in 21 ms
d -> ก  (3 step(s))   applied in 72 ms   # includes switching to Thai
```

A single character goes down a different path from a word: there is no word sense to
weigh up, so the correction is simply the other layout's reading of that character
(`layout::convert`, the same helper upstream uses for layout previews). Two rules keep
it from surprising anyone:

* only letters - and Thai, which includes its vowel marks, so `ุ` still fixes to the
  `u` that was meant. A digit or a comma is itself in both layouts; flipping one would
  be a surprise, not a fix, so it is counted as skipped and nothing is deleted;
* never delete more than the hotkey already did. The application's own Backspace has
  already removed the character, so the remaining count for a one-character word is
  **zero** - the `.max(1)` that was there for multi-character words would have eaten
  the character *before* it.

## How short a word the hotkey can rescue (measured)

`tests/short_words.rs` puts **every word in both dictionaries** through the wrong
layout and counts what the hotkey fixes, plus the opposite direction: a word typed
*correctly* that the rule would flip anyway. Run it with
`cargo test --features linuxos --test short_words -- --nocapture`.

85,000+ word/layout pairs, all lengths 2-7:

* the word checker alone (what the daemon used to do): **97.0% rescued, 0% damage**
* plus the deliberate-request reading below: **99.3% rescued, 0% damage**
* a rule that only asks "is the other reading a word?" (no check on what is on
  screen): 100% rescued but **0.7% damage overall - 29% of correctly typed
  two-character words**. Measured, rejected.

Per length (rescue / damage), word checker -> with the second opinion:

| characters | word checker | with second opinion |
| --- | --- | --- |
| 2 | 50.5% / 0% | ~71% / 0% |
| 3 | 65.3% / 0% | ~96% / 0% |
| 4 | 99.7% / 0% | 100% / 0% |
| 5-7 | 100% / 0% | 100% / 0% |

So a deliberate press is a strong enough signal to take a second opinion, but not
strong enough to guess: short tokens are genuinely ambiguous (two characters are
words in both languages often enough), and refusing beats flipping something that was
already right. The **auto** path (on a boundary) keeps the stricter word checker -
nothing there was asked for.

Live on the X13, all of these were the word checker's refusals before:

```
ก -> d     (3 step(s))   applied in 16 ms
869 -> คุต  (3 step(s))   applied in 20 ms
d -> ก     (3 step(s))   applied in 72 ms   # includes switching to Thai
```

## The fix chord is a choice, and short words flip

**The chord.** `fix_word_hotkey` in `config.toml` is now one of `shift+backspace`
(default) and `shift+spacebar`, and the settings window offers it as a dropdown. The
daemon prints the chord it is using at startup and picks up a change on `reload` (what
the window sends after saving). A shifted Space is *not* a word boundary when it is the
chosen chord: the application has already typed a space by the time we see the key, so
the plan deletes the word **plus that space** and types the correction without a
trailing space.

**Short words.** The deliberate press now has three answers, in order of knowledge:

1. the word checker (compounds, trailing marks);
2. the other layout's reading, if that is a word and the screen is not;
3. the other layout's reading, period - for a token of one to three characters, where
   no dictionary can judge anything ("dd" is in the English list and "กก" is in the
   Thai one, so both verdicts are noise), and for any longer string that is not a word
   where it sits (a name or a loanword typed in the wrong layout).

A longer string that *is* a word stays put, so an accidental press cannot mangle
correct text. Suvasin's reported cases, live on the X13:

```
dd -> กก        (3 step(s))   applied in 13 ms
เเ -> gg        (3 step(s))   applied in 13 ms
fvdw,h -> ดอกไม้  (3 step(s))   applied in 22 ms    # chord: Shift+Backspace
fvdw,h -> ดอกไม้  (3 step(s))   applied in 24 ms    # chord: Shift+Spacebar
```

The auto path (fixing on a boundary) is unchanged: nothing there was asked for, so it
keeps the strict word checker only.

## Where this lives on GitHub

The published repository is **<https://github.com/yumehermes8-spec/openrtlinux>** - our own
repository, not a GitHub fork (a fork would have been tied to upstream's repository and
could not be renamed or reorganised freely; the upstream history is kept instead, so
their commits merge in normally).

```
origin    https://github.com/yumehermes8-spec/openrtlinux       <- ours
upstream  https://github.com/k-affan-th/RightType           <- theirs (for merges)
```

Two working copies on the X13, on purpose:

* `~/Cursor/righttype-ubuntu` - where the daemon is built and installed from
  (`packaging/linux/install.sh`), and what the systemd units run;
* `~/Cursor/righttype-publish` - the clone that is attached to GitHub. Its `main` is
  upstream's history plus one commit containing the whole Linux port.

Publishing rule learned the hard way: **never rsync a whole working tree over an
upstream clone.** A copy taken days earlier carries older versions of files upstream has
since changed, and copying it wholesale silently reverts their work (it did: eleven
files, including `src/hook.rs`). Copy only what this port actually adds or changes, and
check `git status` before committing.

## A remembered word has a window, and the cursor invalidates it (2026-11-10)

The fix chord reaches for a word that is *already on screen*, so the daemon has to
remember it. Two memories did that - the word a boundary closed (`pending`) and the
newest fix (`swap`, what `both` swaps back) - and neither expired. This process sees the
keyboard only: no pointer, no window focus. A word remembered from another document was
therefore deleted at whatever cursor was there when the chord was finally pressed,
minutes later, and the corrected text was typed over it.

The rule now (Suvasin's numbers):

* `recall_seconds = 15` - a word a boundary closed stays reachable for the chord for
  15 s; after that the press does nothing (it counts in `skipped`; nothing is deleted);
* `undo_seconds = 60` - the newest fix can be swapped back or undone for a minute,
  because a wrong *fix* is noticed later than a wrong word;
* `0` in either key means "never forget" - the pre-TTL behaviour, kept for the record;
* both windows run on a monotonic millisecond clock (`Live::set_now`, fed from one
  `Instant` in the daemon loop), so a clock step or a suspended laptop cannot expire a
  word early - and the unit tests move that clock by hand instead of sleeping;
* what the daemon *can* see forgets the word at once: Escape, an arrow, Home/End,
  PageUp/Pagedown, Delete (`is_reset`) and a deliberate layout switch (`note_user_switch`);
* the word *being typed* (`buf`) never expires: it is anchored to the cursor;
* `can_undo()` applies exactly the guards `swap_last_fix` does, age included, so the tray
  button cannot light up for a fix that can no longer be put back.

`linuxos::live::recall_window_tests` covers the edges: at the limit, one millisecond
past it, Escape at once, a switch at once, mid-word never, undo at 60 s and past it.

Note for later: only `both` (and a chord fix in `manual`) keeps a `swap`, so an
*automatic* fix in `auto` mode cannot be undone from the window today.

## A repeated press is a toggle, and the counting has to match the screen (2026-11-10)

`both` mode's hotkey toggles the two spellings, so it can be pressed again and again.
Every press arrives *after* the application has already run its own Shift+Backspace: the
daemon never sees the character that keystroke ate, it has to be told. The first press
was told (`pre_deleted = 1`); the swap was not - it deleted `from.len()` assuming the
keystroke had eaten the boundary. Mid-word there is no boundary, so the keystroke had
eaten a character of the *word*, and every press after the first deleted one character
too many: the word got glued to the text before it, then ate into it.

The rule now, in `swap_last_fix(pre_deleted)`: delete `from.len() + separator.len() -
pre_deleted` and type `to + separator`. The chord passes 1, the settings window's Undo
button passes 0 (nothing was deleted first) - which also stops that button from leaving a
double space behind.

`linuxos::live::chord_repeat_tests` stands a `String` in for the focused text box and
applies the plans to it, including the application's own keystroke, then presses the
chord three and ten times and checks the whole sentence - not just the word. Ten presses
used to leave the text a character shorter each time; that is the test to keep.

## My words, and a press that always converts (2026-11-10)

Two things Suvasin asked for after using it: a place for *his* vocabulary (some programs
are not about the layout at all), and a fix hotkey that fixes without judging.

**The chord converts, always.** `Live::decide` used to refuse when the word on screen was
a real word ("an accidental press must not mangle it"). That guard belongs to the
automatic path, where nobody asked: the press is a deliberate act, so it now takes the
word checker's answer if it has one and otherwise the other layout's reading, period -
real word or not. The boundary path still refuses real words (`tests::english_words_are_left_alone`).

**The typist's own words** live in `~/.config/openrtlinux/words.toml`, which both the
daemon and the settings window edit:

```text
learn = true
word = kubernetes
pair = frdw,h=ดอกไม้
```

* `word` - never fix this one. Three ways in: typed by hand in the settings window,
  [`Words::observe`] after the same English word is left alone at three boundaries (only
  with `learn = true`), or `keep_as_typed` when the typist reverses one of our fixes - a
  swap or the Undo button is an explicit "that spelling was right", so it is learned even
  with `learn = false`.
* `pair` - `what I typed=what it should be`. Checked before every other rule, applied at a
  boundary *and* on the chord: an exact string the typist typed is not a guess.
* The words join the live dictionaries (`Dictionary::learn`), so the automatic path, the
  second opinion and the chord all see them with no further change. Guards before a word
  is kept: letters of one script, 3-20 (Thai 2-30) characters, not ALL CAPS, not shaped
  like code (`code::looks_like_identifier`), not already in the bundled or tech lists.

**Repeated-key lines, not TOML.** `word = …` may appear many times; there is nothing to
escape in Thai, and a half-written line costs one word instead of the file. Both writers
(the daemon appending what it learned, the window editing) re-read the file before
writing, so the last writer never clobbers the other.

**A locked screen disables user extensions, and that looks like a bug.** GNOME 45+ runs
extensions in `user` session mode only: while the lock screen is up, `quick-lang-switch`
(which makes the switch key fast) and `ubuntu-appindicators` (which draws the tray) are
**INACTIVE**. The switch key then does nothing, so a fix that has to switch the layout
fails with `could not apply it: the desktop did not switch to ThaiKedmanee` in the
journal, and the tray icon is missing. Check `gnome-extensions info <uuid> | grep State`
and `org.gnome.ScreenSaver.GetActive` before hunting for a bug in the daemon.

[`Words::observe`]: crate::linuxos::words::Words::observe

