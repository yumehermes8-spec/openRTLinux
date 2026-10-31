# Changelog

All notable changes to RightType. Versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- English keyboards **Dvorak** and **Colemak**: words typed on the wrong
  language are fixed in both directions, live and in Fix text. Any country's
  English on the US or UK keyboard is recognised from its Windows layout file.
  Limits (practice and keyboard-map windows still show the US board, punctuation
  rules are tuned for US) are listed in the guide.
- A table of supported keyboards in the guide and README, including which ones
  RightType stays off for.

### Changed

- The Thai keyboard (Kedmanee, Pattachote, Manoonchai) is detected from the
  keyboard in use, so Settings → Keyboard no longer asks for it; it shows what
  was detected. The one used last is remembered for the next start. Switching
  by shortcut between two keyboards of one language (Kedmanee and Pattachote,
  US and Dvorak) is now undone to the exact keyboard that was left.

### Fixed

- The list at the text cursor (Shift twice), the app's shortcut list and the list
  for a held key now close when you click anywhere outside them. They never take
  the keyboard focus, so a click elsewhere in the same window used to leave one
  open, still taking digits, arrows and Enter that were meant for the app.
- The list at the text cursor no longer runs off the bottom of the screen the
  first time it opens low on the screen: it is placed again (below the cursor,
  or above it when there is no room) when its rows arrive and make it taller.
- Settings, Learned words: the Add and Remove buttons no longer overlap, and the
  page no longer draws a dark band under the table and an empty card under the
  sync switch (they were drawn where the old text box and rows used to be).
  Recent messages gets the same card round its switch as the other pages.
- Settings: buttons follow one placement rule. Buttons that add or open things
  sit on the left under a list and the ones that take things away (Remove, Clear
  all) on the right; the accent button follows its input. Learned words and
  Snippets are laid out that way, and the folder buttons on Learned words are in
  their own card.

## [2.6.5] — 2026-10-29

### Changed

- First-run welcome: a fresh install now starts in Auto, so the try-it box
  fixes `l;ylfu` at the space. Existing installs keep their mode.
- The welcome window has an English | ไทย switch (preselected from Windows),
  shows which Thai and English keyboards Windows has installed (and what to do
  if there is no Thai keyboard), and tells you live whether the word was fixed
  or what to check when it was not.
- The welcome window offers "Show me around" (the tour) next to "Get started",
  and closing it any other way now sends the window flying into the tray icon
  (a short curved flight, honouring Windows' animation setting) with a pill that
  says RightType is running and to start typing.
- Microsoft Store package (MSIX): run at startup uses a Windows startup task.

## [2.6.3] — 2026-10-28

### Fixed

- Dual-language symbols (`.` `?` `"`): the manual flip (Shift+Backspace) now
  reads them according to the active keyboard layout. On Thai they give `"` `M` `W`;
  on English they give `ใ` `ฦ` `.`.
- Long Thai sentences typed on US QWERTY (e.g. sentences with uncommon names or
  out-of-dictionary words) are accurately recognized and converted when segments
  match known words.

### Changed

- The Learned words page now looks and works like Snippets and Apps: a table
  of words with their keyboard, a box and Add button, Remove (or Delete), and
  nothing to save. It used to be a plain text box with a Save button.
- Messages in the pill at the bottom right now have an icon and colour for what
  happened (done, undone, info, check, problem), a short title with the hint
  under it, and stay as long as they take to read; the pointer on one keeps it
  there, and a click opens the new Recent messages page. They slide in from the
  right (a plain fade when Windows animation is off).
- The pill is flat (no lit edge round it) and its corners are concentric with the
  icon: the corner radius is the icon's radius plus the gap kept round it, the
  same for one line and for several. High Contrast keeps its outline.
- The pill's rounded corners and its icon disc are drawn smooth (per-pixel
  alpha) instead of stepped, and it fades with the same blend.
- Settings has a Recent messages page: the last 50 pill messages, kept in memory
  only and forgotten when RightType closes, with Clear all and a switch to
  stop remembering.
- The pill at the bottom right no longer runs a long message out of its
  rounded ends: text that does not fit one line wraps (up to four lines) inside
  a rounded rectangle.
- Fixed the `KFCvijkp,kd` example in a code comment (it should read
  `KFCvijvp,kd`); the behaviour was already right.

## [2.6.2] — 2026-10-27

### Changed

- The windows with a title bar (Settings, Fix text, Typing practice and the
  others) show the RightType icon in the title bar, the taskbar and Alt+Tab
  instead of Windows' generic one.
- The README is now a short front page (install, first use, what is
  different, privacy); the full manual is `docs/GUIDE.md`, and the portable zip
  ships it under `docs/`.

### Fixed

- The Support page showed a stray "i" button at the bottom. It belonged to the
  "start again after a crash" row of the Privacy & about page and had been
  attached to the wrong page; it is back on that row.
- The wallet address on the Support page sat at the top of its box, off the
  line of the Copy button; it is now centred in the box.

## [2.6.1] — 2026-10-24

### Changed

- A softer look: a muted blue accent and cooler greys in both the light and the dark theme, easier on the eyes than the bright blue.

### Fixed

- The installer no longer force-kills RightType (a hidden `taskkill` in its
  script) before installing; it asks you to close the app, as 2.5.0 did. The
  2.6.0 installer was flagged by antivirus software as a false positive, and
  2.6.0 was withdrawn.

## [2.6.0] — 2026-10-24

### Added

- A guided tour of Settings (tray menu → Take the tour): a spotlight and a short explanation for each of 11 steps across four chapters, with Back, Next and Skip.
- **One-line settings with info popover**: every setting now has a title and
  a one-line description; tap the (i) button after a title to see the full
  explanation. A screen reader hears the row's name with it ("More
  information: Fix common Thai misspellings").
- **Support page**: a new page in the sidebar with a Lightning address and a
  Bitcoin address, each with a QR code drawn on the device (nothing is sent
  anywhere) and a Copy button; the whole address is shown, on two lines when
  it must be. `tools/check_support_qr.py` decodes both QR codes and checks
  them against the addresses and their error-correction codewords.
- **Icons throughout**: every page of Settings has one in the sidebar, every
  switch has one in a round tile, buttons carry one before their label,
  section headings and the Stats cards have them too, and the three
  correction modes use the palette's icons. One table (`src/icons.rs`) says
  which icon is which; `tools/check_icons.py` checks that each exists in
  both Segoe Fluent Icons and Segoe MDL2 Assets.
