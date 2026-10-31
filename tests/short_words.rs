//! Deciding rule for the fix hotkey: how far can it go before it starts
//! "fixing" text that was already right?
//!
//!   cargo test --features linuxos --test short_words -- --nocapture
//!
//! Columns:
//!   rescue   - real words put through the wrong layout that get fixed
//!   damage   - real words typed *correctly* that would get flipped anyway

use righttype::dict::{self, Dictionary};
use righttype::layout::{self as core_layout};
use righttype::policy::{self, InputLayout};

fn thai() -> &'static dyn core_layout::Layout {
    core_layout::thai_table(core_layout::thai_variant())
}

fn us() -> &'static dyn core_layout::Layout {
    core_layout::us_table()
}

fn tables(layout: InputLayout) -> &'static dyn core_layout::Layout {
    match layout {
        InputLayout::ThaiKedmanee => thai(),
        InputLayout::UsQwerty => us(),
    }
}

fn other(layout: InputLayout) -> InputLayout {
    match layout {
        InputLayout::ThaiKedmanee => InputLayout::UsQwerty,
        InputLayout::UsQwerty => InputLayout::ThaiKedmanee,
    }
}

fn layout_of(text: &str) -> InputLayout {
    if text.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c)) {
        InputLayout::ThaiKedmanee
    } else {
        InputLayout::UsQwerty
    }
}

fn dicts(layout: InputLayout) -> (&'static Dictionary, &'static Dictionary) {
    match layout {
        InputLayout::ThaiKedmanee => (dict::thai(), dict::english()),
        InputLayout::UsQwerty => (dict::english(), dict::thai()),
    }
}

/// The current rule: the other reading must be a word *and* what is on screen
/// must not be.
fn careful(typed: &str) -> Option<String> {
    let from = layout_of(typed);
    let (here, there) = dicts(from);
    let reading = core_layout::convert(typed, tables(from), tables(other(from)));
    if reading == typed || !there.contains(reading.trim()) || here.contains(typed.trim()) {
        return None;
    }
    Some(reading)
}

/// The deliberate-press rule: the typist asked, so only the *result* has to make
/// sense - the other layout's reading must be a word.
fn willing(typed: &str) -> Option<String> {
    let from = layout_of(typed);
    let (_, there) = dicts(from);
    let reading = core_layout::convert(typed, tables(from), tables(other(from)));
    if reading == typed || !there.contains(reading.trim()) {
        return None;
    }
    Some(reading)
}

fn daemon(typed: &str) -> Option<String> {
    policy::detect_token(typed, layout_of(typed), dict::english(), dict::thai()).map(|d| d.corrected)
}

fn words_of(len: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in dict::thai_words().filter(|w| w.chars().count() == len) {
        out.push(w.to_string());
    }
    for w in dict::english_words()
        .filter(|w| w.chars().count() == len && w.chars().all(|c| c.is_ascii_lowercase()))
    {
        out.push(w.to_string());
    }
    out
}

fn run(rule: fn(&str) -> Option<String>) -> (usize, usize, usize, usize) {
    let (mut rescue, mut total, mut damage, mut given) = (0, 0, 0, 0);
    for len in 2..=7usize {
        for word in words_of(len) {
            let from = layout_of(&word);
            let typed = core_layout::convert(&word, tables(from), tables(other(from)));
            if typed == word {
                continue;
            }
            total += 1;
            if rule(&typed).as_deref() == Some(word.as_str()) {
                rescue += 1;
            }
            // the same word, but typed correctly: flipping it would be damage
            given += 1;
            if rule(&word).is_some() {
                damage += 1;
            }
        }
    }
    (rescue, total, damage, given)
}

#[test]
fn report() {
    println!();
    for (name, rule) in [
        ("daemon (today)", daemon as fn(&str) -> Option<String>),
        ("careful", careful),
        ("willing", willing),
    ] {
        let (rescue, total, damage, given) = run(rule);
        println!(
            "{name:16} rescue {rescue:>6}/{total:<6} ({:>5.1}%)   damage {damage:>5}/{given:<6} ({:.1}%)",
            100.0 * rescue as f64 / total as f64,
            100.0 * damage as f64 / given as f64
        );
    }
    println!();
    println!("per length, daemon vs willing (rescue %  / damage %):");
    for len in 2..=7usize {
        let words = words_of(len);
        let mut row = String::new();
        for (name, rule) in [("daemon", daemon as fn(&str) -> Option<String>), ("willing", willing)] {
            let (mut r, mut t, mut d, mut g) = (0, 0, 0, 0);
            for word in &words {
                let from = layout_of(word);
                let typed = core_layout::convert(word, tables(from), tables(other(from)));
                if typed == *word {
                    continue;
                }
                t += 1;
                if rule(&typed).as_deref() == Some(word.as_str()) {
                    r += 1;
                }
                g += 1;
                if rule(word).is_some() {
                    d += 1;
                }
            }
            row.push_str(&format!(
                " {name}: {:>5.1}% / {:>4.1}% ",
                100.0 * r as f64 / t as f64,
                100.0 * d as f64 / g as f64
            ));
        }
        println!("  len {len} |{row}");
    }
    println!();
    println!("examples the deliberate-request reading rescues (daemon refused):");
    for len in 2..=4usize {
        let mut shown = 0;
        for word in dict::thai_words().filter(|w| w.chars().count() == len) {
            let typed = core_layout::convert(word, thai(), us());
            if typed == word || typed.chars().count() != len {
                continue;
            }
            if daemon(&typed).is_none() && careful(&typed).as_deref() == Some(word) {
                println!("  len {len}: {typed} -> {word}");
                shown += 1;
                if shown == 4 {
                    break;
                }
            }
        }
        if shown == 0 {
            println!("  len {len}: (none found)");
        }
    }
    println!();
}
