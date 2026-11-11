#!/usr/bin/env python3
"""openrtlinux tray + settings window.

    openrtlinux-tray                 # tray icon, and the settings window on demand
    openrtlinux-tray --window        # open the settings window straight away
    openrtlinux-tray --report        # print the daemon's status and exit
    openrtlinux-tray --selftest      # build every widget, print OK, exit (for CI/SSH)

Why Python and not Rust: the port's own layer is Rust, but GTK *bindings* need
`libgtk-3-dev` + pkg-config, which this box cannot install without a password.
python3-gi, GTK3 and libayatana-appindicator are already here, and the daemon is
controlled over a socket, so the language of the window makes no difference.

Talks to the daemon on `/run/user/$UID/openrtlinux.sock`; writes
`~/.config/openrtlinux/config.toml` and asks the daemon to reload it.
"""

from __future__ import annotations

import argparse
import fcntl
import json
import os
import re
import threading
import urllib.request
import socket
import subprocess
import time
import sys
from pathlib import Path

import gi

gi.require_version("Gtk", "3.0")
from gi.repository import GLib, Gtk  # noqa: E402

try:  # the tray needs the AppIndicator library, the window does not
    gi.require_version("AyatanaAppIndicator3", "0.1")
    from gi.repository import AyatanaAppIndicator3 as AppIndicator  # noqa: E402

    HAVE_TRAY = True
except Exception:  # pragma: no cover - depends on the machine
    AppIndicator = None
    HAVE_TRAY = False

APP = "openrtlinux"
LOCK_PATH = Path(
    os.environ.get("XDG_RUNTIME_DIR", "/tmp")
) / f"{APP}-tray.lock"


def claim_single_instance() -> "int | None":
    """One tray per session.

    Two copies means two icons in the panel, and two copies is exactly what happens
    when a login starts the tray by more than one route (the user service *and* an
    XDG autostart entry, say). The lock is held for as long as the process lives, so
    a second copy simply exits instead of drawing a second icon.
    """
    fd = os.open(LOCK_PATH, os.O_CREAT | os.O_RDWR, 0o600)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except OSError:
        os.close(fd)
        return None
    os.ftruncate(fd, 0)
    os.write(fd, str(os.getpid()).encode())
    return fd


WINDOW_REQUEST_PATH = Path(
    os.environ.get("XDG_RUNTIME_DIR", "/tmp")
) / f"{APP}-tray.window"


def request_window() -> None:
    """Ask the tray that already owns the panel icon to open its settings window.

    The launcher runs in the graphical session; a second copy started from the menu
    has no display of its own worth using, so it drops a request and lets the running
    tray (which does) open the window.
    """
    try:
        WINDOW_REQUEST_PATH.write_text(str(os.getpid()))
    except OSError:
        pass


def consume_window_request() -> bool:
    if not WINDOW_REQUEST_PATH.exists():
        return False
    try:
        WINDOW_REQUEST_PATH.unlink()
    except OSError:
        pass
    return True


ICON_ON = "openrtlinux"
ICON_OFF = "openrtlinux-off"
# the chords a person can pick for "fix the word under the cursor"
HOTKEYS = [
    ("shift+backspace", "Shift+Backspace"),
    ("shift+spacebar", "Shift+Spacebar"),
]


def toml_value(path: Path, key: str, default: object) -> object:
    """One key out of config.toml without building anything (the language needs it
    before the first label exists)."""
    try:
        return tomllib.loads(path.read_text()).get(key, default)
    except Exception:
        return default


def normalise_language(text: object) -> str:
    """Whatever the config says -> a key of LANGUAGE_SETTINGS."""
    choice = str(text or "system").strip().lower()
    return choice if choice in ("system", "th", "en") else "system"


def normalise_hotkey(text: object) -> str:
    """Whatever the config says -> one of HOTKEYS."""
    raw = str(text or "").lower().replace(" ", "").replace("_", "")
    if "backspace" in raw:
        return "shift+backspace"
    if "space" in raw:
        return "shift+spacebar"
    return "shift+backspace"


CONFIG_PATH = Path(
    os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")
) / APP / "config.toml"
# The typist's own words. The daemon reads it, the daemon appends what it learns,
# and this window edits it - one file, so `reload` is all the two sides need.
WORDS_PATH = CONFIG_PATH.with_name("words.toml")

# Where this port lives, for the About section and its update check.
REPO = "yumehermes8-spec/openRTLinux"
REPO_URL = f"https://github.com/{REPO}"
RELEASES_URL = f"{REPO_URL}/releases"
# The pause chord the daemon reads (see `is_pause_chord` in the daemon).
PAUSE_CHORD = "Ctrl+Alt+P"


def version_parts(text: str) -> list:
    """`v0.4.0` -> [0, 4, 0]; anything unparseable counts as zero."""
    return [int(bit) if bit.isdigit() else 0 for bit in re.split(r"[.\-+]", text.strip().lstrip("vV"))] or [0]


def is_newer(latest: str, running: str) -> bool:
    """Numbers, not strings: 0.10.0 is newer than 0.9.9."""
    a, b = version_parts(latest), version_parts(running)
    return (a + [0] * len(b)) > (b + [0] * len(a))


def fetch_latest_release() -> dict:
    """The newest release from the GitHub API - only the update button calls it.

    RightType itself never goes online; this is one request the typist asks for, on
    the button, and the answer is shown as text. Nothing is downloaded or run.
    """
    url = f"https://api.github.com/repos/{REPO}/releases/latest"
    request = urllib.request.Request(
        url,
        headers={"User-Agent": "openRTLinux-settings", "Accept": "application/vnd.github+json"},
    )
    try:
        with urllib.request.urlopen(request, timeout=8) as reply:
            data = json.loads(reply.read().decode("utf-8"))
    except Exception as exc:  # noqa: BLE001 - offline, rate-limited, renamed repo
        return {"error": f"{type(exc).__name__}: {exc}"}
    return {"tag": str(data.get("tag_name", "")), "url": str(data.get("html_url", ""))}