- **An (i) button after a setting's title** opens a popover with the full
  explanation, so the row itself keeps one line. A screen reader hears the
  row's name with it ("More information: Fix common Thai misspellings").
- **The palette's "Add a word"** shows only while a word is selected in the
  app, and adds that word (it was always there, and took what was typed in
  the search box). While you search it sits below the matches, so Enter runs
  the command you found. The selection is read when the palette opens,
  through UI Automation or the text box itself, never the clipboard, and is
  wiped when the palette closes.
- **Words typed with `'` `;` `[` `]` `^` `/` `\` or backtick** in them are no
  longer taken for code identifiers, so they are fixed.
- **The "TH" / "EN" tag** by the cursor has its own colour for each language.
- **Fix this field** no longer opens a review popup after fixing; Ctrl+Z
  undoes it.
- **Privacy** (was Privacy & about) is a new page with a GitHub link; the
  Apps page is called **Ignored apps** in the sidebar.
- **Tools** lays its four buttons out as a 2 by 2 grid so each can show its
  icon; Keyboard health moved down to match.

### Changed


### Changed

- **Settings descriptions are one line.** The longer text moved behind (i);
  the Thai text was written again for each, not translated word by word.
- **Tools** lays its four buttons out as a 2 by 2 grid so each can show its
  icon; Keyboard health moved down to match.
- **Privacy** (was Privacy & about) is a new page with a GitHub link; the
  Apps page is called **Ignored apps** in the sidebar.
- **Fix this field** no longer opens a review popup after fixing; Ctrl+Z
  undoes it.
- **The palette's "Add a word"** shows only while a word is selected in the
  app, and adds that word (it was always there, and took what was typed in the
  search box). While you search it sits below the matches, so Enter runs the
  command you found. The selection is read when the palette opens, through UI
  Automation or the text box itself, never the clipboard, and is wiped when the
  palette closes.
- **Words typed with `'` `;` `[` `]` `^` `/` `\` or `` ` `` in them** are no longer
  taken for code identifiers, so they are fixed.
- **The "TH" / "EN" tag** by the cursor has its own colour for each language.

### Removed

- 21 texts no part of the program used any more (the first Donate set among
  them).

### Fixed

- **Tidying Thai text** (`normalize`) now settles in one go: a tone mark
  typed before a vowel that is also typed twice (`ก` + `่` + `ี` + `ี`) came
  out as `กีี่` instead of `กี่`. A long run of misplaced marks (ten or more
  before a vowel) is settled too: there is no fixed number of passes.
- **Tables in dark mode**: the column headers (Apps, Snippets, How each app
  is doing) were white, then dim grey; they are now painted in the
  interface colours.
- **Check marks** (Privacy & about, Keyboard health) are drawn, not typed:
  the font's "✓" read as a "V" at that size.
- **Keyboard page**: rows are taller, so a two-line description no longer
  touches the line below it.
- **Snippets**: the four keyboard choices fit their frame, which no longer
  runs under the Add / save button, and "English only" is no longer cut off.
- **Windows checkouts** no longer embed CRLF into the dictionaries and the
  benchmark article (`.gitattributes` keeps them LF), so a Windows build
  carries the same data as a Linux one and `typing_benchmark` runs there.

## [2.5.0]

### Added

- **Thai and English typed together without a space** are told apart at the
  end of the word: `กินKFC`, `ส่งไฟล์report`, `ใช้API`, `KFCอร่อยมาก`.
- **More symbols**: about 130 more LaTeX-style names (negated relations,
  big operators, arrows, card suits, `\baht` ฿, `\celsius` ℃, `\numero` №).
  New ghost shortcuts (Tab): `-->` `==>` `<==>` `|->` `~~` `/=` `+/-` and the
  fifths, sixths and eighths (⅕ … ⅞).
- **Superscripts and subscripts**: `x_1`, `a_n`, `x_12` offered as ₁ ₙ ₁₂ (not
  `my_var`); `e^x`, `10^-3` use every superscript Unicode has.
- Common Thai misspellings: `ผู้เชี่ยวชาน`, `เชี่ยวชาน`; `ประสบการณ`-style
  endings no longer double the last letter.
- Number words `฿500`, `500 บาทถ้วน`; a space after `ฯ` before Thai text.
- Modern code editors (Antigravity, Trae, Positron, Fleet, Neovim, Helix…)
  and terminals (Windows Terminal, Warp, Tabby, Hyper) are recognised.

### Changed

- **CapsLock** is turned off when a word finishes as Thai, so a whole Thai
  sentence typed with CapsLock on arrives right.
- **Word, Outlook, PowerPoint, OneNote**: the keyboard is switched *before* a
  correction is written, so Thai is no longer marked as English (red
  squiggles). Words there are corrected at their end, not key by key.
- A multi-line snippet in a chat app uses Shift+Enter between lines.
- Ghost suggestions no longer take Tab in browser address and search bars.

### Fixed

- A layout switch an app ignored no longer wipes the word being typed.
- Two-key Thai typos such as `คมขุ` are no longer read as numbers.
- A leading quote or bracket no longer stops Title Case from capitalising.

## [2.4.0]

### Added

- **The list at the text cursor**: press **Shift twice** where you type. Find
  any character Unicode names, in Thai or English (`ลูกศร`, `degree`,
  `ohm`, look-alikes such as `->`); your snippets; and **the app's own
  commands** from its menus and a built-in table (Word, PowerPoint, Excel,
  OneNote, browsers, VS Code, Teams…). Enter types or runs the row picked; a
  command that could lose work takes Enter twice, and its shortcut is shown
  for next time. Empty, it is a grid of the characters you pinned and used
  lately. Keys typed while it opens are never lost to the app.
- **Ghost suggestions** (on by default): `->` `!=` `<=` `1/2` `...` and
  `x^2`, `(x^2)^2`, `H_2O`, `\alpha` show what they can become next to the
  cursor; **Tab** writes it, anything else leaves the text as typed. Plain
  words and sentences never bring one up (checked on every dictionary word).
