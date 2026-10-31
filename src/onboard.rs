//! First-run welcome + anytime hotkeys help — Windows only.
//!
//! One small window, and the one place RightType asks for attention: what it
//! does (with a live example), the hotkeys, the current mode, and a single
//! button. Shown automatically on first launch (config `onboarded`) and
//! reopenable from the tray menu, where it lists every hotkey.

use std::cell::RefCell;
use std::rc::Rc;

use native_windows_gui as nwg;
use righttype::i18n::{tr, trf, T};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::HDC;

use crate::icons;
use crate::settings::hotkey_rows;
use crate::ui::{self, card, divider, field, pal, rect, Gfx, Surface, TextStyle};
use crate::{config, hook};

const W: i32 = 560;
const X: i32 = 32;
const CW: i32 = W - 2 * X;
const ROW: i32 = 46;

struct Welcome {
    window: nwg::Window,
    surface: Rc<Surface>,
    handler: RefCell<Option<nwg::EventHandler>>,
    /// First run only: polls the try-it box so the result can be shown live.
    #[allow(deprecated)]
    timer: RefCell<Option<nwg::Timer>>,
}

/// Vertical layout shared by the controls and the background painter.
struct Layout {
    example_y: Option<i32>,
    keys_y: i32,
    rows: usize,
    height: i32,
}

fn layout(first_run: bool) -> Layout {
    if first_run {
        // Language switch row, headline, sub, keyboards line, then the example.
        let example_y = 190;
        // Room for the practice box and its live result under the example.
        let keys_y = example_y + 72 + 70 + 44 + 28;
        let rows = 4;
        Layout {
            example_y: Some(example_y),
            keys_y,
            rows,
            height: keys_y + rows as i32 * ROW + 8 + 112 + 24,
        }
    } else {
        let keys_y = 84;
        let rows = hotkey_rows().len();
        Layout {
            example_y: None,
            keys_y,
            rows,
            height: keys_y + rows as i32 * ROW + 8 + 112,
        }
    }
}

