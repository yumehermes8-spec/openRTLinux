#!/usr/bin/env python3
"""rtlinux tray + settings window.

    rtlinux-tray                 # tray icon, and the settings window on demand
    rtlinux-tray --window        # open the settings window straight away
    rtlinux-tray --report        # print the daemon's status and exit
    rtlinux-tray --selftest      # build every widget, print OK, exit (for CI/SSH)

Why Python and not Rust: the port's own layer is Rust, but GTK *bindings* need
`libgtk-3-dev` + pkg-config, which this box cannot install without a password.
python3-gi, GTK3 and libayatana-appindicator are already here, and the daemon is
controlled over a socket, so the language of the window makes no difference.

Talks to the daemon on `/run/user/$UID/rtlinux.sock`; writes
`~/.config/rtlinux/config.toml` and asks the daemon to reload it.
"""

from __future__ import annotations

import argparse
import json
import os
import socket
import subprocess
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

APP = "rtlinux"
ICON_ON = "rtlinux"
ICON_OFF = "rtlinux-off"
# the chords a person can pick for "fix the word under the cursor"
HOTKEYS = [
    ("shift+backspace", "Shift+Backspace"),
    ("shift+spacebar", "Shift+Spacebar"),
]


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
MODE_LABELS = {"auto": "แก้เองเมื่อเคาะวรรค (auto)",
               "manual": "แก้เมื่อกด hotkey (manual)",
               "off": "ปิดการแก้ (off)"}


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
            ["systemctl", "--user", "start", "rtlinux.service"],
            capture_output=True,
            text=True,
            timeout=15,
        )
        if done.returncode == 0:
            return "started (systemd)"
    except Exception:  # noqa: BLE001 - fall through to plain spawn
        pass
    exe = shutil_which("rtlinux") or str(Path.home() / ".local/bin" / "rtlinux")
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
        return "daemon: ไม่ได้ทำงาน"
    mode = MODE_LABELS.get(status.get("mode", "?"), status.get("mode", "?"))
    switch = "⚠ ทดลอง — ไม่พิมพ์ (ปิดสวิตช์ 'โหมดทดลอง' เพื่อให้แก้จริง)" if status.get("dry_run") else "แก้จริง"
    layout = "ไทย" if status.get("layout") == "thai" else "อังกฤษ"
    return (
        f"{mode} · {switch} · แป้น: {layout} · "
        f"แก้ไป {status.get('committed', 0)} ครั้ง"
    )


# ------------------------------------------------------------------ settings
class SettingsWindow(Gtk.Window):
    def __init__(self, tray=None) -> None:
        super().__init__(title="RightType (Linux) — ตั้งค่า")
        self.tray = tray
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

        self.mode = Gtk.ComboBoxText()
        for key in ("auto", "manual", "off"):
            self.mode.append(key, MODE_LABELS[key])
        grid.attach(label_start("โหมด"), 0, row, 1, 1)
        grid.attach(self.mode, 1, row, 1, 1)
        row += 1

        self.dry_run = Gtk.CheckButton(
            label="โหมดทดลอง — ตัดสินใจและบันทึก แต่ไม่พิมพ์/ไม่สลับแป้น"
        )
        grid.attach(self.dry_run, 1, row, 1, 1)
        row += 1

        self.hotkey = Gtk.ComboBoxText()
        for key, label in HOTKEYS:
            self.hotkey.append(key, label)
        self.hotkey.set_tooltip_text(
            "คอร์ดที่กดแล้วแก้คำใต้เคอร์เซอร์ — Shift+Spacebar จะลบวรรคที่เกิดขึ้นให้ด้วย"
        )
        grid.attach(label_start("hotkey แก้คำ"), 0, row, 1, 1)
        grid.attach(self.hotkey, 1, row, 1, 1)
        row += 1

        self.switch_key = Gtk.Entry()
        grid.attach(label_start("ปุ่มสลับแป้นของเดสก์ท็อป"), 0, row, 1, 1)
        grid.attach(self.switch_key, 1, row, 1, 1)
        row += 1

        self.current_command = Gtk.Entry()
        grid.attach(label_start("คำสั่งอ่านแป้นปัจจุบัน"), 0, row, 1, 1)
        grid.attach(self.current_command, 1, row, 1, 1)
        row += 1

        self.autostart = Gtk.CheckButton(label="เปิด tray อัตโนมัติเมื่อ login")
        grid.attach(self.autostart, 1, row, 1, 1)
        row += 1

        hint = Gtk.Label()
        hint.set_xalign(0)
        hint.set_halign(Gtk.Align.START)
        hint.set_line_wrap(True)
        hint.set_markup(
            "<small>หมายเหตุ: พอร์ตนี้ยังทำได้แค่แก้คำเมื่อเคาะวรรค (auto) และกด "
            "Shift+Backspace (manual) — ฟีเจอร์ที่ต้องอ่านข้อความจากแอป "
            "(ghost suggestion, per-app) ยังไม่ได้ทำบน Wayland</small>"
        )
        box.pack_start(hint, False, False, 0)

        buttons = Gtk.Box(spacing=8)
        box.pack_end(buttons, False, False, 0)
        save = Gtk.Button(label="บันทึกแล้วโหลดใหม่")
        save.connect("clicked", self.on_save)
        buttons.pack_end(save, False, False, 0)
        refresh = Gtk.Button(label="รีเฟรชสถานะ")
        refresh.connect("clicked", lambda *_: self.reload_status())
        buttons.pack_end(refresh, False, False, 0)

        self.load_config()
        self.reload_status()
        GLib.timeout_add(1500, self._tick)

    # ---- config file
    @staticmethod
    def defaults(self=None) -> dict:
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

    def load_config(self) -> None:
        data = dict(self.defaults())
        try:
            import tomllib  # python 3.11+

            data.update(tomllib.loads(CONFIG_PATH.read_text()))
        except Exception:  # noqa: BLE001 - missing file or old python
            pass
        self.config = data
        self.mode.set_active_id(str(data.get("mode", "auto")))
        self.dry_run.set_active(bool(data.get("dry_run", True)))
        self.hotkey.set_active_id(normalise_hotkey(data.get("fix_word_hotkey", "shift+backspace")))
        self.switch_key.set_text(str(data.get("switch_command", "")))
        self.current_command.set_text(str(data.get("current_command", "")))
        self.autostart.set_active(bool(data.get("tray_autostart", False)))

    def on_save(self, *_args) -> None:
        self.config["mode"] = self.mode.get_active_id() or "auto"
        self.config["dry_run"] = self.dry_run.get_active()
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
            "บันทึกแล้ว — " + json.dumps(result, ensure_ascii=False)
        )
        if self.tray is not None:
            self.tray.refresh()

    def reload_status(self) -> bool:
        status = ask("status")
        suffix = f"  [{status['error']}]" if "error" in status else ""
        self.status.set_text(describe(status) + suffix)
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
    target = Path.home() / ".config" / "autostart" / "rtlinux-tray.desktop"
    if not enabled:
        if target.exists():
            target.unlink()
        return "autostart off"
    target.parent.mkdir(parents=True, exist_ok=True)
    source = Path(__file__).resolve().parent / "rtlinux-tray.desktop"
    if source.exists():
        target.write_text(source.read_text())
    else:
        target.write_text(
            "[Desktop Entry]\nType=Application\nName=RightType tray\n"
            f"Exec={sys.executable} {Path(__file__).resolve()}\n"
            "Icon=rtlinux\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
        )
    return f"autostart on ({target})"