- **Math, three ways**, in the list at the cursor (and as a ghost when it
  ends what you type):
  - said in words, Thai or English: `x ยกกำลังสองบวก 1`, `รากที่สองของ x`,
    `a ส่วน b`, `x squared plus y squared`, `sum from i=1 to n of x sub i`;
  - written short, as Word's equation editor reads it:
    `x^2 + (2x)/8 + x^(3/2) + pi + Sigma_(i=1)^(n) X_i`;
  - LaTeX: `\frac{1}{2}`, `\sqrt[3]{x}`, `\sum_{i=1}^{n}`, `\mathbb{R}`.
  Written as one line of Unicode anywhere (x² + 1, √x, ∑ᵢ₌₁ⁿ). In **Word,
  PowerPoint and OneNote** two more choices: a **real equation** (Alt+=,
  which the app builds up) or text with **superscript and subscript**
  formatting, for what Unicode has no small letters for (x^(3/2)).
- **Characters by key**: tap **Right Alt** after a character to step it
  through its set (`2` → ² ₂ ⅔, `e` → é è ê ë, `=` → ≠ ≈ ≤ ≥); Esc puts it
  back. Pin any character to a key from the list with **Ctrl+P**; holding a
  key (opt-in) offers the same set. Settings → Keyboard turns it off.
- **Macros**: a snippet whose text has steps — `{กด Ctrl+B}`, `{รอ 300}`,
  `{คำสั่ง ตัวหนา}`, `{สไตล์ Heading 1}`, `{คลิปบอร์ด}` (or `{press …}`,
  `{wait …}`, `{command …}`, `{style …}`, `{clipboard}`) — run by its trigger
  or from the list. Esc, or another app in front, stops one; a step that
  cannot run keeps the snippet from being saved.
- **Thai abbreviations typed on the English keyboard**: `d"8"` → ก.ค.,
  `dm,"` → กทม., `fi"l,=kp` → ดร.สมชาย (Kedmanee's period is the `"`
  key).
- More of Word's shortcuts in the table (headings, styles, equation,
  superscript, spacing, comments, track changes…) and OneNote's.

### Changed

- **Word**: deleting a word to fix it counts what Word kept on screen — it
  drops Thai marks with no letter before them, and RightType used to delete
  into the word before.
- **Fix this field** fixes only the words picked, each where it is, instead
  of retyping the whole field (which lost formatting and set off Word's
  AutoFormat on every quote).
- **Fix text** repairs better: short Thai dictionary words next to Thai are
  read in context.
- **Command palette**: shorter rows that tell themselves apart, sections
  that open as a page of their own (Backspace goes back), and a window that
  scrolls instead of running off the screen.
- **Typing practice** uses everyday words and real sentences; the game has
  combos, levels and hearts.
- The text kept before the cursor for ghost suggestions follows what
  RightType writes (a word put into Thai, a snippet), in memory only, 48
  characters at most.

## [2.3.0]

### Added

- **Settings → Keyboard**, a new page (second in the sidebar) with every
  typing switch that was only in the command palette, the Thai keyboard
  picker, and the new switches below. Pages taller than the window scroll.
- **Settings → Tools**: clean the keyboard, test it, the keyboard map and
  typing practice, one click each, and a **keyboard health check**: keys
  Windows counts as held down, Sticky Keys / Filter Keys and the shortcuts
  that turn them on by accident, Thai keyboards installed twice or not at
  all, Ctrl+Shift as the language switch, Windows' own autocorrect,
  CapsLock, keys that type twice. Each problem has a fix button.
- **Clean the keyboard**: every key is held for 30 s to 2 minutes (the
  Windows key, Alt+Tab, media keys too; the mouse when asked); the keys
  wiped light up on a keyboard drawing. Unlocks by itself or with the
  button; Ctrl+Alt+Del always works. The window names the keys no program
  can hold (Fn, brightness, Wi-Fi).
- **Key tester**: each key pressed lights up and goes nowhere else; Esc
  held for 2 seconds closes it.
- **Keys that type twice by themselves** (a worn switch: `hhello`) are
  found, and filtered for the keys you choose.
- **Barcode scanners** with the Thai keyboard on: a scan comes back as its
  digits and letters. **Fake keyboards** (a device typing a command into
  the Run box faster than any hand) are held back. Both on by default.
- **Ctrl and Alt shortcuts in English** (opt-in): web apps that read the
  letter saw Ctrl+แ for Ctrl+C; with this on, Ctrl or Alt switches to
  English until let go.
- **This app's shortcuts**: the shortcuts of the app in front (from its
  menu, a built-in table for browsers, VS Code, Office, Discord, Teams,
  Slack…, and Windows' own), searchable in Thai or English; Enter presses
  the one picked. From the palette, or by holding Ctrl for a second
  (opt-in).
- **Hold a key for more characters** (opt-in): `.` → … · •, `-` → – —,
  vowels → accents, digits → Thai digits, `$` → ฿ € £, ๆ → ฯ ฯลฯ.
- Settings warns when a new hotkey takes keys apps already use.
- **Typing practice** (palette, Tools): Kedmanee, Pattachote, Manoonchai
  or English, a row of keys at a time, with Thai tone marks and digits;
  practice text from real words that use only the keys learnt so far,
  leaning on the keys missed most; 60-second rounds and a falling-words
  game. Works without the keyboard installed, and nothing typed there
  reaches any app. Day scores and per-key counts are kept only if you
  turn that on (never the text); turning it off deletes them.
- **Keep the keyboard on screen while you work**: from practice, the
  keyboard map of the keyboard you are learning stays on top, and the keys
  you know fade.
- **Show shortcuts on screen** (opt-in), for teaching or recording:
  Ctrl+C, Alt+Tab, Enter… at the bottom of the screen, left in recordings.
  Never letters, nothing in password fields or apps on the safety list.
- **Rest reminder** (opt-in): after 50 minutes of typing without a
  5-minute break. Counts only, in memory.
- **How each app is doing** (Statistics → By app): per program, the words
  fixed, read back as sent or shown differently, and undone, with a hint
  (Suggest mode where fixes are often undone). Memory only.

### Changed

- **Every wait in the keyboard hook shares one budget of 200 ms per key**,
  so Windows never lets a key through on its own (the `lสวัสดี` bug). The
  problem report says how long the hook took per key (numbers only).
- **Half the memory for the dictionaries** (10.2 MB → 5.6 MB) and a
  quicker first word: the bundled words are no longer copied one by one.
