#!/usr/bin/env python3
"""Talk to the openrtlinux daemon from the shell.

    python3 tools/linux/rtctl.py status
    python3 tools/linux/rtctl.py set mode manual
    python3 tools/linux/rtctl.py set dry_run false
    python3 tools/linux/rtctl.py reload
    python3 tools/linux/rtctl.py quit

The tray uses the same socket, so this doubles as the way to check what the GUI
would see - useful over SSH, where the tray itself is not visible.
"""

import json
import os
import socket
import sys
from pathlib import Path

SOCKET = Path(os.environ.get("XDG_RUNTIME_DIR", "/tmp")) / "openrtlinux.sock"


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    command = " ".join(sys.argv[1:])
    if not SOCKET.exists():
        print(f"no daemon: {SOCKET} does not exist")
        return 1
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
            sock.settimeout(3)
            sock.connect(str(SOCKET))
            sock.sendall((command + "\n").encode())
            reply = sock.recv(65536).decode("utf-8", "replace").strip()
    except Exception as exc:  # noqa: BLE001
        print(f"cannot reach the daemon: {exc}")
        return 1
    try:
        print(json.dumps(json.loads(reply), ensure_ascii=False, indent=2))
    except json.JSONDecodeError:
        print(reply)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