# ----------------------------------------------------------------------- tray
class Tray:
    def __init__(self) -> None:
        self.indicator = AppIndicator.Indicator.new(
            APP, ICON_ON, AppIndicator.IndicatorCategory.APPLICATION_STATUS
        )
        self.indicator.set_status(AppIndicator.IndicatorStatus.ACTIVE)
        self.indicator.set_title("RightType")
        self.menu = Gtk.Menu()
        self.status_item = Gtk.MenuItem(label="…")
        self.status_item.set_sensitive(False)
        self.menu.append(self.status_item)
        self.menu.append(Gtk.SeparatorMenuItem())

        self.mode_items = {}
        group = None
        for key in ("auto", "manual", "off"):
            item = Gtk.RadioMenuItem(label=MODE_LABELS[key])
            if group is None:
                group = item
            else:
                item.join_group(group)
            item.connect("activate", self.on_mode, key)
            self.menu.append(item)
            self.mode_items[key] = item
        self.menu.append(Gtk.SeparatorMenuItem())

        self.dry_item = Gtk.CheckMenuItem(label="โหมดทดลอง (ไม่พิมพ์จริง)")
        self.dry_item.connect("activate", self.on_dry_run)
        self.menu.append(self.dry_item)

        self.start_item = Gtk.MenuItem(label="เริ่ม daemon")
        self.start_item.connect("activate", self.on_start)
        self.menu.append(self.start_item)

        settings_item = Gtk.MenuItem(label="ตั้งค่า…")
        settings_item.connect("activate", self.on_settings)
        self.menu.append(settings_item)

        log_item = Gtk.MenuItem(label="ดู log (journal)")
        log_item.connect("activate", self.on_log)
        self.menu.append(log_item)
        self.menu.append(Gtk.SeparatorMenuItem())

        quit_item = Gtk.MenuItem(label="หยุด daemon")
        quit_item.connect("activate", self.on_quit_daemon)
        self.menu.append(quit_item)
        exit_item = Gtk.MenuItem(label="ปิด tray นี้")
        exit_item.connect("activate", lambda *_: Gtk.main_quit())
        self.menu.append(exit_item)

        self.menu.show_all()
        self.indicator.set_menu(self.menu)
        self.window = None
        self.refresh()
        GLib.timeout_add(2000, self.refresh)

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
        self.indicator.set_icon_full(ICON_OFF if off else ICON_ON, "RightType")
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
                 "notify-send -a RightType 'RightType' 'เริ่มแก้จริงแล้ว — จะพิมพ์/สลับแป้นให้'"],
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
    args = parser.parse_args()

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
        print(f"  gate: GTK3 yes · tray {'yes' if tray else 'no'} · config {CONFIG_PATH}")
        print(f"  socket: {SOCKET_PATH} · {describe(ask('status'))}")
        return 0

    if not HAVE_TRAY or args.no_tray:
        window = SettingsWindow()
        window.connect("destroy", Gtk.main_quit)
        window.show_all()
    else:
        tray = Tray()
        if args.window:
            tray.on_settings()
    Gtk.main()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
