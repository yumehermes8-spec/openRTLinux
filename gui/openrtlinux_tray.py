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
        self.set_default_size(560, 380)
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

        self.switch_key = Gtk.Entry()
        self.labels["switch_key"] = label_start(t("switch_key"))
        grid.attach(self.labels["switch_key"], 0, row, 1, 1)
        grid.attach(self.switch_key, 1, row, 1, 1)
        row += 1

        self.current_command = Gtk.Entry()
        self.labels["current_command"] = label_start(t("current_command"))
        grid.attach(self.labels["current_command"], 0, row, 1, 1)
        grid.attach(self.current_command, 1, row, 1, 1)
        row += 1

        self.autostart = Gtk.CheckButton(label=t("autostart"))
        grid.attach(self.autostart, 1, row, 1, 1)
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
        self.render_history(status)
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
            print("openRTLinux tray is already running - not starting a second one")
            return 0
        tray = Tray()
        if args.window:
            tray.on_settings()
    Gtk.main()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
