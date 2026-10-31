//! GENERATED - physical key table for Linux (evdev code <-> US canonical char).
//!
//! Built from /usr/share/X11/xkb/symbols/us + /usr/include/linux/input-event-codes.h,
//! on top of the same table the thailang project validated against RightLang's own
//! shift.txt (43/43 keys agreed). Regenerate rather than hand-edit.

/// (evdev key code, unshifted US char, shifted US char)
pub const KEYS: &[(u16, char, char)] = &[
    (2, '1', '!'),
    (3, '2', '@'),
    (4, '3', '#'),
    (5, '4', '$'),
    (6, '5', '%'),
    (7, '6', '^'),
    (8, '7', '&'),
    (9, '8', '*'),
    (10, '9', '('),
    (11, '0', ')'),
    (12, '-', '_'),
    (13, '=', '+'),
    (16, 'q', 'Q'),
    (17, 'w', 'W'),
    (18, 'e', 'E'),
    (19, 'r', 'R'),
    (20, 't', 'T'),
    (21, 'y', 'Y'),
    (22, 'u', 'U'),
    (23, 'i', 'I'),
    (24, 'o', 'O'),
    (25, 'p', 'P'),
    (26, '[', '{'),
    (27, ']', '}'),
    (30, 'a', 'A'),
    (31, 's', 'S'),
    (32, 'd', 'D'),
    (33, 'f', 'F'),
    (34, 'g', 'G'),
    (35, 'h', 'H'),
    (36, 'j', 'J'),
    (37, 'k', 'K'),
    (38, 'l', 'L'),
    (39, ';', ':'),
    (40, '\'', '"'),
    (41, '`', '~'),
    (43, '\\', '|'),
    (44, 'z', 'Z'),
    (45, 'x', 'X'),
    (46, 'c', 'C'),
    (47, 'v', 'V'),
    (48, 'b', 'B'),
    (49, 'n', 'N'),
    (50, 'm', 'M'),
    (51, ',', '<'),
    (52, '.', '>'),
    (53, '/', '?'),
];

/// What this key produces on a US keyboard.
pub fn us_char(code: u16, shift: bool) -> Option<char> {
    KEYS.iter()
        .find(|(c, _, _)| *c == code)
        .map(|(_, plain, shifted)| if shift { *shifted } else { *plain })
}

/// The key (and whether Shift is needed) that types this US char.
pub fn key_for(us: char) -> Option<(u16, bool)> {
    for (code, plain, shifted) in KEYS {
        if *plain == us {
            return Some((*code, false));
        }
        if *shifted == us {
            return Some((*code, true));
        }
    }
    None
}

/// Every key code we can drive, for the virtual keyboard declaration.
pub fn codes() -> Vec<u16> {
    KEYS.iter().map(|(c, _, _)| *c).collect()
}
