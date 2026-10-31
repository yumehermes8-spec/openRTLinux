# RightType — full guide

> This is the detailed guide. For the short overview and download, see the [main README](../README.md).

A modern, safe **Rust** successor to [RightLang](https://www.rightlang.com/) — it fixes
text typed in the **wrong keyboard layout** (Thai Kedmanee typed while English QWERTY
is active, or vice-versa) **without retyping**.

> Type `correct` while Thai is active and you get `แนพพำแะ`. RightType turns it back
> into `correct` — automatically as you type, or on a hotkey.

## Why a rebuild?

RightLang is excellent but unmaintained, and its design causes real bugs:

| RightLang bug | RightType fix |
| --- | --- |
| Dies after the PC wakes from sleep (must kill & reopen) | Reinstalls the keyboard hook on power/session events, with one delayed retry |
| Manual switch sometimes emits `ggg…` garbage | Releases held modifiers + injects one atomic Unicode batch (never replays keys) |
| Dropped/reordered characters in Word | Injects raw Unicode codepoints — no layout switching, no race |

…plus a first-class **privacy/security layer** (see below) and a tiny footprint.

## How it works

| Settings | Apps: a mode per app | Statistics |
| --- | --- | --- |
| ![Settings, General page](img/settings.png) | ![Settings, Apps page in dark mode](img/apps-dark.png) | ![Statistics with the last 7 days](img/stats.png) |
| **Fix text** (Thai interface) | **Command palette** | **Hotkeys you can change** (Thai interface) |
| ![Fix text window](img/fix-text-th.png) | ![Command palette](img/palette-dark.png) | ![Settings, Hotkeys page](img/hotkeys-th.png) |

RightType lives in the **system tray** (no window, no console). Click the tray icon
(left or right) for: **Enable**, **Pause** (10 minutes, 30 minutes or an hour —
it switches itself back on), **Manual/Auto/Suggest** mode, **In this app** (a mode
of its own, or off, for the app you were just typing in), **Learn new words**,
**Start with Windows**, **Fix text** (paste a paragraph typed in the wrong layout and
copy it back fixed), **Settings**, **Statistics**, **Hotkeys & help**, **Save a problem report**
(what RightType did lately, to attach to a bug report — words appear only as
letter counts, never as text), and **Quit**.
The tooltip shows the current mode, and the icon turns grey while RightType is off.

**Settings** has six pages — General (mode, on/off, start with Windows, learning,
fixing common Thai misspellings),
Hotkeys, Learned words (see, add, remove, clear, import or export what RightType
has learned, or keep the list — and, if you like, your settings and snippets — in a
sync folder such as OneDrive), **Snippets** (a short trigger such as `;addr` and
Space becomes a longer text, on the Thai keyboard, the English one or either),
Apps (a **table** of every app with a mode of its own — pick a mode or right-click a
row; **Code mode** is the default in code editors: names are never touched and Thai
appears only in comments and strings — plus an opt-in **guess each field's
language** that switches the keyboard as you click into a field you always use for
one language — only word counts are kept) and Privacy & about (with a **Check for
updates** button that opens the Releases page in your browser — RightType itself
never goes online — and **start again after a crash**). Changes take effect the moment you make
them. The interface is in **English or Thai** (it follows the Windows display
language; switch it at the bottom of the Settings sidebar), follows the Windows
**light/dark** app theme, and stays sharp at any display scaling.

**Manual mode (default)** — type freely; when you notice a wrong-layout word, fix it
with a hotkey:

| Hotkey | Action |
| --- | --- |
| `Shift`+`Backspace` | Convert the last word in place — or, right after Auto changed a word, flip it back. Press it again to flip the word before as well (up to 8 words). Each press says what it did; see [`docs/UNDO.md`](UNDO.md) |
| `Shift`+`CapsLock` | Convert the current **selection**: read from the app through UI Automation (never through the clipboard, which Windows can keep in its history and sync to your phone), then typed back as Unicode. Apps that do not share their selection get a message; `selection_via_clipboard = true` in `config.toml` lets RightType copy it instead |
| `Ctrl`+`CapsLock` | Cycle **Manual** → **Auto** → **Suggest** (works in every app, including ones RightType otherwise stays out of; in an app with its own mode, cycles that app's mode) |
| `Tab` or `Alt`+`CapsLock` | Accept the current Suggest hint (Tab only right after it appears; otherwise Tab is Tab) |
| `Ctrl`+`Shift`+`CapsLock` | Undo the last correction (selection undo requires the same focused context) |
| `Ctrl`+`Alt`+`CapsLock` | Enable/disable RightType immediately |
| `Ctrl`+`Alt`+`Space` | Command palette: your recent words (tick several with Space — they are tinted in the app — and Enter flips exactly those), **fix this field** (the words are listed and tinted first; untick any), fix only the wrong-keyboard words of a selection, Thai text in standard form, year พ.ศ. ↔ ค.ศ., numbers and baht amounts in words, special characters by name, **type the copied text key by key** (remote desktops, VMs), pause, off in this app or **this field**, switch mode, settings |
| `Ctrl`+`Backspace` | After Thai: deletes one Thai word, not the whole run back to the last space |
| `Shift`+`Backspace` held | Flips the rest of the words in one go |
| `Ctrl`+`Alt`+`K` | Keyboard map: click a key to type it |

Every hotkey can be changed in Settings → Hotkeys (click **Change**, press the new keys).

**Auto mode** — direction-aware, built for how each language is actually written:

- **EN → TH (instant)**: the moment your in-flight keystrokes form a known Thai
  word with high confidence, RightType fixes it **and switches to Thai** — your
  remaining keystrokes finish the word natively. No spaces required, none
  inserted: Thai doesn't work that way. It **holds** while the same keystrokes
  could still become an English word (`diffe` is on its way to `different`), so
  ordinary English typing is never converted mid-word — and for a few keystrokes
  after a conversion it keeps watching, putting the letters back by itself if the
  run turns out not to be Thai after all.
- **TH → EN (at whitespace)**: English *is* space-delimited, so the token
  commits when you hit space.
- **The whole word gets the last say.** A conversion made mid-word is not final:
  when you reach the space, RightType looks at the *entire* word — including the
  part typed after it switched to Thai — and if your keystrokes spell English it
  puts the whole word back and returns to English. (Before 1.1.0 only the part after
  the switch was judged, which could leave half-Thai, half-English words.)
- English is more than the dictionary: compounds of everyday words
  (`middleware`, `workflow`, `frontend`, `codebase`) and words you have taught it
  are treated as English everywhere.
- Ambiguous prefixes (a short valid word that begins a longer one) resolve in
  your favour as you keep typing; `Shift`+`Backspace` and Undo cover the rare
  miss. It deliberately does not destructively convert mid-word runs that aren't
  fully-known Thai — those stay available to Manual and Suggest.

**Suggest mode** uses the same rules as Auto but only shows the fix next to the
text cursor — as soon as the keys typed so far clearly spell it, not only at the
space. It changes text only when you take it (`Tab` right after it appears, or
`Alt`+`CapsLock`), and drops the hint when focus, layout, mode, or typing
context changes. A small `TH` / `EN` tag also flashes at the cursor whenever
RightType switches the language.

**Learning** (opt-in, "Learn new words") — an English word you type three times is
remembered, and so is any word you *flip back* after RightType changed it
(`Shift`+`Backspace` or Undo), immediately and in either language. Learned words
take effect at once for every decision.

**Supported keyboards.** RightType finds out which keyboard you are typing on
from Windows itself, from the keyboard you are typing on; there is nothing to
choose in Settings (Settings → Keyboard shows what it found, for example "Detected: Thai
Kedmanee · English Dvorak").

| Language | Keyboard | How it is recognised | Limits |
| --- | --- | --- | --- |
| Thai | Kedmanee (with or without ShiftLock) | Windows layout `KBDTH0` / `KBDTH2` | — |
| Thai | Pattachote (with or without ShiftLock) | `KBDTH1` / `KBDTH3` | — |
| Thai | Manoonchai | a keyboard installed from file whose name says Manoonchai | — |
| English | US keyboard, including English of other countries that uses it (Australia, New Zealand, Canada set to US, …) | `KBDUS` | — |
| English | UK keyboard | `KBDUK` | — |
| English | Dvorak (US) | `KBDDV` | Practice and Keyboard map windows show the US board; a lone `.` `?` `"` flipped by hand is read as on US; punctuation rules are tuned for US |
| English | Colemak | an installed keyboard whose name says Colemak | same limits as Dvorak |

**Not supported, and RightType stays off while one is active:** left- and
right-handed Dvorak, Colemak-DH and other modified Colemaks, any keyboard whose
keys sit elsewhere than on the US or UK keyboard (German, French, Canadian
French, Irish, …), other languages, and any Thai keyboard Windows cannot
identify. It never guesses a table: wrong guesses would corrupt what you type.
If Thai keyboards are installed but none of them is recognised, nothing is
converted into Thai either. Keyboards are read when RightType starts, so one
you add later is picked up after a restart. The Thai keyboard you used last is
remembered for the next start.
Both a Thai and an English keyboard can be installed in several kinds at once
(for example Kedmanee and Pattachote); the one you are typing on is used.

Hotkeys can be changed in Settings → Hotkeys, and each app
can have its own mode (Settings → Apps).

**At the text cursor** (2.4):

- Press **Shift twice** for a list where you type: any character by name in
  Thai or English, your snippets, and the app's own commands (Enter runs one
  and shows its shortcut for next time). Empty, it is a grid of the
  characters you pinned and used lately.
- **Ghost suggestions**: `->`, `!=`, `x^2`, `\alpha` and math said in words
  (`x ยกกำลังสองบวก 1`) show what they can become; **Tab** writes it.
- **Math**: said in words (Thai or English), written short like Word's
  equation editor (`x^(3/2) + (2x)/8 + Sigma_(i=1)^(n) X_i`), or LaTeX —
  as Unicode anywhere, or in Word, PowerPoint and OneNote as a real equation
  or with superscript formatting.
- **Tap Right Alt** after a character for its others (`2` → ² ₂ ⅔,
  `e` → é è ê ë); **Ctrl+P** in the list pins one to a key.
- **Macros**: snippets with steps — `{กด Ctrl+B}`, `{รอ 300}`,
  `{คำสั่ง ตัวหนา}`, `{สไตล์ Heading 1}`, `{คลิปบอร์ด}`.

**Keyboard tools** (Settings → Tools, or the command palette `Ctrl`+`Alt`+`Space`):

- **Typing practice** for Kedmanee, Pattachote, Manoonchai or English: a row of
  keys at a time, practice text made of real words, 60-second rounds and a
  falling-words game. It works without the keyboard installed, and nothing typed
  there reaches any app. While you move to a new keyboard, its map can stay on
  screen, and the keys you know fade.
- **Clean the keyboard** (every key held for up to 2 minutes), a **key tester**,
  and a **health check** for stuck keys, Sticky Keys, keys that type twice and
  keyboard settings that cause trouble.
- **This app's shortcuts**, searchable in Thai or English; **shortcuts in
  English** while the Thai keyboard is on (opt-in); **hold a key** for … – ฿ ฯ
  and accents (opt-in); **show shortcuts on screen** for recordings (opt-in,
  never letters); a **rest reminder** (opt-in).
- **Barcode scanners** work with the Thai keyboard on, and a device that types a
  command faster than any hand is held back.

![Typing practice](img/practice-th.png)

Short, genuinely ambiguous words (e.g. `ok` vs Thai `นา`, which share keys) are left for
you to fix manually — no tool can resolve those without guessing.

The same limit applies to words RightType has never seen. A **typo or a name**
that is not in the English dictionary cannot be recognised as an unfinished
English word, so Auto mode may start converting one — but that decision stays
under review for the next few keystrokes and undoes itself as soon as the Thai
reading stops making sense. What survives is the narrow case where a mistyped
word's *entire* conversion keeps reading as valid Thai; `Shift`+`Backspace` flips
the whole word back — and, with learning on, remembers it so it does not happen
again.

## Privacy & security (built for sensitive typing)

A keyboard tool sees everything you type. RightType is designed so secrets never leak:

- **Sensitive-context guards** — it disables itself entirely in **password fields**
  (native `ES_PASSWORD`, and browser/Electron/UWP fields via UI Automation) and in a
  default blacklist of **wallets, password managers, terminals, remote
  desktops and virtual machines** (the keys belong to another computer), and
  it stays out of **full-screen games** (`wasd` spells Thai ไฟหก).
- **Secret-shaped bail-out** — even elsewhere, identifiable private keys and addresses
  are always ignored. Consecutive BIP39 words trigger a phrase-level stream guard.
  Password-like/long ASCII can be wrong-layout Thai;
  it is eligible only when the complete conversion is fully-known Thai, and it is never
  sent to the learning/persistence path.
- **Minimal in-memory footprint** — it holds only the word in progress and, for
  flipping back, the last few words (at most 8, dropped on a click, arrow key or
  window change); all are **zeroized** when dropped, and it runs **non-elevated**.
- **No telemetry, zero network code** — verifiable in `Cargo.lock`; there is no HTTP,
  update, or analytics crate anywhere in the dependency tree.
- **Nothing typed is written to disk** — the only files are app settings
  (`%APPDATA%\RightType\config.toml`, with the snippets you write yourself) and,
  only if you turn them on: the learned
  words (`learned.txt`, "Learn new words"), two numbers per day for the 7-day
  chart (`stats.toml`), per-field word counts for guessing a field's language
  (`contexts.toml`) and typing-practice scores (`practice.txt`: a score per
  day and two counts per key, never the text). All are **off by default**, and learning never runs in the
  sensitive contexts above.

The detailed data lifetimes, controls, and known residual risks are documented in
[`docs/THREAT_MODEL.md`](THREAT_MODEL.md). In particular, ordinary English
words overlap the BIP39 list, so phrase-level protection is contextual/stream-based
rather than a blanket ban on every individual BIP39 word.

## For end users

### ติดตั้ง (ไม่ต้อง build เอง)

1. ไปที่หน้า **[Releases ล่าสุด](https://github.com/k-affan-th/RightType/releases/latest)**
   แล้วดาวน์โหลด `RightType-<เวอร์ชัน>-setup.exe`
2. ดับเบิลคลิกไฟล์ ถ้า Windows ขึ้น "Windows protected your PC" ให้กด
   **More info → Run anyway** (ไฟล์ยังไม่ได้ลงลายเซ็นดิจิทัล)
3. เลือกได้ว่าจะสร้าง shortcut และเปิดพร้อม Windows ไหม — ไม่ต้องใช้สิทธิ์ admin
4. RightType อยู่ที่มุมขวาล่างของจอ (ไอคอน **Aก**) คลิกเพื่อเปิดเมนูและหน้าตั้งค่า

ถอนการติดตั้ง: Settings → Apps → RightType

### Install (no building needed)

**Installer (recommended, per-user, no admin):** download
`RightType-2.6.5-setup.exe` from the
[latest release](https://github.com/k-affan-th/RightType/releases/latest) and run it.
It installs RightType, offers a desktop shortcut and start-at-login, and registers
a normal Windows uninstaller (Settings -> Apps -> RightType).

**Windows on ARM** (Surface Pro X, Copilot+ PCs): use `RightType-2.6.5-arm64-setup.exe`
or `RightType-2.6.5-arm64.zip`.

**Portable zip:** extract `RightType-2.6.5-x64.zip` and run `righttype.exe` where it
sits. Put an empty file named `portable` next to it and settings and learned
words stay in a `data` folder beside it (for a USB stick) instead of
`%APPDATA%\RightType`. Or install it per-user from the extracted folder:

```powershell
powershell -ExecutionPolicy Bypass -File install.ps1 -Autostart
```

- Installs to `%LOCALAPPDATA%\RightType`, adds a **Start Menu shortcut**, and
  (with `-Autostart`) launches at login.
- Uninstall anytime: `powershell -ExecutionPolicy Bypass -File uninstall.ps1`
  (add `-KeepSettings` to preserve your config/learned words).
- Verify what you downloaded against `SHA256.txt` before running it:

```powershell
Get-FileHash -Algorithm SHA256 .\RightType-2.6.5-setup.exe
```
- Unsigned builds show a SmartScreen prompt — "More info → Run anyway".
  Code signing is planned for a later release.

## For developers

Requires the Rust **MSVC** toolchain and the Visual Studio C++ Build Tools (Desktop C++).

```sh
# Pure-logic core (no OS calls) — runs anywhere:
cargo test

# Full Windows app:
cargo run --features winos
cargo build --release --features winos

# Installer + portable zip + SHA256.txt into dist\ (version from Cargo.toml):
pwsh -File packaging\build_release.ps1
```

Changes per version are in [`CHANGELOG.md`](../CHANGELOG.md).

**Releasing** needs no Windows machine: bump `version` in `Cargo.toml`, add its
`CHANGELOG.md` section, merge to `main`, then run **Actions → Release → Run
workflow** (or push a tag `v<version>`). The workflow tests, builds the installer,
zip and `SHA256.txt` on Windows and publishes them as a GitHub Release with
install instructions. How accurately and how
fast RightType handles a real mixed Thai/English text — and how often it touches
text that was typed correctly — is measured in
[`docs/TYPING_BENCHMARK.md`](TYPING_BENCHMARK.md)
(`cargo run --release --example typing_benchmark`,
`cargo run --release --example false_positive_audit`).

The windows are drawn by `src/ui.rs` (real Win32 controls with custom painting, so
keyboard navigation and screen readers keep working). A debug build opens one
directly for a quick look: `RIGHTTYPE_SHOW=settings` (or `settings-hotkeys`, `settings-learned`,
`settings-blocked`, `settings-about`, `welcome`, `help`, `stats`). The interface
typeface is IBM Plex Sans Thai, embedded from `assets/fonts` under the SIL Open
Font License (`assets/fonts/OFL.txt`).

The crate is split into an OS-free **core** (`layout`, `secret`, `dict`, `detect`,
`segment`, `buffer` — exhaustively unit-tested) and a Windows **integration layer**
behind the `winos` feature (`hook`, `inject`, `manual`, `safety`, `focus`, `session`,
`tray`, `toast`, `config`, `startup`, `learn`). Keeping the core OS-free makes it
testable anywhere and a future macOS backend additive.

**Adding a language.** `layout::registry` decides which language and table a
keyboard handle is: add its rule in `detect_with` and its layout-file names in
`classify_layout_file`, add the tables under `src/layout/`, a dictionary under
`assets/`, and tests. `policy::InputLayout` is still two-valued (Thai, English),
so converting between three languages needs that type replaced first; see the
follow-up in `docs/superpowers/specs/2026-10-29-keyboard-detection-design.md`. See the long-term
[`docs/archive/PLAN.md`](archive/PLAN.md) and the current
[`docs/DYNAMIC_PLAN.md`](DYNAMIC_PLAN.md) for active decisions, evidence, and
the next implementation step. What comes next is planned there as
[RightType 2.0](DYNAMIC_PLAN.md#s8--righttype-20); every feature idea — in 2.0
or not — is collected in [`docs/IDEAS.md`](IDEAS.md).

## License

Dual-licensed under either of [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE) at
your option.
