# Plan: ghost suggestions / palette, and focused-app rules

Two features asked for after the first release. Both come down to the same missing piece:
**on Wayland, no ordinary application can see which window has focus, and none can draw over
another window.** Both need one small **GNOME Shell extension** — code that runs *inside* the
compositor, where both of those things are normal.

This is a plan, not a promise: phases are ordered so each one is useful and verifiable on its
own, and the riskiest work is last.

## What was measured first (on the X13, Ubuntu 26.04)

| Question | Result |
| --- | --- |
| GNOME Shell version | **50.1** (extensions must target this; GNOME 45+ uses ESM) |
| `org.gnome.Shell.Introspect.GetWindows` (would list windows + focus) | **AccessDenied — "GetWindows is not allowed"** |
| `org.gnome.Shell.Eval` (would run shell code) | **blocked** — returns `(false, '')` |
| AT-SPI bus (accessibility, the only caret/text API on Wayland) | present (`/run/user/1000/at-spi/bus`, `at-spi2-registryd` running) |
| `toolkit-accessibility` setting | **false** — applications do not expose their widgets until this is on |
| Python `Atspi` bindings | **available** (python3-gi) |
| Extension tooling | `gnome-extensions` present, `~/.local/share/gnome-shell/extensions/` in use (e.g. `quick-lang-switch`) |

So: the two APIs a normal program would use are denied (as designed, since GNOME 41), and the
accessibility route exists but is off by default and costs performance. The extension is not a
workaround — it is the only supported place from which a program may see focus and draw
overlays on Wayland.

## Architecture: one extension, one channel

```
 keys ─► daemon (Rust)  ──socket /run/user/$UID/openrtlinux.sock──►  extension (GJS, in the shell)
           │  decides + types                                          │  draws ghost / palette
           └─ knows the app id, applies per-app rules ◄───────────────┘  reports focus
```

* The extension talks to the daemon over the **existing control socket** (`Gio.SocketClient`),
  so the daemon needs no new dependencies. Polling at ~150 ms while the palette is open is
  plenty; if we later want push, the same protocol moves to D-Bus.
* Keys stay in the daemon (it already reads evdev); drawing stays in the shell. Neither does
  the other's job.
* New socket commands (additive, existing ones unchanged): `candidates` → the list of possible
  readings for the word at the cursor, `apply <n>` → do the one chosen, `focus <app-id>` → the
  extension telling the daemon what is focused.

## Feature B — focused-app rules (do this first: smallest, and it unblocks A)

What it buys: skip a program entirely (a terminal, a game, a password field), or give a
program its own rules. Upstream already has the machinery for per-app behaviour
(`src/app_quality.rs`) — it only needs to be told which app is focused.

* **B1 — the extension reports focus.** `metadata.json` + `extension.js` (ESM) +
  `global.display.focus_window` → `get_wm_class()` / `get_gtk_application_id()` / `get_pid()`,
  sent as `focus <app-id>` whenever it changes. Deliverable: `journalctl` shows the app id
  changing as the user alt-tabs. Small, and it proves the whole extension pipeline.
* **B2 — the daemon uses it.** An `app_id` inside the daemon's state, and a policy filter
  before any fix: a config list of apps to ignore, plus per-app overrides. The auto/manual
  behaviour for other apps is untouched.
* **B3 — the settings window lists apps.** "Fix in this program: yes/no", fed from the apps
  seen so far (the extension reports them anyway). Nothing fancy: a list of checkboxes.

Estimated: B1 a short session, B2 half a session, B3 half a session.

## Feature A — ghost suggestion and keyboard palette

What it buys: see the correction *before* accepting it, and pick from a list instead of
blindly trusting the fixer.

* **A1 — the daemon can answer "what could this word be?"** A `candidates` command returning
  the deliberate-press reading, the word-checker reading, and dictionary completions
  (`dict::sure_completion` already exists). Verifiable from the shell with no UI at all.
* **A2 — a ghost line while typing.** The daemon tells the extension the current word and its
  best reading; the extension shows it in an overlay `St.Label`. **Honest limitation: without
  accessibility there is no caret position on Wayland**, so the first version draws the ghost
  in a fixed place — an OSD strip at the bottom of the screen, like GNOME's own OSD — rather
  than inline after the cursor. Inline needs A4.
* **A3 — the palette.** A small list near the focused window (its rect is known to the
  extension): the candidates, arrow keys to move, Enter to apply (sends `apply <n>`; the
  daemon does the deleting and typing exactly as it does for a fix). Escape closes. A
  keybinding opens it on demand (the extension registers one through its own GSettings
  schema) — nothing is drawn while the typist is not asking.
* **A4 — inline, caret-accurate ghost (optional, opt-in).** Turn on
  `toolkit-accessibility` and read the caret with AT-SPI (`get_character_extents`) to place the
  ghost exactly after the cursor. Costs: applications build an accessibility tree (slower
  start, some apps need `--force-renderer-accessibility`, terminals expose nothing), and it
  means reading what is on screen — so it stays a checkbox, off by default, with the trade-off
  written next to it.

Estimated: A1 a short session, A2 a session, A3 one to two sessions, A4 one to two sessions
with a lot of per-application testing.

## Packaging and updates

`packaging/linux/install.sh` gains `--with-extension`: copies the extension into
`~/.local/share/gnome-shell/extensions/openrtlinux@yume/` and enables it with
`gnome-extensions enable`. The settings window gets a switch for it (default off: the fixer
must keep working without any extension installed). Note that a newly installed extension
can be enabled without logging out; editing its code only needs disable/enable, which is the
normal Wayland development loop.

## Risks

1. **An extension bug can freeze the whole desktop** — it runs on the compositor's main loop.
   Rule: keep `extension.js` tiny, wrap every callback, never do synchronous I/O on that loop.
2. **GNOME moves / breaks extensions between releases.** Pin `shell-version` honestly and
   expect a small fix per GNOME release.
3. **Privacy.** A ghost suggestion means showing what is being typed. The extension only ever
   receives the single word the daemon is considering, never the surrounding text, and only
   when the feature is on.
4. **Performance of accessibility** (A4): measurable, and why it is opt-in.
5. **Nothing here helps other desktops.** KDE and friends would need their own layer.

## Decisions wanted

1. **The palette's key** — proposed `Super+Shift+Space` (Super+Space is GNOME's own input
   switcher). Any preference?
2. **Where the ghost appears** — bottom-of-screen OSD (works everywhere, no accessibility) or
   wait for A4's inline version (needs accessibility on)?
3. **Which apps to ignore by default** — terminals and password fields are the obvious ones;
   is there anything else?

## Known-good checkpoint

Everything above starts from **v0.1.0** (`https://github.com/yumehermes8-spec/openRTLinux/releases/tag/v0.1.0`,
commit `19fda01`): four modes, both hotkeys, 317 unit tests, fixes measured at 13–27 ms, and a
clean migration to the `openrtlinux` names. If a later change misbehaves, this is the state to
come back to.
