//! The exact pairs Suvasin reported: what does the daemon do with them, and why?
//!   cargo test --features linuxos --test reported_words -- --nocapture

use righttype::dict;
use righttype::layout::{self as core_layout};
use righttype::policy::{self, InputLayout};

fn thai() -> &'static dyn core_layout::Layout {
    core_layout::thai_table(core_layout::thai_variant())
}

fn us() -> &'static dyn core_layout::Layout {
    core_layout::us_table()
}

#[test]
fn reported() {
    println!();
    for s in ["dd", "gg", "กก", "เเ", "fvdw,h", "ก", "d"] {
        let has_thai = s.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c));
        let layout = if has_thai {
            InputLayout::ThaiKedmanee
        } else {
            InputLayout::UsQwerty
        };
        let (from, to) = if has_thai {
            (thai() as &dyn core_layout::Layout, us() as &dyn core_layout::Layout)
        } else {
            (us(), thai())
        };
        let flipped = core_layout::convert(s, from, to);
        let checked = policy::detect_token(s, layout, dict::english(), dict::thai());
        println!(
            "{s:>8} in {:?}: flipped={flipped:?} | english dict has it: {} | thai dict has it: {} | thai dict has the flip: {} | english dict has the flip: {} | word checker: {:?}",
            layout,
            dict::english().contains(s),
            dict::thai().contains(s),
            dict::thai().contains(&flipped),
            dict::english().contains(&flipped),
            checked.map(|d| d.corrected)
        );
    }
    println!();
}