- A text box a few keys behind (Windows 11 Notepad) is asked again before
  RightType falls back to typing keys.
- An app's own keyboard is set as soon as it comes to the front.
- Windows draw off screen and appear whole: no flicker in the practice
  window, and nothing missing at 144 DPI.
- **A keyboard per device** (planned) is not possible safely: Windows says
  which device a key came from only after RightType must decide. Scanners
  and fake keyboards are told apart by speed instead.

## [2.2.0]

### Added

- **Addresses and numbers typed with the Thai keyboard on come back**:
  `ืฟทำ๑เทฟรสใแนท` → `name@gmail.com`, `www.` and `http` addresses and
  domains such as `.com` and `.co.th`; `ๅจจ` → `100`, also `1,250`
  `3.50` `100%` `10:30` `2/10` `081-234-5678`. Judged by the keys pressed;
  Thai text on screen (a dictionary word, or known words in a row) is never
  read as one. On by default; the command palette turns it off.
- **Hold Shift+Backspace** (half a second) to flip the rest of the words in
  one go; one more press, or Undo, puts them all back.
- **TH / CAPS tag at password fields** when the Thai keyboard or CapsLock
  is on — the usual reason a password is refused. Only the keyboard's
  language and CapsLock are looked at; not in apps on the safety list.
- **The keypad with NumLock off** (it moves the cursor instead of typing
  digits) is pointed out; or, by choice, NumLock is turned on and the digit
  typed. **Insert** (typing over the text) is pointed out, or held back.
- **A keyboard per app** (Settings → Apps, the new Keyboard column, or the
  palette): Thai or English when the app comes to the front, or **English
  outside text** for apps whose tools are single keys (Figma, Photoshop,
  Blender): English while there is no text cursor, the Thai keyboard back
  in text.
