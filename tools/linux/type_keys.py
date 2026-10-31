#!/usr/bin/env python3
"""Type a physical key sequence on the X13 through uinput, for testing openrtlinux.

    python3 tools/linux/type_keys.py "fvdw,h "     # type those physical keys
    python3 tools/linux/type_keys.py switch        # press the layout switch key (grave)

Needs read/write access to /dev/uinput (uaccess ACL or the `input` group). It is
a *test* device on purpose: openrtlinux filters only its own "openRTLinux virtual
keyboard" by name, so what this types looks exactly like a person typing.
"""

import sys
import time

from evdev import UInput, ecodes as e

# A virtual device that only carries the keys it sends is not what a keyboard
# looks like; declare the whole board so nothing filters it out.
ALPHABET = [
    "KEY_A", "KEY_B", "KEY_C", "KEY_D", "KEY_E", "KEY_F", "KEY_G", "KEY_H", "KEY_I",
    "KEY_J", "KEY_K", "KEY_L", "KEY_M", "KEY_N", "KEY_O", "KEY_P", "KEY_Q", "KEY_R",
    "KEY_S", "KEY_T", "KEY_U", "KEY_V", "KEY_W", "KEY_X", "KEY_Y", "KEY_Z",
    "KEY_0", "KEY_1", "KEY_2", "KEY_3", "KEY_4", "KEY_5", "KEY_6", "KEY_7", "KEY_8",
    "KEY_9", "KEY_COMMA", "KEY_DOT", "KEY_SEMICOLON", "KEY_APOSTROPHE", "KEY_SLASH",
    "KEY_GRAVE", "KEY_MINUS", "KEY_EQUAL", "KEY_LEFTBRACE", "KEY_RIGHTBRACE",
    "KEY_BACKSLASH", "KEY_SPACE", "KEY_ENTER", "KEY_TAB", "KEY_BACKSPACE",
    # modifiers must be declared too: a uinput device only ever sends the codes
    # it announced, so an undeclared Shift silently never arrives
    "KEY_LEFTSHIFT", "KEY_RIGHTSHIFT", "KEY_LEFTCTRL", "KEY_LEFTALT", "KEY_LEFTMETA",
    "KEY_CAPSLOCK",
]

# every key the tool can type: generated so no character is silently missing
# (a hand-written subset is how "-" and "y" turned into KeyError mid-test)
KEYS = {
    name.split("_", 1)[1].lower(): name
    for name in ALPHABET
    if name.startswith("KEY_") and len(name) == 5  # KEY_A / KEY_0
}
KEYS.update(
    {
        " ": "KEY_SPACE",
        ",": "KEY_COMMA",
        ".": "KEY_DOT",
        ";": "KEY_SEMICOLON",
        "'": "KEY_APOSTROPHE",
        "/": "KEY_SLASH",
        "`": "KEY_GRAVE",
        "-": "KEY_MINUS",
        "=": "KEY_EQUAL",
        "[": "KEY_LEFTBRACE",
        "]": "KEY_RIGHTBRACE",
        "\\": "KEY_BACKSLASH",
        "return": "KEY_ENTER",
        "enter": "KEY_ENTER",
        "space": "KEY_SPACE",
    }
)



def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    what = sys.argv[1]
    board = sorted({getattr(e, name) for name in ALPHABET})
    u = UInput({e.EV_KEY: board}, name="righttype test keys")
    time.sleep(1.2)
    if what == "shift-space":
        # the other chord a person can pick: hold Shift, tap Space
        u.write(e.EV_KEY, e.KEY_LEFTSHIFT, 1)
        u.syn()
        time.sleep(0.15)
        for value in (1, 0):
            u.write(e.EV_KEY, e.KEY_SPACE, value)
            u.syn()
            time.sleep(0.03)
        time.sleep(0.15)
        u.write(e.EV_KEY, e.KEY_LEFTSHIFT, 0)
        u.syn()
    elif what == "backspace-then-shift":
        # the other key order a fast chord can produce: Backspace, then Shift
        for value in (1, 0):
            u.write(e.EV_KEY, e.KEY_BACKSPACE, value)
            u.syn()
            time.sleep(0.03)
        u.write(e.EV_KEY, e.KEY_LEFTSHIFT, 1)
        u.syn()
        time.sleep(0.15)
        u.write(e.EV_KEY, e.KEY_LEFTSHIFT, 0)
        u.syn()
    elif what == "hold-shift-backspace":
        # The manual hotkey as a person does it: Shift goes down, Backspace
        # clicks, and Shift stays down for a moment. Anything the daemon types
        # while it is still down arrives shifted - the bug this reproduces.
        hold = float(sys.argv[2]) if len(sys.argv) > 2 else 1.2
        u.write(e.EV_KEY, e.KEY_LEFTSHIFT, 1)
        u.syn()
        time.sleep(0.1)
        for value in (1, 0):
            u.write(e.EV_KEY, e.KEY_BACKSPACE, value)
            u.syn()
            time.sleep(0.06)
        print(f"holding Shift for {hold}s after Backspace", flush=True)
        time.sleep(hold)
        u.write(e.EV_KEY, e.KEY_LEFTSHIFT, 0)
        u.syn()
    elif what == "switch":
        for value in (1, 0):
            u.write(e.EV_KEY, e.KEY_GRAVE, value)
            u.syn()
            time.sleep(0.06)
    else:
        for ch in what:
            shifted = ch.isupper() or (ch in "~!@#$%^&*()_+{}|:\"<>?")
            base = ch.lower() if ch.isupper() else SHIFTED.get(ch, ch)
            code = getattr(e, KEYS[base])
            for value in (1, 0):
                u.write(e.EV_KEY, code, value)
                u.syn()
                time.sleep(0.05)
    time.sleep(0.4)
    u.close()
    print(f"sent {what!r}")
    return 0


# shifted punctuation -> the unshifted key that carries it
SHIFTED = {
    "~": "`", "!": "1", "@": "2", "#": "3", "$": "4", "%": "5", "^": "6",
    "&": "7", "*": "8", "(": "9", ")": "0", "_": "-", "+": "=", "{": "[",
    "}": "]", "|": "\\", ":": ";", '"': "'", "<": ",", ">": ".", "?": "/",
}

LINGER = 1.6  # the daemon rescans keyboards once a second: stay visible past that


if __name__ == "__main__":
    code = main()
    # stay alive a moment: the daemon rescans keyboards once a second, and a
    # device that comes and goes between two scans is never seen at all.
    time.sleep(LINGER)
    raise SystemExit(code)

