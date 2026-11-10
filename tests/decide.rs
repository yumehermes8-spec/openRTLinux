//! What does the daemon actually decide for a given string?
//!
//!   cargo test --features linuxos --test decide -- --nocapture
//!
//! Prints, for each input: the other layout's reading of it, what the word checker
//! says, and the plan `Live` would carry out if the typist pressed the fix hotkey.

use righttype::dict;
use righttype::layout::{self as core_layout};
use righttype::linuxos::config::Mode;
use righttype::linuxos::live::{self, Live};
use righttype::policy::{self, InputLayout};

fn table(layout: InputLayout) -> &'static dyn core_layout::Layout {
    match layout {
        InputLayout::ThaiKedmanee => core_layout::thai_table(core_layout::thai_variant()),
        InputLayout::UsQwerty => core_layout::us_table(),
    }
}

fn ask(typed: &str) {
    let layout = live::layout_of(typed);
    let other = match layout {
        InputLayout::ThaiKedmanee => InputLayout::UsQwerty,
        InputLayout::UsQwerty => InputLayout::ThaiKedmanee,
    };
    let reading = core_layout::convert(typed, table(layout), table(other));
    let checked = policy::detect_token(typed, layout, dict::english(), dict::thai());

    let mut live_state = Live::new(layout);
    live_state.mode = Mode::Manual;
    for c in typed.chars() {
        live_state.on_char(c);
    }
    let decision = live_state.on_fix_hotkey(1);

    println!("typed      : {typed:?}  (layout {layout:?}, {} char)", typed.chars().count());
    println!("other side : {reading:?}");
    println!(
        "word check : {}",
        match &checked {
            Some(d) => format!("Some({:?})", d.corrected),
            None => "None (it refuses)".to_string(),
        }
    );
    match decision {
        Some(c) => println!("the daemon : {:?} -> {:?}", c.shown, c.corrected),
        None => println!("the daemon : nothing (press does nothing)"),
    }
    println!();
}

#[test]
fn decision_table() {
    println!();
    for s in [
        "กพนไหหฟยๅฝขภ",   // what Suvasin just sent
        "drows...",        // a guess at what was meant
        "drowssap",        // ...
    ] {
        ask(s);
    }
}
