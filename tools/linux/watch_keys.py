#!/usr/bin/env python3
"""Print every key event the kernel sees, with the device that sent it.

Used to verify what openrtlinux *injects* when there is no focusable app to read
back (no X11 test app is installed on this box and windows cannot be focused
over SSH).

    python3 tools/linux/watch_keys.py 25      # watch for 25 seconds

The Ubuntu python-evdev is older than the Rust crate: no `enumerate()`, no
`set_blocking()`. A selector over the device fds is what works there, and it
keeps one quiet keyboard from blocking the others.
"""

import os
import selectors
import sys
import time

import evdev

NAMES = {v: k for k, v in vars(evdev.ecodes).items() if k.startswith("KEY_")}


def main() -> int:
    seconds = float(sys.argv[1]) if len(sys.argv) > 1 else 20.0
    sel = selectors.DefaultSelector()
    devices: dict = {}
    stat_index: dict = {}
    deadline = time.time() + seconds
    last_scan = 0.0
    while time.time() < deadline:
        if time.time() - last_scan > 1.0:
            last_scan = time.time()
            for path in evdev.list_devices():
                if path in devices:
                    try:
                        same = os.stat(path).st_ino == stat_index.get(path)
                    except OSError:
                        same = False
                    if same:
                        continue
                    # the path now belongs to a *new* device (tools recreate
                    # theirs on every run): drop the stale one and watch this
                    sel.unregister(devices[path])
                    devices.pop(path)
                try:
                    stat_index[path] = os.stat(path).st_ino
                except OSError:
                    pass
                try:
                    dev = evdev.InputDevice(path)
                    name = dev.name or path
                except (OSError, Exception) as exc:  # noqa: BLE001 - report, never die
                    print(f"skip {path}: {exc}", flush=True)
                    continue
                devices[path] = dev
                sel.register(dev, selectors.EVENT_READ, dev)
                print(f"device {path}: {name}", flush=True)
        for key, _ in sel.select(timeout=0.1):
            dev = key.data
            try:
                events = list(dev.read())
            except OSError:
                sel.unregister(dev)
                devices.pop(dev.path, None)
                continue
            for ev in events:
                if ev.type != evdev.ecodes.EV_KEY:
                    continue
                name = NAMES.get(ev.code, f"code{ev.code}")
                stamp = time.strftime("%H:%M:%S.") + f"{(time.time() % 1):.3f}"[2:]
                who = (dev.name or dev.path)[:34]
                print(f"{stamp} {name:20s} {who:32s} {ev.value}", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