/// Show the welcome/help window. `first_run` picks the introduction; closing
/// it then records that onboarding is done.
pub fn show(first_run: bool) {
    let started = std::time::Instant::now();
    if first_run && hook::mode() != hook::Mode::Auto {
        // A fresh install starts in Auto, so the try-it box below fixes the
        // word at once. Settings and Ctrl+CapsLock change it anytime; the
        // choice is saved when the welcome window closes.
        hook::set_mode(hook::Mode::Auto);
    }
    ui::refresh();
    let lay = layout(first_run);
    let mut window = nwg::Window::default();
    let _ = nwg::Window::builder()
        .flags(nwg::WindowFlags::WINDOW)
        .size((W, lay.height))
        .title(tr(if first_run {
            T::WelcomeTitle
        } else {
            T::HelpTitle
        }))
        .topmost(true)
        .build(&mut window);

    let paint_layout = layout(first_run);
    let surface = Surface::attach(
        &window,
        0x5254_0011,
        Box::new(move |g, hdc, rc, _| paint(g, hdc, rc, &paint_layout)),
    );
    let s = &surface;
    let p = pal();

    let mut lang_en = None;
    let mut lang_th = None;
    let mut try_edit = None;
    let mut try_result = None;
    if let Some(example_y) = lay.example_y {
        // Interface language: preselected from Windows, one tap to change.
        let th_on = righttype::i18n::lang() == righttype::i18n::Lang::Th;
        lang_en = Some(s.button_icon("English", '\0', !th_on, (X + CW - 164, 12, 80, 32), p.bg, 0));
        lang_th = Some(s.button_icon("ไทย", '\0', th_on, (X + CW - 80, 12, 80, 32), p.bg, 0));
        s.label(
            tr(T::WelcomeHeadline),
            TextStyle::Title,
            (X, 52, CW, 44),
            p.bg,
            0,
        );
        s.label(tr(T::WelcomeSub), TextStyle::Dim, (X, 104, CW, 44), p.bg, 0);
        let (thai, english) = crate::health::keyboard_counts();
        let kb = format!(
            "{} {thai} · {} {english}",
            tr(T::HealthThai),
            tr(T::HealthEnglish)
        );
        let kb_line = if thai == 0 {
            tr(T::WelcomeNoThai).to_string()
        } else {
            trf(T::WelcomeKeyboards, &[("kb", &kb)])
        };
        s.label(&kb_line, TextStyle::Dim, (X, 148, CW, 36), p.bg, 0);
        s.label(
            tr(T::WelcomeExample),
            TextStyle::Subtitle,
            (X + 24, example_y + 20, CW - 48, 32),
            p.surface,
            0,
        );
        // A box to try it in, right here: RightType works in its own
        // window like in any other (Auto fixes the word at the space; in
        // Manual, Shift+Backspace does).
        s.label(
            tr(if hook::mode() == hook::Mode::Auto {
                T::WelcomeTry
            } else {
                T::WelcomeTryManual
            }),
            TextStyle::Dim,
            (X, example_y + 84, CW, 20),
            p.bg,
            0,
        );
        try_edit = Some(s.edit("", (X, example_y + 106, CW, 34), 0));
        // Filled in by the timer below once the word has (or has not) changed.
        try_result = Some(s.label("", TextStyle::Dim, (X, example_y + 146, CW, 36), p.bg, 0));
        s.label(
            tr(T::HeadHotkeys),
            TextStyle::Section(icons::HOTKEYS),
            (X, lay.keys_y - 30, CW, 20),
            p.bg,
            0,
        );
    } else {
        s.label(
            tr(T::HelpHeadline),
            TextStyle::Title,
            (X, 26, CW, 40),
            p.bg,
            0,
        );
    }
    for (i, (action, keys)) in hotkey_rows().into_iter().take(lay.rows).enumerate() {
        let y = lay.keys_y + 4 + i as i32 * ROW;
        s.label(
            &format!("{}\t{keys}", tr(action)),
            TextStyle::Keys,
            (X + 20, y + 8, CW - 40, 30),
            p.surface,
            0,
        );
    }
    let foot_y = lay.keys_y + lay.rows as i32 * ROW + 8 + 20;
    let mode = tr(match hook::mode() {
        hook::Mode::Auto => T::ModeAuto,
        hook::Mode::Suggest => T::ModeSuggest,
        hook::Mode::Manual => T::ModeManual,
        hook::Mode::Code => T::ModeCode,
    });
    // First run: the mode line gets the full width, and the buttons sit below it.
    s.label(
        &trf(T::WelcomeMode, &[("mode", mode)]),
        TextStyle::Dim,
        (X, foot_y, if first_run { CW } else { CW - 170 }, 50),
        p.bg,
        0,
    );
    let tour_btn = first_run.then(|| {
        s.button_icon(
            tr(T::BtnWelcomeTour),
            icons::HOTKEYS,
            true,
            (X + CW - 214, foot_y + 56, 214, 36),
            p.bg,
            0,
        )
    });
    let start = s.button_icon(
        tr(if first_run {
            T::BtnGetStarted
        } else {
            T::BtnClose
        }),
        if first_run {
            icons::CHECK
        } else {
            icons::CLOSE
        },
        // With a tour offered, "Get started" is the quiet choice.
        !first_run,
        if first_run {
            (X + CW - 214 - 12 - 150, foot_y + 56, 150, 36)
        } else {
            (X + CW - 150, foot_y + 4, 150, 36)
        },
        p.bg,
        0,
    );

    ui::size_and_center(surface.hwnd, W, lay.height);
    let win = Rc::new(Welcome {
        window,
        surface,
        handler: RefCell::new(None),
        timer: RefCell::new(None),
    });
    if let Some(h) = win.window.handle.hwnd() {
        crate::ui::present(windows::Win32::Foundation::HWND(h as _), "welcome", started);
    } else {
        win.window.set_visible(true);
    }

    let weak = Rc::downgrade(&win);
    win.surface.on_click(move |id| {
        let Some(win) = weak.upgrade() else { return };
        if id == start {
            finish(&win, first_run, false);
        } else if Some(id) == tour_btn {
            finish(&win, first_run, true);
        } else if Some(id) == lang_en || Some(id) == lang_th {
            let lang = if Some(id) == lang_th {
                righttype::i18n::Lang::Th
            } else {
                righttype::i18n::Lang::En
            };
            if lang != righttype::i18n::lang() {
                config::set_language(Some(lang));
                config::persist();
                // Every string changes: rebuild the window in the new language.
                close(&win);
                show(first_run);
            }
        }
    });

    // First run: watch the try-it box so it can say, live, whether the word
    // was fixed or what to check when it was not.
    let live = try_edit.zip(try_result);
    if live.is_some() {
        #[allow(deprecated)]
        {
            let mut timer = nwg::Timer::default();
            if nwg::Timer::builder()
                .parent(&win.window)
                .interval(300)
                .stopped(false)
                .build(&mut timer)
                .is_ok()
            {
                *win.timer.borrow_mut() = Some(timer);
            }
        }
    }
    let seen = std::cell::RefCell::new((String::new(), 0u32));
    let win_h = win.clone();
    let handler = nwg::full_bind_event_handler(&win.window.handle, move |evt, _data, handle| {
        if matches!(evt, nwg::Event::OnWindowClose) && handle == win_h.window.handle {
            finish(&win_h, first_run, false);
        } else if matches!(evt, nwg::Event::OnTimerTick) {
            let Some((edit, result)) = live else { return };
            let s = &win_h.surface;
            let text = s.text_of(edit);
            let mut seen = seen.borrow_mut();
            seen.1 = if text == seen.0 { seen.1 + 1 } else { 0 };
            seen.0 = text.clone();
            let fixed = text.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c));
            let shown = if fixed {
                tr(T::WelcomeTryOk)
            } else if !text.trim().is_empty() && seen.1 >= 5 {
                // About 1.5 s of nothing happening after typing.
                tr(T::WelcomeTryNot)
            } else {
                ""
            };
            if s.text_of(result) != shown {
                s.set_text(result, shown);
            }
        }
    });
    *win.handler.borrow_mut() = Some(handler);
}