- **The grave key (`) can type its character** instead of switching the
  language (opt-in), for code and Markdown on Windows set up for Thai.
- **A language switch that comes with a shortcut** (Ctrl/Alt + Shift + a
  key) is put back (opt-in).
- Palette: **Thai spacing** for the selection by the Royal Institute's
  rules (ๆ, ฯลฯ, ฯ, brackets, runs of spaces); only spaces move.
- Palette: **Why?** — why the last word was fixed or left as typed (an
  English word, Thai already, Manual mode, a possible recovery phrase…).
- Snippets: **My typo** — a word you often misspell and its right spelling,
  fixed like the built-in misspellings (Backspace right after undoes it).
- **Complete long Thai words with Tab** (opt-in): offered only when every
  dictionary word that starts so goes on the same way (`ประชาสัมพั` →
  `ประชาสัมพันธ์`), so the offer is never a guess.
- **Keyboard map** (`Ctrl`+`Alt`+`K`, or the palette): the Thai keyboard in
  use key by key, Shift for the upper characters; click a key to type it
  where you are (the map never takes the focus).
- **Manoonchai**, a third Thai keyboard (Settings → Hotkeys), with its own
  check that no dictionary word is changed.

### Changed

- **Works alongside apps that rewrite text**: Word, Outlook and Google Docs
  capitalise, curl quotes and AutoCorrect a word after RightType writes it;
  the check-after-write no longer reads that as garbling (or slows those
  apps down).
- Ctrl+Backspace by Thai word is left to programs that already do it
  (browsers, Electron apps, Office, LibreOffice); RightType does it where
  they do not (Notepad, classic programs).
- Excel's AutoComplete is cleared before a correction when something is
  selected.
- Fix this field in Google Docs says how to let Docs share its text
  (Tools → Accessibility → screen reader support).
- Privacy & about notes when Windows' own keyboard autocorrect is on.

## [2.1.0]

### Added

- **Code mode** for code editors (VS Code, Cursor, Visual Studio,
  JetBrains IDEs, Sublime Text, Notepad++ and others; on by default there,
  changeable per app):
  - names are never touched: `camelCase`, `snake_case`, `CONSTANT`, and
    anything with digits or `_ . :: ->`;
  - Thai keys typed for code come back as the English typed (`ฟหกด` →
    `asdf`), even when that is not a dictionary word;
  - English typed for Thai becomes Thai only inside a comment or a string;
    where the editor does not share its text, it is offered as a hint;
  - CapsLock is not treated as an accident (`MAX_SIZE` is meant).
- **A calmer mode offered where fixes keep being taken back**: three in ten
  minutes in one app, and RightType offers Suggest (or Manual) there —
  **this time only** or **from now on** — from the palette (1 or 2).
- **Settings → Apps is a table**: app, mode, set by (you, this time only,
  default, safety), and where the program is. Select a row and pick a mode,
  or right-click it (keep for good, remove, show the program file);
  **+ Add an app that is open** lists running programs instead of typing a
  name. Built-in safety apps are listed, locked.
- **Recent words in the palette**: the last words typed, each with what it
  would flip to. Tick several with Space and they are **tinted where they
  are in the app** (when the app says where), then Enter flips exactly
  those and leaves the words between them alone.
- **Off in this field** (palette): RightType stays out of one field — a
  search box, a code cell — and keeps working everywhere else in the app,
  until it restarts.
- **Ctrl+Backspace deletes one Thai word** (on by default): Windows takes
  everything back to the last space, which in Thai is often a whole
  sentence. Only when the text before the cursor ends in a run of Thai of
  more than one word; otherwise the key is Windows' own. Ctrl can stay held
  for the next word.
- **A chat message typed on the wrong keyboard is not sent unread** (on by
  default): in LINE, Teams, Discord, Slack, Telegram, WhatsApp, Messenger,
  Signal, Zoom and Skype, Enter on a message where at least two words (and
  at least half) look typed on the wrong keyboard is held once with a note;
  Enter again sends it, Shift+Backspace fixes words. More apps can be added
  as `chat_apps` in the config. Not in browsers (Enter there may not send).
- **Snippets** (Settings → Snippets): a short trigger and Space becomes a
  longer text, line breaks included. **Date and time fields** fill in when
  the snippet is typed — `{วันที่}` 2 ตุลาคม 2569, `{วันที่เต็ม}`
  วันพฤหัสบดีที่ 2 ตุลาคม พ.ศ. 2569, `{วันที่ย่อ}` 2 ต.ค. 69,
  `{วันที่เลขไทย}` ๒ ตุลาคม ๒๕๖๙, `{วันที่ตัวเลข}` 02/10/2569, `{เวลา}`
  14.30 น., `{date}` 2 October 2026, `{date-us}` October 2, 2026, `{iso}`
  2026-10-02, `{time}` 14:30 — picked from a **Date / time** button. Each works on the Thai keyboard, the
  English one, or **either** — matched by the keys pressed, so `;addr`
  works with the Thai keyboard on too. Shift+Backspace right after puts the
  trigger back. Not in password fields.
- **Common Thai misspellings put right** (opt-in, Settings → General or the
  palette): `อนุญาติ` → `อนุญาต`, `ผลลัพท์` → `ผลลัพธ์` and about 60 more,
  only where the result reads better to the dictionary and never a word the
  dictionary knows. Each fix shows in its own colour (purple) with what
  changed; **Backspace right after puts the word back** instead of
  deleting into it, and that word is left alone from then on.
- **Screen readers** (Narrator, NVDA, JAWS) hear each fix ("Fixed:
  สวัสดี"), tag and message, only while one is running.
- **High Contrast**: Settings, the palette, tags and highlights use the
  contrast theme's own colours.
- **One RightType at a time, with no dialog**: opening it again shows
  "already running" for a moment; opening another version or copy closes
  the running one and takes over ("Now running 2.1.0, closed 2.0.1").
- **Start again after a crash** (on by default, Settings → Privacy & about):
  not after you quit it or end it in Task Manager, at most three times in a
  row.
- **Sync settings and snippets too** (opt-in): with a sync folder for the
  learned words, modes, apps, hotkeys and snippets are shared by every PC
  using that folder.
- **Windows on ARM**: an arm64 installer and zip.
- **English prefixes get their hyphen in prose** (on by default, in Auto):
  `relogin` → `re-login`, `reenable` → `re-enable`, shown in the spelling
  colour, Backspace right after puts it back. Never in Code mode or an
  address bar; words the dictionary writes closed (`reinstall`, `reenter`)
  are left alone.
- **A preview while you type (Auto)**: before Auto is sure enough to fix a
  word, the cursor tag shows where the keys are heading — `l;yl` shows
  `→ สวัส` — so you can see a fix is coming without anything changing yet.
- **Command palette** (`Ctrl`+`Alt`+`Space`) gains:
  - **Fix this field, checked first**: the words it would change are listed
    ticked and tinted where they are in the field; Space unticks one, Enter
    fixes the ticked ones (Ctrl+Z undoes it). The field is read from the
    app without selecting it and never through the clipboard; if the text
    changed meanwhile nothing is fixed. More than 12 words are fixed at
    once. An app that does not share its text gets the **Fix text** window
    instead (now under Options in the palette, and in the tray).
  - **Fix only the wrong-keyboard words** of the selection (the flip hotkey
    still flips all of it).
  - **Thai in standard form**: text from PDFs that looks right but is not
    found by search (`นํ้า` → `น้ำ`, a tone mark before its vowel, old
    fonts' private-use letters) put into the form Windows searches for.
  - **Year พ.ศ. ↔ ค.ศ.** (`พ.ศ. 2569` ↔ `ค.ศ. 2026`), **number in Thai
    words** (`1,250` → `หนึ่งพันสองร้อยห้าสิบ`) and **amount in words**
    as on a cheque (`1,250.50` → `หนึ่งพันสองร้อยห้าสิบบาทห้าสิบสตางค์`).
    These and the case/digit changes sit behind "More changes to the
    selection".
  - **Special characters** by name in either language: `฿` (baht), `ๆ`,
    `ฯ`, `ฯลฯ`, `°`, `×`, `±`, `…`, `—`, `©`, `€` and more — type to search.
  - **Type the copied text key by key**, for remote desktops, virtual
    machines and forms that refuse paste: each character is the key that
    types it on the keyboard in use; Esc stops it. At most 2,000
    characters; the clipboard is read only when this is picked.
  - **Icons** on every row and heading, from Windows' own icon font.
  - **Never convert “word”**: for words you took back lately while learning
    is off — one click and RightType leaves that word alone from then on.
  - **Thai digits ↔ 0–9**, **UPPER CASE**, **lower case**, **Title Case**
    and **sWAP cASE** (CapsLock undone) for the selection.
  - **Works without a mouse**: ↑↓ (or Tab) and Enter, 1–9 to run a line,
    type to search (on either keyboard: `fxw` finds ซ่อม), Esc to close.
  - **CapsLock switches Thai/English** (off by default): tap CapsLock to
    switch, hold it half a second for CAPS.
  - **Tray icon shows TH / EN** (off by default).
- **Thai typed in a wrong order that looks right is put right** (Auto):
  `เเ` (two เ) for `แ`, `ํา` for `ำ`, a tone mark typed before the vowel
  above it, the same mark twice — when the result is a Thai word.
- **Tags and hints stay off shared screens**: Teams, Zoom, recordings and
  screenshots do not show them (a Suggest hint or preview is what you
  typed); you still see them.
- **Portable mode**: an empty file named `portable` next to
  `righttype.exe` keeps settings and learned words in a `data` folder beside
  it instead of `%APPDATA%`.
- **The first three fixes of a session show `↶ Shift+Backspace`** at the
  cursor, and the Welcome window has a box to try a fix in.

- **Tray → "Save a problem report…"** writes what RightType did lately (the
  last 400 steps: a word ended and was or wasn't fixed, how the correction
  was typed, Shift+Backspace, which program the caret moved to) to a file
  you choose, to attach to a bug report. There is no typed text in it:
  a word appears only as how many Thai and English letters it had. It is
  kept in memory only and written only when you save it.

- **Check-after-write**: after RightType types a correction, it reads back
  the word before the cursor (when the app shares its text) and compares.
  If an app shows something other than what was sent — the way Windows 11
  Notepad turned สวัสดี into `ีีีีีี` — it tells you, notes it in the problem
  report, and from then on waits longer before typing in that app.

- **Stays out of full-screen games, Remote Desktop and virtual machines.**
  Movement keys spell Thai (`wasd` is ไฟหก), so a game could get
  Backspaces; and in a remote session or VM the keys belong to the other
  computer. Games in exclusive full screen, slide shows, and any full-screen
  window without a text cursor are left alone (a full-screen browser or
  editor still works); Remote Desktop, the Windows App, Hyper-V, VirtualBox,
  VMware, AnyDesk, TeamViewer, RustDesk, Parsec and VNC viewers are on the
  built-in list.


### Fixed

- **A full-screen browser on some PCs was taken for a game** (Chrome after
  F11 shows Windows no text cursor), so nothing was corrected there. A
  full-screen window counts as a game only when the focus is not in a text
  field. Found by the self-test on a real PC.
- **Notepad, WordPad and other standard text boxes: English typed on the
  Thai layout could take two more characters with it when fixed**
  (`;yoouh there` came out as `วันนีthere`). These boxes drop Thai vowels
  and tone marks that cannot follow the letter before (`there` on the Thai
  layout is `ะ้ำพำ`; the box keeps `ะพำ`), so there were fewer characters to
  delete than were typed. RightType now looks at what the box really holds
  before replacing.
- **A slow text box could get a correction in the wrong place**: Windows 11
  Notepad sometimes answers before it has handled the latest keys. If what
  is before the caret is not what RightType expects yet, the correction is
  typed as keys instead, which arrive after them.

### Changed

- **The command palette is grouped**: the words just typed, fixing text,
  the selected text, this app and field, and the mode, each under its own
  heading; numbers 1–9 go to the things done to text, and the switches and
  Settings are folded under one row until opened (or searched for). Rows
  are left-aligned with their state (On/Off, in use) at the right, and every
  row and heading has an icon from Windows' own icon font; the selected
  row's icon takes the accent colour.

- **Smoother tags and messages**: the pill now rises a few pixels into
  place while fading in (140 ms, ease-out) and fades out with an ease-in
  (220 ms), each frame computed from the time elapsed at about 60 frames a
  second — it used to appear at once and fade in coarse 30 ms steps. A tag
  that is already showing moves and stays instead of fading in again. The
  TH, EN and CAPS tags each have a colour of their own (teal, blue, amber).
- **A `CAPS` tag at the text cursor** when CapsLock is switched on (with
  the TH/EN tags on), before a sentence comes out in capitals.
- **CapsLock left on by accident is put right, with a way back**: in Auto,
  `hELLO` — Shift on the first letter, so capitals were not meant — becomes
  `Hello`, and a Thai word typed on the Thai layout with CapsLock on (every
  key comes out shifted: สวัสดี as `ศซํศโ๊`) becomes the word meant;
  CapsLock is turned off and a note says so. Meant the capitals? One
  Shift+Backspace (or Ctrl+Shift+CapsLock) puts them back and CapsLock on
  again. In Manual and Suggest it is only offered (`⇪ Hello · Tab`). Words
  in capitals (`VARIABLE`, `NASA`) are never touched.
- **Thai typed with CapsLock left on is still fixed**: the English layout
  shows `L;YLFU`, but the keys are the ones for สวัสดี, and RightType now
  reads them that way. English typed with CapsLock on stays as it is
  (`HELLO`), and anything RightType puts back as typed — Shift+Backspace,
  undo, a word that goes back at the space — comes back in capitals as it
  was shown. Pressing CapsLock mid-word leaves that word alone.
- **English words with a prefix or suffix stay English**: a known word with
  `re`, `un`, `pre`, `dis`, `multi` … in front or `ing`, `ed`, `ness`,
  `able`, `s` … behind (`rerise`, `resit`, `multiholes`) is no longer
  turned into Thai, even when it is not in the word list. Unknown English
  words wrongly converted in the live study: 170 → 112 of 20,000; Thai is
  unchanged (a Thai dictionary word always wins).

## [2.0.1] — 2026-09-30

### Changed

- **Settings, Statistics, Fix text and Welcome appear fully drawn**: they
  used to show their frame first and fill in control by control; switching
  a Settings page repainted it dozens of times.
- **Convert selection no longer goes through the clipboard.** It pressed
  Ctrl+C for you, so the selected text went into Windows clipboard history,
  could be synced to your other devices (an Android phone) and was visible
  to any program watching the clipboard — even though RightType put the old
  clipboard back. It now asks the app for the selection through UI
  Automation. Apps that do not share it say so; `selection_via_clipboard =
  true` in `config.toml` brings back the copy for them.
- What RightType puts on the clipboard (Fix text's **Copy**) is marked to
  stay out of clipboard history and cloud sync.
- **Every Shift+Backspace says what it did**: "Flipped 1 word · again for
  the one before", "Flipped back 3 words", "Undone", or "Nothing to flip
  here" when the cursor moved and RightType no longer knows the text before
  it (a press that did nothing used to look like one that failed). What is
  kept for this, and what each press does, is in `docs/UNDO.md`.
- **Shift+Backspace pressed once too often puts things back**: after the
  oldest word was flipped (or the only one — `reload` → `พำสนฟก`), the next
  press restores every word the run changed, instead of saying there is
  nothing to flip.

### Fixed

- **Corrections in Notepad (Windows 11 and classic), WordPad and other
  standard text boxes are made by the box itself**: RightType tells it to
  replace the word in one edit (Ctrl+Z undoes it) instead of typing
  Backspaces and characters. Typed corrections came out garbled in Windows
  11 Notepad (`สวัสดี` became `ีีีีีี`) and keys typed while a word was being
  rewritten were lost. Elsewhere, text typed after Backspaces now waits
  40 ms for them to land.
- **RightType could crash in Edge's address bar** (2.0.0), after which
  nothing was corrected and Shift+Backspace was a plain Backspace until it
  was restarted: the small TH/EN tag could be painted after its text had
  been wiped, and drawing empty text read an invalid pointer. Found with
  four Edge rounds per CI run and a crash report in the debug build.
- **Shift+Backspace right after an automatic fix did nothing** (2.0.0): the
  Shift key on its own made RightType forget the words it keeps for flipping
  back, so the flip found nothing. It flips the fixed word back again.
- **Corrections in the browser's address/search bar left a stray letter**
  (`giupo` came out as `gเรียน`): when the bar completes what you type and
  selects the rest, the first Backspace only removed that selection.
  RightType now clears it first in Chrome, Edge and other Chromium browsers
  and in Firefox.
- **Edge's address bar forgot the start of the word** (`l;ylfu` came out as
  `l;ัสดี`): its suggestion list gives the highlighted row accessibility
  focus on almost every keystroke, and RightType took each of those as a
  move to another field. Focus on the same field or on a list/menu row no
  longer counts (and leaves the password-field status as it was).
- **An English word RightType does not know could turn Thai** (`relogin`
  became `พำสนเรื`): its first letters read as short Thai words. At the
  space, a reading that cannot end a Thai word goes back to the keys typed
  (when they were all letters), and 24 everyday computer words (`relogin`,
  `logout`, `signin`, `dropdown`, …) were added.
- **Fewer unknown English words turn Thai** (`reavik` became `พำฟอรา`):
  a reading made of nothing but three or more one- and two-letter Thai
  words (พำ + ฟ + อ + รา) is no longer enough to convert a word, mid-way or
  at the space. On 20,000 unknown English words, wrong conversions fell
  from 246 to 170; of 20,000 unknown Thai words, one fewer arrives as Thai.
- With both US and UK English keyboards installed, RightType starts with the
  one in use rather than the first installed, so **Fix text** opened from the
  tray before any typing uses the right punctuation (`"` `@` `£` `#`).

## [2.0.0] — 2026-09-27

RightType 2.0 shows what it does where you are typing, lets you steer it from
the keyboard, and works with more keyboards. The highlights:
a `TH` / `EN` tag and Suggest hints at the text cursor (Tab takes a hint),
Pause, a mode per app, flipping back several words, a Fix text window, a
command palette, hotkeys you can change, Thai Pattachote and UK English, an
opt-in 7-day chart and learned-words sync folder, and install/upgrade through
winget. As always, it never goes online and never saves what you type.

### Fixed

- **The status pill (mode, on/off, errors, Suggest hint) never appeared** in
  1.1.0: since it became lazily created, it was only ever allowed to be created
  on a thread it had not yet recorded. It shows again.
- Settings, Welcome and Statistics stay sharp on a second monitor with a
  different scale: RightType is now per-monitor DPI aware (v2), opens windows on
  the monitor under the mouse, and re-lays a window out when it is dragged to a
  monitor with another scale.

### Added

- **Pause** from the tray: 10 minutes, 30 minutes or an hour, then RightType
  switches itself back on (the tooltip shows the time left). A pause is never
  saved as "off".
- **A mode per app**: Auto, Suggest, Manual or Off for one program — say Manual
  in your code editor and Auto in chat. Set it from the tray (**In this app**,
  for the app you were just typing in) or in Settings → **Apps**. The built-in
  safety list (password managers, terminals, wallets) still always wins.
  `Ctrl`+`CapsLock` in such an app cycles that app's mode.
- **Flip back several words**: press `Shift`+`Backspace` again to flip the word
  before as well, up to 8 words, so a phrase typed in the wrong layout comes
  back without selecting it. A click, arrow key or Backspace in between starts
  over.
- **Import / Export** learned words as a text file (Settings → Learned words).
- **Time saved** estimate in Statistics.
- **Fix text** window (tray → Fix text…): paste a paragraph typed in the wrong
  layout, or take it from the clipboard with **Paste and fix**, and copy back
  the fixed text. Every word is judged by the same rules as while typing; in
  a test of 20,000 correctly typed mixed Thai/English sentences, none was
  changed.
- **Hints at the text cursor**: a small `TH` / `EN` tag flashes under the text
  cursor when RightType switches the language, and the Suggest hint appears
  there too. **Tab** takes a Suggest hint right after it appears (Tab is
  otherwise untouched; `Alt`+`CapsLock` still works). Can be turned off in
  Settings → General.
- **Guess each field's language** (opt-in, Settings → Apps): RightType counts
  how many Thai and English words you finish in each kind of field (the program
  and the kind of control), and once a field clearly has one language (20+
  words, 80 %+), clicking into it switches the keyboard before you type. Only
  the counts are stored (`contexts.toml`), and **Clear** removes them.
- A colon or question mark typed after a Thai phrase on the Thai layout (the
  keys give ซ and ฦ there) comes back as `:` / `?` when the phrase is known
  Thai (a dictionary word or three or more known words).
- **Last 7 days** in Statistics (opt-in): a bar per day and the week's total
  words fixed and time saved. Only two numbers per day are kept
  (`stats.toml`); turning it off deletes them.
- **Sync folder for learned words** (Settings → Learned words): keep the list
  in a OneDrive / Google Drive / team folder; both lists are merged when you
  choose it, and a change made on another PC is picked up within seconds.
- **Change the hotkeys** (Settings → Hotkeys): click Change and press the new
  keys. Chords that would get in the way of typing, and chords already in use,
  are refused; Reset all brings back the defaults.
- **Command palette** (`Ctrl`+`Alt`+`Space`): a short list next to the text
  cursor — Fix text, pause, turn RightType off in this app, switch mode,
  settings. Arrow keys and Enter, Esc to close.
- The TH / EN tag and the Suggest hint now also find the text cursor in apps
  that draw their own (through UI Automation).
- **Suggest while typing**: in Suggest mode the Thai reading appears next to
  the cursor as soon as the keys typed so far clearly spell it (the bar Auto
  uses); Tab flips the word there and then.
- Thai words whose keys give no letters on the English layout come back:
  `57'` → ถึง, `]'` → ลง, `[688]` → บุคคล. Numbers, dates, times, prices and
  emoticons never change (0 of 750,043 tested); 163 more Thai dictionary words
  are recovered.
- **Thai Pattachote keyboard**: choose it in Settings → Hotkeys → Thai
  keyboard. Its table comes from the maintained xkeyboard-config layout; the
  same quality gate runs on it (no Thai or English dictionary word changed;
  60,213 of 60,964 Thai words recovered).
- **English on the UK keyboard, and English of other countries** (Australia,
  New Zealand, Canada set to the US keyboard, …): RightType now works with
  them instead of staying off. Which English keyboard you use is found
  automatically, so Thai text converted back to English gets the UK
  punctuation (`"` `@` `£` `#`) when that is your keyboard.
- **Check for updates** in Settings → Privacy & about: opens the Releases page
  in your browser; RightType itself still never goes online.

### Changed

- Hotkey names in Settings and Welcome are no longer hidden under long key
  combinations; they are cut short with "…" only when there is truly no room.
- A mouse click now counts as moving the caret: the word in progress, Undo and
  the words kept for `Shift`+`Backspace` are dropped, so a flip can never edit
  text somewhere else.
- The status pill appears on the monitor you are working on (the one holding
  the active window) instead of always the primary one, sized for that
  monitor. It can also be placed next to the text cursor (used by the 2.0 caret
  badge).
- **Technical terms and product names** in their usual casing (`PyThaiNLP`,
  `WangchanBERTa`, `RoBERTa`, `LoRA`, `JavaScript`, `GitHub`, … ~300 in
  `assets/tech_terms.txt`) come back when typed on the Thai layout. In the
  typing benchmark RightType now fixes 127 of 131 wrong-layout words (was 118),
  still with no correctly typed word changed.
- If the keyboard hook is lost and cannot be reinstalled, RightType says so
  (message and grey tray icon with a warning tooltip) and keeps retrying, then
  tells you when it is back.
- The installer is available in Thai (chosen from the Windows language).

### Build and release

- CI quality gate: the false-positive audit and the typing benchmark run on
  every PR and fail if any correctly typed dictionary word, phrase or
  benchmark word would be changed, or recall drops below its floor.
- CI builds the installer on every PR, so a broken installer script is caught
  before release.
- **winget**: each release carries winget manifests
  (`winget-manifests-<version>.zip`, package `k-affan-th.RightType`), and the
  Release workflow submits them to winget when a `WINGET_TOKEN` secret is set.
- Release checklist: a 2.0 section of checks that need a real Windows desktop.

## [1.1.0] — 2026-09-26

### Fixed

- **Whole-word decisions.** A word converted to Thai mid-word is re-judged as a
  whole at the space: if all of its keystrokes spell English, the entire word
  goes back to English and the layout returns to US. Previously only the part
  typed after the switch was judged, leaving half-Thai, half-English words that
  `Shift`+`Backspace` could not flip back whole.
- Keystrokes typed right after RightType switches the layout are read with the
  new layout, and RightType's own switch no longer drops the word in progress.
- English typed right after a Thai word (layout still Thai) is brought back.
- `Ctrl`+`CapsLock` (mode cycle) works in every app, including Electron apps
  such as the Claude desktop app, where it used to toggle CapsLock instead.
- `Alt`+`CapsLock` (accept suggestion) no longer triggers the app's menu bar in
  Chrome/Electron, which swallowed the correction.
- Selection Undo waits for the hotkey's modifiers to be released, so it can no
  longer type a literal `z`.
- Undo is discarded once more text has been typed, so it can no longer delete
  what you typed after the correction; after Undo the layout follows the
  restored text.
- **Seed-phrase guard:** every BIP39 word now counts toward the guard on either
  layout (words like `cat`/`ski`, which are real Thai words on the Thai layout,
  used to reset it); mid-word conversion is held once three seed words are in a
  row; and when the guard trips, the Undo record, last word and any Suggest hint
  are wiped from memory.
- Converting a selection while another program holds the clipboard retries
  briefly and then says the clipboard is busy, instead of wrongly saying it
  needs plain text.
- If Windows silently drops the keyboard hook (no sleep/lock event involved),
  RightType notices the silence while you type and reinstalls it within ~30 s.

- **Backspace inside a word** could leave raw keys mixed into the Thai
  (`mujouj1` then Backspace showed `muที่นี่`). Found by the new typing
  simulation; a Backspace now re-renders only a run RightType already owns.
- **Fewer wrong conversions of Thai typed on the Thai layout.** Punctuation
  keys that are Thai letters on Kedmanee (`;` ว, `[` บ, `'` ง, `,` ม) are no
  longer stripped before checking for English, and English read from Thai
  keys must look like English. Across the bundled dictionaries: Thai words and
  phrases wrongly converted 1,994 → 0, English words with punctuation 146 → 0,
  random unknown Thai 2.6 % → 0.18 % (see `docs/TYPING_BENCHMARK.md`).

### Changed

- **Technical English typed on the Thai layout comes back**: numbers
  (`12,480`, `0.912`, `64%`, `2e-5`), acronyms (`GPU`, `A100`), derived words
  (`tokenization`) and hyphenated terms (`fine-tuning`, `F1-score`, `TF-IDF`),
  also inside quotes or brackets. Thai typed on the English layout keeps a
  `:`/`?`/`"` typed after it. In a simulated academic article typed without
  ever switching layout, RightType now fixes 90 % of wrong-layout words
  (was 70 %), with no correctly typed word changed.

- **New look.** Settings, Welcome and Statistics were rebuilt: a Settings
  sidebar with General / Hotkeys / Blocked apps / Privacy & about pages, cards,
  toggle switches, a segmented mode picker, key caps for hotkeys, and an accent
  colour. Changes in Settings apply immediately (no Apply/OK).
- **Learned words are editable.** Settings → Learned words lists every word
  RightType has learned; add, remove or clear them and Save — changes take
  effect at once.
- **Typeface:** IBM Plex Sans Thai is embedded and used for every window and
  the toast, so Thai and English share one modern design.
- **Thai interface.** Every window, the tray menu and all messages are available
  in Thai; the language follows Windows and can be switched in Settings.
- **Light and dark** — the windows follow the Windows app theme (previously
  always dark).
- **Sharp at any display scaling** — RightType is DPI-aware; at 125–200 %
  Windows used to stretch its windows and toast into a blur.
- **New icon**, with a grey tray icon while RightType is off; the `.exe` now
  carries the icon and version information.
- Compounds of everyday English words (`middleware`, `workflow`, `frontend`,
  `codebase`) are treated as English.
- Learned words take effect immediately in every decision (they were stored
  but never used). Flipping back an automatic conversion (`Shift`+`Backspace`
  or Undo) with learning on remembers that word at once, in either language.
- Tray: left-click opens the menu, new **Hotkeys & help** item, and the tooltip
  shows the current mode. The Suggest hint shows the suggested text.
- Dictionaries are loaded at startup instead of on the first keystroke.

### Build and release

- Cargo.toml is the single source of the version; `build_release.ps1` passes it
  to the installer, and the portable zip now carries the licenses and this
  changelog.
- Release artifacts are no longer committed to the repository: the Release
  workflow builds them on Windows and publishes them as GitHub Release assets,
  with install instructions in Thai and English.
- CI now builds, lints and tests the Windows integration layer and runs
  `cargo audit`.

## [1.0.0] — 2026-08-31

First release: Thai Kedmanee ↔ US English QWERTY, Manual/Auto/Suggest modes,
sensitive-context guards, per-user installer and portable zip (unsigned).