SOCKET_PATH = Path(os.environ.get("XDG_RUNTIME_DIR", "/tmp")) / f"{APP}.sock"
# ---------------------------------------------------------------- languages
# The settings window and the tray menu speak Thai or English. The *behaviour* is
# the same either way - this only changes the words, so someone using a Thai
# Ubuntu in another language can still find their way around.
# the order the modes are offered in: the automatic one, the deliberate one, the
# one that does both, then off
MODES = ("auto", "manual", "both", "off")

LANGUAGE_SETTINGS = [
    ("system", "ตามระบบ / Follow system"),
    ("th", "ไทย / Thai"),
    ("en", "English / English"),
]

TEXTS = {
    "th": {
        "window_title": "openRTLinux — ตั้งค่า",
        "language": "ภาษา",
        "language_tip": "ภาษาของหน้าตั้งค่าและเมนูถาด (การทำงานเหมือนกันทุกภาษา)",
        "mode": "โหมด",
        "dry_run": "โหมดทดลอง — ตัดสินใจและบันทึก แต่ไม่พิมพ์/ไม่สลับแป้น",
        "hotkey": "hotkey แก้คำ",
        "hotkey_tip": "คอร์ดที่กดแล้วแก้คำใต้เคอร์เซอร์ — Shift+Spacebar จะลบวรรคที่เกิดขึ้นให้ด้วย",
        "switch_key": "ปุ่มสลับแป้นของเดสก์ท็อป",
        "current_command": "คำสั่งอ่านแป้นปัจจุบัน",
        "autostart": "เปิด tray อัตโนมัติเมื่อ login",
        "words": "คำของฉัน",
        "words_learn": "เรียนรู้คำที่ฉันพิมพ์บ่อย (อัตโนมัติ)",
        "words_learn_tip": "พิมพ์คำเดิมครบ 3 ครั้ง = จำว่าเป็นคำของฉัน แล้วจะไม่ถูกแก้ให้อีก",
        "words_placeholder": "เพิ่มคำ เช่น kubernetes",
        "words_add": "เพิ่มคำ",
        "words_pair_wrong": "ที่พิมพ์ (ผิด)",
        "words_pair_right": "ให้เป็น",
        "words_pair_add": "เพิ่มคู่",
        "words_remove_tip": "เอาออกจากรายการ",
        "words_hint": "คำในรายการนี้จะไม่ถูกแก้ · คู่คำจะถูกแก้ให้ทันทีทั้งตอนเว้นวรรคและตอนกด hotkey",
        "about": "เกี่ยวกับ",
        "version": "เวอร์ชัน",
        "releases": "ดู release ทั้งหมด",
        "update": "ตรวจอัปเดต",
        "update_checking": "กำลังตรวจ…",
        "update_current": "เป็นเวอร์ชันล่าสุดแล้ว ({v})",
        "update_available": "มีเวอร์ชันใหม่: {latest}",
        "update_failed": "ตรวจไม่ได้: {why}",
        "pause_hint": "พัก/แก้ต่อ: กด <b>{chord}</b> (ไม่ต้องเปิดถาด)",
        "code_guard": "ไม่แตะคำที่เป็นโค้ด (camelCase, snake_case, ALLCAPS, มีตัวเลข)",
        "code_guard_tip": "โหมดโค้ด: ชื่อในโค้ดไม่ใช่คำพิมพ์ผิด และไทยที่อ่านออกมาเป็นชื่อจะถูกแก้กลับเป็นอังกฤษ · เปิดแล้วปุ่ม hotkey ก็จะไม่แก้คำทรงนี้เช่นกัน",
        "note": ("<small>หมายเหตุ: พอร์ตนี้แก้คำเมื่อเคาะวรรค (auto) และเมื่อกด hotkey (manual) — "
                 "ฟีเจอร์ที่ต้องอ่านข้อความจากแอป (ghost suggestion, per-app) ยังไม่ได้ทำบน Wayland</small>"),
        "save": "บันทึกแล้วโหลดใหม่",
        "refresh": "รีเฟรชสถานะ",
        "saved": "บันทึกแล้ว — ",
        "history": "ประวัติการแก้ (ล่าสุด)",
        "history_empty": "(รอบนี้ยังไม่มีการแก้)",
        "undo": "ย้อนการแก้ครั้งล่าสุด",
        "undo_tip": "ย้อนได้เฉพาะตอนที่ยังไม่ได้พิมพ์อะไรต่อหลังการแก้ครั้งล่าสุด",
        "undoing": "กำลังย้อน… ",
        "reachable_no": "ทักไดมอนไม่ได้",
        "daemon_off": "daemon: ไม่ได้ทำงาน",
        "dry": "⚠ ทดลอง — ไม่พิมพ์ (ปิดสวิตช์ 'โหมดทดลอง' เพื่อให้แก้จริง)",
        "real": "แก้จริง",
        "layout_th": "ไทย",
        "layout_en": "อังกฤษ",
        "committed": "แก้ไป {n} ครั้ง",
        "mode_auto": "แก้เองเมื่อเคาะวรรค (auto)",
        "mode_manual": "แก้เมื่อกด hotkey (manual)",
        "mode_both": "both — วรรค + hotkey",
        "mode_off": "ปิดการแก้ (off)",
        "tray_dry": "โหมดทดลอง (ไม่พิมพ์จริง)",
        "tray_start": "เริ่ม daemon",
        "tray_settings": "ตั้งค่า…",
        "tray_log": "ดู log (journal)",
        "tray_quit": "หยุด daemon",
        "tray_exit": "ปิด tray นี้",
        "notify": "เริ่มแก้จริงแล้ว — จะพิมพ์/สลับแป้นให้",
    },
    "en": {
        "window_title": "openRTLinux — Settings",
        "language": "Language",
        "language_tip": "Language of the settings window and the tray menu (behaviour is the same either way)",
        "mode": "Mode",
        "dry_run": "Dry run — decide and log, but do not type or switch the layout",
        "hotkey": "Fix hotkey",
        "hotkey_tip": "The chord that fixes the word under the cursor — Shift+Spacebar also removes the space it types",
        "switch_key": "Desktop switch key",
        "current_command": "Read the active layout with",
        "autostart": "Start the tray at login",
        "words": "My words",
        "words_learn": "Learn words I keep typing (automatic)",
        "words_learn_tip": "Three sightings of the same word keep it as mine - it is never fixed again",
        "words_placeholder": "Add a word, e.g. kubernetes",
        "words_add": "Add word",
        "words_pair_wrong": "what I typed",
        "words_pair_right": "what it should be",
        "words_pair_add": "Add pair",
        "words_remove_tip": "Remove from the list",
        "words_hint": "Words here are never fixed - a pair is applied at a space and on the hotkey",
        "about": "About",
        "version": "Version",
        "releases": "All releases",
        "update": "Check for updates",
        "update_checking": "Checking…",
        "update_current": "This is the newest version ({v})",
        "update_available": "A newer version exists: {latest}",
        "update_failed": "Could not check: {why}",
        "pause_hint": "Pause / resume: press <b>{chord}</b> (no need to open the tray)",
        "code_guard": "Leave code alone (camelCase, snake_case, ALLCAPS, digits)",
        "code_guard_tip": "Code mode: a name in code is not a typo, and Thai that reads as a name goes back to English · with it on, the fix hotkey leaves those alone too",
        "note": ("<small>Note: this port fixes a word on a boundary (auto) and on the fix hotkey "
                 "(manual) — features that need to read text from the app (ghost suggestion, "
                 "per-app rules) are not on Wayland yet</small>"),
        "save": "Save and reload",
        "refresh": "Refresh status",
        "saved": "Saved — ",
        "history": "Recent fixes",
        "history_empty": "(nothing fixed yet in this session)",
        "undo": "Undo the last fix",
        "undo_tip": "Only possible while nothing has been typed since that fix",
        "undoing": "Undoing… ",
        "reachable_no": "cannot reach the daemon",
        "daemon_off": "daemon: not running",
        "dry": "⚠ dry run — not typing (turn off \"Dry run\" to fix for real)",
        "real": "fixing",
        "layout_th": "Thai",
        "layout_en": "English",
        "committed": "fixed {n} time(s)",
        "mode_auto": "Fix on space or Enter (auto)",
        "mode_manual": "Fix on the hotkey (manual)",
        "mode_both": "both - space and hotkey",
        "mode_off": "Off",
        "tray_dry": "Dry run (do not type)",
        "tray_start": "Start the daemon",
        "tray_settings": "Settings…",
        "tray_log": "View log (journal)",
        "tray_quit": "Stop the daemon",
        "tray_exit": "Close this tray",
        "notify": "Now fixing for real — keys and the layout will change",
    },
}