/// Close the window and record that onboarding is done. On a first run,
/// either start the tour, or say where RightType went: it has no window of its
/// own, so without a word it would look like nothing happened.
fn finish(win: &Rc<Welcome>, first_run: bool, tour: bool) {
    if first_run {
        config::mark_onboarded();
        // Save what the welcome window set up (a fresh install starts in Auto).
        config::persist();
    }
    // Without the tour, the window flies into the tray icon and, as it lands,
    // a pill says RightType is running. The window is photographed first, so
    // closing it below cannot interrupt the flight.
    if first_run && !tour {
        if let Some(h) = win.window.handle.hwnd() {
            // The pill is made by the UI thread: the flight runs on its own.
            crate::dock::fly_to_tray(windows::Win32::Foundation::HWND(h as _), || {
                crate::tray::on_ui(crate::tray::UI_WELCOME_DONE)
            });
            close(win);
            return;
        }
    }
    close(win);
    if first_run {
        if tour {
            crate::tour::start(crate::tour::Chapter::ALL[0]);
        } else {
            crate::overlay::show(tr(T::WelcomeInTray));
        }
    }
}

fn close(win: &Rc<Welcome>) {
    win.surface.detach();
    if let Some(h) = win.handler.borrow_mut().take() {
        nwg::unbind_event_handler(&h);
    }
    win.timer.borrow_mut().take();
    win.window.close();
}

fn paint(g: &Gfx, hdc: HDC, _rc: RECT, lay: &Layout) {
    if let Some(example_y) = lay.example_y {
        card(g, rect(X, example_y, CW, 72));
        // Accent edge: this card is the product in one line.
        g.fill_round(rect(X, example_y, 4, 72), ui::px(2) as f32, pal().accent);
        // The box to try it in, framed like the other fields.
        field(g, rect(X - 2, example_y + 104, CW + 4, 38));
    }
    card(g, rect(X, lay.keys_y, CW, lay.rows as i32 * ROW + 8));
    for i in 1..lay.rows as i32 {
        divider(hdc, X + 16, lay.keys_y + 4 + i * ROW - 1, CW - 32);
    }
}