LANG = "th"
# set by --lang: a one-off choice that outranks whatever the config says
LANG_OVERRIDE: "str | None" = None


def system_language() -> str:
    """Thai if the session speaks Thai, English otherwise."""
    for name in ("LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"):
        value = os.environ.get(name, "")
        if value:
            return "th" if value.lower().startswith("th") else "en"
    return "en"


def resolve_language(setting: object) -> str:
    choice = str(setting or "system").strip().lower()
    if choice in ("th", "en"):
        return choice
    return system_language()


def t(key: str, **fmt: object) -> str:
    text = TEXTS.get(LANG, TEXTS["th"]).get(key) or TEXTS["th"].get(key, key)
    return text.format(**fmt) if fmt else text


def mode_label(key: str) -> str:
    return t(
        {
            "auto": "mode_auto",
            "manual": "mode_manual",
            "both": "mode_both",
            "off": "mode_off",
        }.get(key, key)
    )


# --------------------------------------------------------------------- daemon
def ask(command: str, timeout: float = 2.0) -> dict:
    """One line to the daemon, one JSON line back."""
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
            sock.settimeout(timeout)
            sock.connect(str(SOCKET_PATH))
            sock.sendall((command + "\n").encode("utf-8"))
            reply = sock.recv(65536).decode("utf-8", "replace").strip()
        return json.loads(reply) if reply else {"error": "empty reply"}
    except FileNotFoundError:
        return {"error": f"daemon not running (no socket at {SOCKET_PATH})"}
    except Exception as exc:  # noqa: BLE001 - surfaced in the UI
        return {"error": str(exc)}


def daemon_running() -> bool:
    return "error" not in ask("status")


def start_daemon() -> str:
    """Start the daemon - through systemd when there is a user manager.

    Spawning it directly leaves an unreaped child (and, when the tray dies, an
    orphan that keeps typing); the user unit is supervised and restartable.
    """
    try:
        done = subprocess.run(
            ["systemctl", "--user", "start", "openrtlinux.service"],
            capture_output=True,
            text=True,
            timeout=15,
        )
        if done.returncode == 0:
            return "started (systemd)"
    except Exception:  # noqa: BLE001 - fall through to plain spawn
        pass
    exe = shutil_which("openrtlinux") or str(Path.home() / ".local/bin" / "openrtlinux")
    try:
        subprocess.Popen(
            [exe],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
    except Exception as exc:  # noqa: BLE001
        return f"start failed: {exc}"
    return "started"


def shutil_which(name: str) -> str | None:
    for directory in os.environ.get("PATH", "").split(os.pathsep):
        candidate = Path(directory) / name
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return str(candidate)
    return None


def gsettings(command: str) -> str:
    try:
        out = subprocess.run(
            ["sh", "-c", command], capture_output=True, text=True, timeout=5
        )
        return out.stdout.strip()
    except Exception:  # noqa: BLE001
        return ""


def describe(status: dict) -> str:
    if "error" in status:
        return t("daemon_off")
    mode = mode_label(status.get("mode", "?"))
    switch = t("dry") if status.get("dry_run") else t("real")
    layout = t("layout_th") if status.get("layout") == "thai" else t("layout_en")
    return f"{mode} · {switch} · {layout} · {t('committed', n=status.get('committed', 0))}"


# ------------------------------------------------------------------ settings
class SettingsWindow(Gtk.Window):
    def __init__(self, tray=None) -> None:
        super().__init__(title=t("window_title"))
        self.tray = tray
        self.labels: dict[str, object] = {}
        self.set_default_size(560, 620)
        self.set_border_width(12)

        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        self.add(box)

        self.status = Gtk.Label()
        self.status.set_xalign(0)
        self.status.set_halign(Gtk.Align.START)
        self.status.set_line_wrap(True)
        box.pack_start(self.status, False, False, 0)

        grid = Gtk.Grid(column_spacing=12, row_spacing=8)
        box.pack_start(grid, False, False, 0)
        row = 0

        self.lang = Gtk.ComboBoxText()
        for key, label in LANGUAGE_SETTINGS:
            self.lang.append(key, label)
        self.lang.set_tooltip_text(t("language_tip"))
        self.lang.connect("changed", self.on_language)
        self.labels["language"] = label_start(t("language"))
        grid.attach(self.labels["language"], 0, row, 1, 1)
        grid.attach(self.lang, 1, row, 1, 1)
        row += 1

        self.mode = Gtk.ComboBoxText()
        for key in MODES:
            self.mode.append(key, mode_label(key))
        self.labels["mode"] = label_start(t("mode"))
        grid.attach(self.labels["mode"], 0, row, 1, 1)
        grid.attach(self.mode, 1, row, 1, 1)
        row += 1

        self.dry_run = Gtk.CheckButton(label=t("dry_run"))
        grid.attach(self.dry_run, 1, row, 1, 1)
        row += 1

        self.hotkey = Gtk.ComboBoxText()
        for key, label in HOTKEYS:
            self.hotkey.append(key, label)
        self.hotkey.set_tooltip_text(t("hotkey_tip"))
        self.labels["hotkey"] = label_start(t("hotkey"))
        grid.attach(self.labels["hotkey"], 0, row, 1, 1)
        grid.attach(self.hotkey, 1, row, 1, 1)
        row += 1

        # Two fields a typist never touches - the desktop's own switch key and the
        # command that reads the active layout - are deliberately *not* on this
        # screen: what is left is only the choices somebody actually makes. They
        # are still built, loaded and saved, so config.toml keeps them and nothing
        # that reads those widgets has to learn about their absence.
        self.switch_key = Gtk.Entry()
        self.labels["switch_key"] = label_start(t("switch_key"))
        self.current_command = Gtk.Entry()
        self.labels["current_command"] = label_start(t("current_command"))

        self.autostart = Gtk.CheckButton(label=t("autostart"))
        grid.attach(self.autostart, 1, row, 1, 1)
        row += 1

        self.code_guard = Gtk.CheckButton(label=t("code_guard"))
        self.code_guard.set_tooltip_text(t("code_guard_tip"))
        grid.attach(self.code_guard, 1, row, 1, 1)
        row += 1

        hint = Gtk.Label()
        hint.set_xalign(0)
        hint.set_halign(Gtk.Align.START)
        hint.set_line_wrap(True)
        self.hint = hint
        hint.set_markup(t("note")
        )
        box.pack_start(hint, False, False, 0)

        # ---- what it fixed lately, and one button to put the last one back
        self.labels["history"] = label_start(t("history"))
        box.pack_start(self.labels["history"], False, False, 0)
        scroll = Gtk.ScrolledWindow()
        scroll.set_policy(Gtk.PolicyType.AUTOMATIC, Gtk.PolicyType.AUTOMATIC)
        scroll.set_min_content_height(120)
        self.history_box = Gtk.ListBox()
        self.history_box.set_selection_mode(Gtk.SelectionMode.NONE)
        scroll.add(self.history_box)
        box.pack_start(scroll, True, True, 0)
        self.undo_button = Gtk.Button(label=t("undo"))
        self.undo_button.set_tooltip_text(t("undo_tip"))
        self.undo_button.set_sensitive(False)
        self.undo_button.connect("clicked", self.on_undo)
        box.pack_start(self.undo_button, False, False, 0)

        # ---- the typist's own words: never fix these, and these pairs are theirs
        self.labels["words"] = label_start(t("words"))
        box.pack_start(self.labels["words"], False, False, 0)
        self.words_learn = Gtk.CheckButton(label=t("words_learn"))
        self.words_learn.set_tooltip_text(t("words_learn_tip"))
        self.words_learn.connect("toggled", self.on_words_learn)
        box.pack_start(self.words_learn, False, False, 0)

        add_row = Gtk.Box(spacing=6)
        self.word_entry = Gtk.Entry()
        self.word_entry.set_placeholder_text(t("words_placeholder"))
        self.word_entry.connect("activate", self.on_words_add)
        add_row.pack_start(self.word_entry, True, True, 0)
        self.labels["words_add"] = Gtk.Button(label=t("words_add"))
        self.labels["words_add"].connect("clicked", self.on_words_add)
        add_row.pack_start(self.labels["words_add"], False, False, 0)
        box.pack_start(add_row, False, False, 0)
        box.pack_start(self.word_list(), True, True, 0)

        pair_row = Gtk.Box(spacing=6)
        self.pair_wrong = Gtk.Entry()
        self.pair_wrong.set_placeholder_text(t("words_pair_wrong"))
        pair_row.pack_start(self.pair_wrong, True, True, 0)
        self.pair_right = Gtk.Entry()
        self.pair_right.set_placeholder_text(t("words_pair_right"))
        self.pair_right.connect("activate", self.on_pair_add)
        pair_row.pack_start(self.pair_right, True, True, 0)
        self.labels["words_pair_add"] = Gtk.Button(label=t("words_pair_add"))
        self.labels["words_pair_add"].connect("clicked", self.on_pair_add)
        pair_row.pack_start(self.labels["words_pair_add"], False, False, 0)
        box.pack_start(pair_row, False, False, 0)
        box.pack_start(self.pair_list(), True, True, 0)

        self.labels["about"] = label_start(t("about"))
        box.pack_start(self.labels["about"], False, False, 0)
        self.version_label = Gtk.Label(xalign=0)
        box.pack_start(self.version_label, False, False, 0)
        self.pause_label = Gtk.Label(xalign=0)
        self.pause_label.set_line_wrap(True)
        box.pack_start(self.pause_label, False, False, 0)
        self.link = Gtk.LinkButton(uri=REPO_URL, label=REPO_URL)
        box.pack_start(self.link, False, False, 0)
        update_row = Gtk.Box(spacing=8)
        self.labels["update"] = Gtk.Button(label=t("update"))
        self.labels["update"].connect("clicked", self.on_check_update)
        update_row.pack_start(self.labels["update"], False, False, 0)
        self.update_status = Gtk.Label(xalign=0)
        self.update_status.set_line_wrap(True)
        update_row.pack_start(self.update_status, True, True, 0)
        box.pack_start(update_row, False, False, 0)
        self.release_link = Gtk.LinkButton(uri=RELEASES_URL, label=t("releases"))
        box.pack_start(self.release_link, False, False, 0)

        self.hint_words = Gtk.Label()
        self.hint_words.set_xalign(0)
        self.hint_words.set_halign(Gtk.Align.START)
        self.hint_words.set_line_wrap(True)
        self.hint_words.set_markup(t("words_hint"))
        box.pack_start(self.hint_words, False, False, 0)
        self.ready = True

        buttons = Gtk.Box(spacing=8)
        box.pack_end(buttons, False, False, 0)
        save = Gtk.Button(label=t("save"))
        self.labels["save"] = save
        save.connect("clicked", self.on_save)
        buttons.pack_end(save, False, False, 0)
        refresh = Gtk.Button(label=t("refresh"))
        self.labels["refresh"] = refresh
        refresh.connect("clicked", lambda *_: self.reload_status())
        buttons.pack_end(refresh, False, False, 0)

        self.load_config()
        self.reload_status()
        GLib.timeout_add(1500, self._tick)

    # ---- the typist's own words
    def word_list(self) -> Gtk.ScrolledWindow:
        scroll = Gtk.ScrolledWindow()
        scroll.set_policy(Gtk.PolicyType.AUTOMATIC, Gtk.PolicyType.AUTOMATIC)
        scroll.set_min_content_height(80)
        self.words_box = Gtk.ListBox()
        self.words_box.set_selection_mode(Gtk.SelectionMode.NONE)
        scroll.add(self.words_box)
        return scroll

    def pair_list(self) -> Gtk.ScrolledWindow:
        scroll = Gtk.ScrolledWindow()
        scroll.set_policy(Gtk.PolicyType.AUTOMATIC, Gtk.PolicyType.AUTOMATIC)
        scroll.set_min_content_height(70)
        self.pairs_box = Gtk.ListBox()
        self.pairs_box.set_selection_mode(Gtk.SelectionMode.NONE)
        scroll.add(self.pairs_box)
        return scroll

    def read_words(self) -> dict:
        """The file the daemon reads, in the same shape: one key per line."""
        data: dict = {"learn": False, "words": [], "pairs": []}
        try:
            text = WORDS_PATH.read_text()
        except OSError:
            return data
        for line in text.splitlines():
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            key, _, value = line.partition("=")
            key, value = key.strip(), value.strip()
            if key == "learn":
                data["learn"] = value in ("true", "1", "yes", "on")
            elif key == "word" and value:
                data["words"].append(value)
            elif key == "pair" and "=" in value:
                wrong, _, right = value.partition("=")
                if wrong.strip() and right.strip():
                    data["pairs"].append((wrong.strip(), right.strip()))
        return data

    def write_words(self, data: dict) -> str:
        lines = [
            "# openRTLinux - the typist's own words (this window writes it).",
            "# word = never fix this one \u00b7 pair = what I typed=what it should be",
            f"learn = {'true' if data['learn'] else 'false'}",
        ]
        lines += [f"word = {word}" for word in data["words"]]
        lines += [f"pair = {wrong}={right}" for wrong, right in data["pairs"]]
        try:
            WORDS_PATH.parent.mkdir(parents=True, exist_ok=True)
            WORDS_PATH.write_text("\n".join(lines) + "\n")
        except OSError as exc:
            return f"could not save: {exc}"
        ask("reload")  # the daemon re-reads both files, exactly like a config save
        return "saved"

    def render_words(self) -> None:
        data = self.read_words()
        try:
            self.words_stamp = WORDS_PATH.stat().st_mtime_ns
        except OSError:
            self.words_stamp = None
        self.words_learn.handler_block_by_func(self.on_words_learn)
        self.words_learn.set_active(bool(data["learn"]))
        self.words_learn.handler_unblock_by_func(self.on_words_learn)
        for child in self.words_box.get_children():
            self.words_box.remove(child)
        for word in data["words"]:
            self.words_box.add(self.words_row(word))
        for child in self.pairs_box.get_children():
            self.pairs_box.remove(child)
        for wrong, right in data["pairs"]:
            self.pairs_box.add(self.pairs_row(wrong, right))
        self.words_box.show_all()
        self.pairs_box.show_all()

    def words_row(self, word: str) -> Gtk.ListBoxRow:
        row = Gtk.ListBoxRow()
        line = Gtk.Box(spacing=6)
        line.pack_start(Gtk.Label(label=word, xalign=0), True, True, 0)
        remove = Gtk.Button(label="\u00d7")
        remove.set_tooltip_text(t("words_remove_tip"))
        remove.connect("clicked", self.on_words_remove, word)
        line.pack_end(remove, False, False, 0)
        row.add(line)
        return row

    def pairs_row(self, wrong: str, right: str) -> Gtk.ListBoxRow:
        row = Gtk.ListBoxRow()
        line = Gtk.Box(spacing=6)
        line.pack_start(Gtk.Label(label=f"{wrong}  \u2192  {right}", xalign=0), True, True, 0)
        remove = Gtk.Button(label="\u00d7")
        remove.set_tooltip_text(t("words_remove_tip"))
        remove.connect("clicked", self.on_pair_remove, wrong)
        line.pack_end(remove, False, False, 0)
        row.add(line)
        return row

    def on_words_add(self, *_args) -> None:
        word = self.word_entry.get_text().strip()
        if not word:
            return
        data = self.read_words()
        if word not in data["words"]:
            data["words"].append(word)
        self.write_words(data)
        self.word_entry.set_text("")
        self.render_words()

    def on_words_remove(self, _button, word: str) -> None:
        data = self.read_words()
        data["words"] = [w for w in data["words"] if w != word]
        self.write_words(data)
        self.render_words()

    def on_pair_add(self, *_args) -> None:
        wrong = self.pair_wrong.get_text().strip()
        right = self.pair_right.get_text().strip()
        if not wrong or not right:
            return
        data = self.read_words()
        data["pairs"] = [(w, r) for w, r in data["pairs"] if w != wrong] + [(wrong, right)]
        self.write_words(data)
        self.pair_wrong.set_text("")
        self.pair_right.set_text("")
        self.render_words()

    def on_pair_remove(self, _button, wrong: str) -> None:
        data = self.read_words()
        data["pairs"] = [(w, r) for w, r in data["pairs"] if w != wrong]
        self.write_words(data)
        self.render_words()

    def on_words_learn(self, button) -> None:
        if not getattr(self, "ready", False):
            return
        data = self.read_words()
        data["learn"] = bool(button.get_active())
        self.write_words(data)

    # ---- about and updates
    def on_check_update(self, *_args) -> None:
        self.labels["update"].set_sensitive(False)
        self.update_status.set_text(t("update_checking"))

        def work() -> None:
            result = fetch_latest_release()
            GLib.idle_add(self.show_update_result, result)

        threading.Thread(target=work, daemon=True).start()

    def show_update_result(self, result: dict) -> None:
        self.labels["update"].set_sensitive(True)
        if "error" in result:
            self.update_status.set_text(t("update_failed").format(why=result["error"]))
            return
        running = getattr(self, "running_version", None) or "?"
        if is_newer(result["tag"], running):
            self.update_status.set_text(
                t("update_available").format(latest=result["tag"])
            )
            self.release_link.set_uri(result.get("url") or RELEASES_URL)
        else:
            self.update_status.set_text(t("update_current").format(v=running))

    def refresh_words_if_changed(self) -> None:
        """Show what the daemon learned, without rebuilding the list every tick."""
        try:
            stamp = WORDS_PATH.stat().st_mtime_ns
        except OSError:
            stamp = None
        if stamp != getattr(self, "words_stamp", "unset"):
            self.render_words()

    # ---- config file
    @staticmethod
    def defaults(self=None) -> dict:  # noqa: D401
        return {
            "mode": "auto",
            "dry_run": True,
            "current_command": "gsettings get org.gnome.desktop.input-sources mru-sources",
            "switch_command": (
                "gsettings get org.gnome.desktop.wm.keybindings switch-input-source"
            ),
            "tray_autostart": False,
            "layout": "",
            "fix_word_hotkey": "shift+backspace",
            "code_guard": False,
        }

    def load_config(self) -> None:  # noqa: D401
        data = dict(self.defaults())
        try:
            import tomllib  # python 3.11+

            data.update(tomllib.loads(CONFIG_PATH.read_text()))
        except Exception:  # noqa: BLE001 - missing file or old python
            pass
        self.config = data
        self.mode.set_active_id(str(data.get("mode", "auto")))
        self.dry_run.set_active(bool(data.get("dry_run", True)))
        global LANG
        chosen = LANG_OVERRIDE or data.get("ui_language", "system")
        self.lang.set_active_id(normalise_language(chosen))
        LANG = resolve_language(chosen)
        self.hotkey.set_active_id(normalise_hotkey(data.get("fix_word_hotkey", "shift+backspace")))
        self.switch_key.set_text(str(data.get("switch_command", "")))
        self.current_command.set_text(str(data.get("current_command", "")))
        self.autostart.set_active(bool(data.get("tray_autostart", False)))
        self.code_guard.set_active(bool(data.get("code_guard", False)))

    def on_language(self, *_args) -> None:
        """Preview the choice at once; saving is what stores it."""
        global LANG
        LANG = resolve_language(self.lang.get_active_id())
        if not getattr(self, "ready", False):
            return  # the window is still being built; every label is already right
        self.retranslate()

    def retranslate(self) -> None:
        """Say everything again in the chosen language."""
        self.set_title(t("window_title"))
        self.lang.set_tooltip_text(t("language_tip"))
        for key, widget in self.labels.items():
            # the save/refresh entries are buttons: same idea, different method
            if isinstance(widget, Gtk.Button):
                widget.set_label(t(key))
            else:
                widget.set_text(t(key))
        self.dry_run.set_label(t("dry_run"))
        self.autostart.set_label(t("autostart"))
        self.hotkey.set_tooltip_text(t("hotkey_tip"))
        self.hint.set_markup(t("note"))
        self.code_guard.set_label(t("code_guard"))
        self.code_guard.set_tooltip_text(t("code_guard_tip"))
        self.pause_label.set_markup(t("pause_hint").format(chord=PAUSE_CHORD))
        if hasattr(self, "release_link"):
            self.release_link.set_label(t("releases"))
        self.words_learn.set_label(t("words_learn"))
        self.words_learn.set_tooltip_text(t("words_learn_tip"))
        self.word_entry.set_placeholder_text(t("words_placeholder"))
        self.pair_wrong.set_placeholder_text(t("words_pair_wrong"))
        self.pair_right.set_placeholder_text(t("words_pair_right"))
        self.hint_words.set_markup(t("words_hint"))
        self.undo_button.set_label(t("undo"))
        self.undo_button.set_tooltip_text(t("undo_tip"))
        chosen = self.mode.get_active_id()
        self.mode.remove_all()
        for key in MODES:
            self.mode.append(key, mode_label(key))
        if chosen:
            self.mode.set_active_id(chosen)
        if self.tray is not None:
            self.tray.retranslate()
        self.reload_status()

    def on_save(self, *_args) -> None:
        self.config["mode"] = self.mode.get_active_id() or "auto"
        self.config["dry_run"] = self.dry_run.get_active()
        global LANG
        self.config["ui_language"] = self.lang.get_active_id() or "system"
        LANG = resolve_language(self.config["ui_language"])
        self.config["fix_word_hotkey"] = self.hotkey.get_active_id() or "shift+backspace"
        self.config["switch_command"] = self.switch_key.get_text()
        self.config["current_command"] = self.current_command.get_text()
        self.config["tray_autostart"] = self.autostart.get_active()
        self.config["code_guard"] = self.code_guard.get_active()
        CONFIG_PATH.parent.mkdir(parents=True, exist_ok=True)
        CONFIG_PATH.write_text(toml_dump(self.config))
        apply_autostart(self.config["tray_autostart"])
        result = ask("reload")
        # the daemon reads mode/dry_run from its own state; keep it in step
        ask(f"set mode {self.config['mode']}")
        ask(f"set dry_run {str(self.config['dry_run']).lower()}")
        self.status.set_text(
            t("saved") + json.dumps(result, ensure_ascii=False)
        )
        if self.tray is not None:
            self.tray.refresh()

    def render_history(self, status: dict) -> None:
        """The recent fixes, newest first, exactly as the daemon reports them."""
        if not getattr(self, "history_box", None):
            return
        for child in self.history_box.get_children():
            self.history_box.remove(child)
        items = status.get("history") or []
        if not items:
            row = Gtk.Label(label=t("history_empty"))
            row.set_xalign(0)
            row.set_sensitive(False)
            self.history_box.add(row)
        for item in items:
            when = time.strftime("%H:%M:%S", time.localtime(float(item.get("at", 0))))
            mark = "↩" if item.get("kind") == "swap" else "✓"
            row = Gtk.Label(
                label=f"{when}   {item.get('shown', '')} → {item.get('corrected', '')}   {mark}"
            )
            row.set_xalign(0)
            row.set_selectable(True)
            self.history_box.add(row)
        self.history_box.show_all()
        self.undo_button.set_sensitive(bool(status.get("can_undo")))

    def on_undo(self, *_args) -> None:
        answer = ask("undo")
        self.status.set_text(t("undoing") + json.dumps(answer, ensure_ascii=False))
        # the daemon does it on its next turn, then we look again
        GLib.timeout_add(700, self.reload_status)

    def reload_status(self) -> bool:
        status = ask("status")
        suffix = f"  [{status['error']}]" if "error" in status else ""
        self.status.set_text(describe(status) + suffix)
        self.running_version = str(status.get("version", "")) or None
        self.version_label.set_text(f"{t('version')} {self.running_version or '?'}")
        self.render_history(status)
        self.refresh_words_if_changed()
        return False

    def _tick(self) -> bool:
        self.reload_status()
        return True


def label_start(text: str):
    label = Gtk.Label(label=text)
    label.set_xalign(0)
    label.set_halign(Gtk.Align.START)
    return label


def write_config(updates: dict) -> str:
    """Persist a change the tray made, so a restart does not undo it.

    Without this the tray's switches lived only in the running daemon: quit and
    restart, and the config file put the old values back - which reads as "it
    stopped working".
    """
    data = SettingsWindow.defaults(None)  # type: ignore[arg-type]
    try:
        import tomllib

        data.update(tomllib.loads(CONFIG_PATH.read_text()))
    except Exception:  # noqa: BLE001
        pass
    data.update(updates)
    try:
        CONFIG_PATH.parent.mkdir(parents=True, exist_ok=True)
        CONFIG_PATH.write_text(toml_dump(data))
        return "saved"
    except Exception as exc:  # noqa: BLE001
        return f"could not save: {exc}"


def toml_dump(data: dict) -> str:
    lines = []
    for key, value in data.items():
        if isinstance(value, bool):
            lines.append(f"{key} = {'true' if value else 'false'}")
        elif isinstance(value, str):
            lines.append(f'{key} = "{value}"')
        else:
            lines.append(f"{key} = {value}")
    return "\n".join(lines) + "\n"


def apply_autostart(enabled: bool) -> str:
    """Start the tray at login - through the user service, which now owns it.

    This used to write an XDG autostart entry while the service unit was also
    enabled: logging in started the tray twice, and two trays mean two icons.
    """
    unit = "openrtlinux-tray.service"
    legacy = Path.home() / ".config" / "autostart" / "openrtlinux-tray.desktop"
    if legacy.exists():
        legacy.unlink()  # the unit replaced this; keeping both starts two copies
    action = "enable" if enabled else "disable"
    try:
        subprocess.run(
            ["systemctl", "--user", action, "--now", unit],
            check=True, capture_output=True, timeout=15,
        )
        return f"systemd: {unit} {action}d"
    except Exception as why:  # no systemd user session: fall back to autostart
        target = legacy
        if not enabled:
            return f"autostart off ({why})"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(
            "[Desktop Entry]\nType=Application\nName=openRTLinux tray\n"
            f"Exec={sys.executable} {Path(__file__).resolve()}\n"
            "Icon=openrtlinux\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
        )
        return f"autostart on ({target})"


# ----------------------------------------------------------------------- tray
def shlex_quote(text: str) -> str:
    return "'" + text.replace("'", "'\\''") + "'"

class Tray:
    def __init__(self) -> None:
        self.indicator = AppIndicator.Indicator.new(
            APP, ICON_ON, AppIndicator.IndicatorCategory.APPLICATION_STATUS
        )
        self.indicator.set_status(AppIndicator.IndicatorStatus.ACTIVE)
        self.indicator.set_title("openRTLinux")
        self.menu = Gtk.Menu()
        self.status_item = Gtk.MenuItem(label="…")
        self.status_item.set_sensitive(False)
        self.menu.append(self.status_item)
        self.menu.append(Gtk.SeparatorMenuItem())

        self.mode_items = {}
        group = None
        for key in MODES:
            item = Gtk.RadioMenuItem(label=mode_label(key))
            if group is None:
                group = item
            else:
                item.join_group(group)
            item.connect("activate", self.on_mode, key)
            self.menu.append(item)
            self.mode_items[key] = item
        self.menu.append(Gtk.SeparatorMenuItem())

        self.dry_item = Gtk.CheckMenuItem(label=t("tray_dry"))
        self.dry_item.connect("activate", self.on_dry_run)
        self.menu.append(self.dry_item)

        self.start_item = Gtk.MenuItem(label=t("tray_start"))
        self.start_item.connect("activate", self.on_start)
        self.menu.append(self.start_item)

        self.settings_item = Gtk.MenuItem(label=t("tray_settings"))
        settings_item = self.settings_item
        settings_item.connect("activate", self.on_settings)
        self.menu.append(settings_item)

        self.log_item = Gtk.MenuItem(label=t("tray_log"))
        log_item = self.log_item
        log_item.connect("activate", self.on_log)
        self.menu.append(log_item)
        self.menu.append(Gtk.SeparatorMenuItem())

        self.quit_item = Gtk.MenuItem(label=t("tray_quit"))
        quit_item = self.quit_item
        quit_item.connect("activate", self.on_quit_daemon)
        self.menu.append(quit_item)
        self.exit_item = Gtk.MenuItem(label=t("tray_exit"))
        exit_item = self.exit_item
        exit_item.connect("activate", lambda *_: Gtk.main_quit())
        self.menu.append(exit_item)

        self.menu.show_all()
        self.indicator.set_menu(self.menu)
        self.window = None
        self.refresh()
        GLib.timeout_add(2000, self.refresh)

    def retranslate(self) -> None:
        """Say the menu again in the chosen language."""
        self.indicator.set_title("openRTLinux")
        self.dry_item.set_label(t("tray_dry"))
        self.start_item.set_label(t("tray_start"))
        self.settings_item.set_label(t("tray_settings"))
        self.log_item.set_label(t("tray_log"))
        self.quit_item.set_label(t("tray_quit"))
        self.exit_item.set_label(t("tray_exit"))

    def refresh(self) -> bool:
        if consume_window_request():
            print("window requested from the menu - opening the settings window", flush=True)
            self.on_settings()
        status = ask("status")
        running = "error" not in status
        self.status_item.set_label(describe(status))
        self.start_item.set_sensitive(not running)
        for key, item in self.mode_items.items():
            item.set_sensitive(running)
            item.handler_block_by_func(self.on_mode)
            item.set_active(status.get("mode") == key)
            item.handler_unblock_by_func(self.on_mode)
        self.dry_item.set_sensitive(running)
        self.dry_item.handler_block_by_func(self.on_dry_run)
        self.dry_item.set_active(bool(status.get("dry_run", True)))
        self.dry_item.handler_unblock_by_func(self.on_dry_run)
        off = status.get("mode") == "off" or not running
        self.indicator.set_icon_full(ICON_OFF if off else ICON_ON, "openRTLinux")
        return True

    def on_mode(self, item, key: str) -> None:
        if not item.get_active():
            return
        ask(f"set mode {key}")
        write_config({"mode": key})
        self.refresh()

    def on_dry_run(self, item) -> None:
        on = bool(item.get_active())
        ask(f"set dry_run {str(on).lower()}")
        write_config({"dry_run": on})
        self.refresh()
        if not on:
            # say it out loud once: this is the switch that makes it act
            subprocess.Popen(
                ["sh", "-c", "command -v notify-send >/dev/null && "
                 "notify-send -a openRTLinux 'openRTLinux' " + shlex_quote(t("notify"))],
                start_new_session=True,
            )

    def on_start(self, *_args) -> None:
        message = start_daemon()
        self.status_item.set_label(f"{message}…")
        GLib.timeout_add(1500, self.refresh)

    def on_settings(self, *_args) -> None:
        if self.window is None:
            self.window = SettingsWindow(tray=self)
            self.window.connect("delete-event", self._hide)
        self.window.show_all()
        self.window.present()

    def _hide(self, *_args) -> bool:
        self.window.hide()
        return True

    def on_log(self, *_args) -> None:
        subprocess.Popen(
            ["sh", "-c", "journalctl --user -n 60 --no-pager | tail -40; echo; read -p 'Enter to close' _"],
            start_new_session=True,
        )

    def on_quit_daemon(self, *_args) -> None:
        ask("quit")
        GLib.timeout_add(500, self.refresh)


# ----------------------------------------------------------------------- main
def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--window", action="store_true", help="open the window at once")
    parser.add_argument("--no-tray", action="store_true", help="window only, no tray icon")
    parser.add_argument("--report", action="store_true", help="print status and exit")
    parser.add_argument("--selftest", action="store_true", help="build the widgets, then exit")
    parser.add_argument("--lang", choices=["system", "th", "en"], help="UI language for this run")
    args = parser.parse_args()

    # the language has to be known before any label is built
    global LANG, LANG_OVERRIDE
    LANG_OVERRIDE = args.lang
    stored = toml_value(CONFIG_PATH, "ui_language", "system")
    LANG = resolve_language(args.lang or stored)

    if args.report:
        status = ask("status")
        print(json.dumps(status, ensure_ascii=False, indent=2))
        return 0 if "error" not in status else 1

    if args.selftest:
        window = SettingsWindow()
        window.show_all()
        tray = None
        if HAVE_TRAY and not args.no_tray:
            tray = Tray()
        print("widgets built OK")
        # anything left out of the grid on purpose: it has no parent, so it can
        # never reach the screen even after `window.show_all()`
        hidden = [
            key
            for key in ("switch_key", "current_command")
            if window.labels[key].get_parent() is None
        ]
        print(f"  hidden rows: {', '.join(hidden) if hidden else 'none'}")
        words = window.read_words()
        print(
            f"  my words: {len(words['words'])} kept \u00b7 {len(words['pairs'])} pair(s) "
            f"\u00b7 learn {words['learn']} \u00b7 {WORDS_PATH}"
        )
        print(f"  about: {window.version_label.get_text()} \u00b7 {REPO_URL} \u00b7 pause {PAUSE_CHORD}")
        print(f"  language: {LANG} ({'override' if args.lang else 'from config/environment'})")
        print(f"  sample: {t('window_title')} | {t('save')} | {mode_label('manual')}")
        print(f"  gate: GTK3 yes · tray {'yes' if tray else 'no'} · config {CONFIG_PATH}")
        print(f"  socket: {SOCKET_PATH} · {describe(ask('status'))}")
        return 0

    if not HAVE_TRAY or args.no_tray:
        window = SettingsWindow()
        window.connect("destroy", Gtk.main_quit)
        window.show_all()
    else:
        holder = claim_single_instance()
        if holder is None:
            if args.window:
                request_window()
                print("openRTLinux tray is already running - asked it to show the settings window")
                return 0
            print("openRTLinux tray is already running - not starting a second one")
            return 0
        tray = Tray()
        if args.window:
            tray.on_settings()
    Gtk.main()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
