//! User-interface strings in English and Thai.
//!
//! Every visible string goes through [`tr`], so a missing translation is a
//! compile error rather than an English word left in a Thai window. The
//! language is process-wide: the Windows layer picks it from the config or the
//! Windows display language at startup, and windows opened afterwards use it.

use std::sync::atomic::{AtomicU8, Ordering};

/// A supported interface language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En = 0,
    Th = 1,
}

impl Lang {
    /// The config value for this language.
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Th => "th",
        }
    }

    /// Parse a config value; anything else means "follow Windows".
    pub fn from_code(code: &str) -> Option<Lang> {
        match code.trim().to_ascii_lowercase().as_str() {
            "en" => Some(Lang::En),
            "th" => Some(Lang::Th),
            _ => None,
        }
    }

    /// Thai when the Windows display language's primary language is Thai.
    pub fn from_windows_langid(langid: u16) -> Lang {
        const LANG_THAI: u16 = 0x1E;
        if langid & 0x3FF == LANG_THAI {
            Lang::Th
        } else {
            Lang::En
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(Lang::En as u8);

/// The interface language in use.
pub fn lang() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == Lang::Th as u8 {
        Lang::Th
    } else {
        Lang::En
    }
}

/// Switch the interface language for everything drawn from now on.
pub fn set_lang(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

macro_rules! texts {
    ($($key:ident => $en:expr, $th:expr;)*) => {
        /// A translatable string.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum T { $($key),* }

        impl T {
            /// Every key, for tests.
            pub const ALL: &'static [T] = &[$(T::$key),*];

            /// This string in `lang`.
            pub fn get(self, lang: Lang) -> &'static str {
                match (self, lang) {
                    $((T::$key, Lang::En) => $en, (T::$key, Lang::Th) => $th,)*
                }
            }
        }
    };
}

texts! {
    // Tray menu. `&&` shows one ampersand in a menu item.
    TrayEnabled => "Enabled", "เปิดใช้งาน";
    TrayAuto => "Auto mode", "โหมดอัตโนมัติ";
    TrayManual => "Manual mode", "โหมดกดแก้เอง";
    TraySuggest => "Suggest mode", "โหมดแนะนำ";
    TrayLearn => "Learn new words", "เรียนรู้คำใหม่";
    TrayStartup => "Start with Windows", "เปิดพร้อม Windows";
    TrayFix => "Fix text…", "ซ่อมข้อความ…";
    TraySettings => "Settings…", "ตั้งค่า…";
    TrayStats => "Statistics…", "สถิติ…";
    TrayHelp => "Hotkeys && help…", "ปุ่มลัดและวิธีใช้…";
    TrayTour => "Take the tour…", "ดูทัวร์แนะนำ…";
    TrayReport => "Save a problem report…", "บันทึกรายงานปัญหา…";
    TrayQuit => "Quit", "ออกจากโปรแกรม";
    TrayPause => "Pause", "หยุดชั่วคราว";
    TrayPause10 => "For 10 minutes", "10 นาที";
    TrayPause30 => "For 30 minutes", "30 นาที";
    TrayPause60 => "For 1 hour", "1 ชั่วโมง";
    TrayResume => "Resume now", "กลับมาทำงานเดี๋ยวนี้";
    TrayThisApp => "In this app", "ในแอปนี้";
    TrayNoApp => "Type in an app first", "พิมพ์ในแอปที่ต้องการก่อน";
    TrayAppDefault => "Use the default mode", "ใช้โหมดปกติ";
    TrayAppOff => "Off in this app", "ปิดในแอปนี้";

    // Modes and state, as short labels.
    ModeAuto => "Auto", "อัตโนมัติ";
    ModeSuggest => "Suggest", "แนะนำ";
    ModeManual => "Manual", "กดแก้เอง";
    StateOff => "Off", "ปิดอยู่";
    StateHookLost => "Not working — retrying", "หยุดทำงาน — กำลังลองใหม่";
    StatePaused => "Paused — back in {n} min", "หยุดชั่วคราว — กลับมาในอีก {n} นาที";
    ModeOff => "Off", "ปิด";

    // Toasts.
    ToastOn => "RightType is on", "RightType เปิดแล้ว";
    ToastOff => "RightType is off", "RightType ปิดแล้ว";
    ToastModeAuto => "Auto mode", "โหมดอัตโนมัติ";
    ToastModeSuggest => "Suggest mode", "โหมดแนะนำ";
    ToastModeManual => "Manual mode", "โหมดกดแก้เอง";
    ToastLearnOn => "Learning new words", "เปิดการเรียนรู้คำใหม่";
    ToastLearnOff => "Not learning new words", "ปิดการเรียนรู้คำใหม่";
    ToastUndo => "Undone", "ย้อนกลับแล้ว";
    ToastFlippedWords => "Flipped back {n} words", "แก้ย้อน {n} คำแล้ว";
    ToastFlippedOne => "Flipped 1 word", "แก้ 1 คำแล้ว";
    ToastFlippedOneBack => "Flipped 1 word · again to put it back", "แก้ 1 คำแล้ว · กดอีกครั้งเพื่อคืนค่าเดิม";
    ToastFlippedOneMore => "Flipped 1 word · again for the one before", "แก้ 1 คำแล้ว · กดอีกครั้งแก้คำก่อนหน้า";
    ToastNothingToFlip => "Nothing to flip here (the cursor moved, or no more words)", "ไม่มีคำให้แก้ตรงนี้ (เคอร์เซอร์ย้ายแล้ว หรือไม่มีคำก่อนหน้า)";
    ToastPaused => "Paused for {n} minutes", "หยุดชั่วคราว {n} นาที";
    ToastAppMode => "{mode} in {app}", "{mode} ใน {app}";
    ToastHookLost => "RightType lost the keyboard — retrying…", "RightType ตรวจจับคีย์บอร์ดไม่ได้ — กำลังลองใหม่…";
    ToastHookBack => "RightType is working again", "RightType กลับมาทำงานแล้ว";
    ToastSaved => "Saved", "บันทึกแล้ว";
    PaletteUpper => "UPPER CASE", "ตัวพิมพ์ใหญ่ทั้งหมด";
    PaletteLower => "lower case", "ตัวพิมพ์เล็กทั้งหมด";
    PaletteTitle => "Title Case", "ขึ้นต้นคำด้วยตัวใหญ่";
    PaletteSwapCase => "sWAP cASE", "สลับตัวเล็ก/ใหญ่";
    PaletteSwapDigits => "Thai digits ↔ 0–9", "เลขไทย ↔ เลขอารบิก";
    PaletteTrayLanguage => "Tray icon shows TH / EN", "ไอคอนถาดระบบแสดง TH / EN";
    PaletteCapsSwitch => "CapsLock switches Thai/English", "CapsLock สลับภาษาไทย/อังกฤษ";
    ToastCapsSwitchOn => "Tap CapsLock to switch language · hold it for CAPS", "แตะ CapsLock เพื่อสลับภาษา · กดค้างเพื่อพิมพ์ตัวใหญ่";
    ToastCapsSwitchOff => "CapsLock is CapsLock again", "CapsLock กลับมาเป็นปุ่มตัวพิมพ์ใหญ่ตามเดิม";
    TipShiftBackspace => "↶ Shift+Backspace", "↶ Shift+Backspace ย้อนได้";
    PaletteFixField => "Fix this field", "ซ่อมทั้งช่องนี้";
    ErrFieldTooLong => "This field is too long to fix here — use Fix text", "ช่องนี้ยาวเกินไป ใช้หน้าต่างซ่อมข้อความแทน";
    ToastNothingToFix => "Nothing to fix in this field", "ไม่มีคำที่ต้องแก้ในช่องนี้";
    ToastFixedWords => "Fixed {n} words · Ctrl+Z undoes it", "แก้ {n} คำ · Ctrl+Z ย้อนได้";
    PaletteKeepAsTyped => "Never convert “{word}”", "ไม่ต้องแปลง “{word}” อีก";
    PaletteLearnWord => "Add “{word}”", "เพิ่ม “{word}”";
    PaletteLearnRecent => "Keep “{word}”", "เก็บ “{word}” ไว้";
    ToastWordLearned => "Added “{word}”", "เพิ่ม “{word}” แล้ว";
    ToastWordInvalid => "Can't add this word", "เพิ่มคำนี้ไม่ได้";
    ToastCopied => "Copied", "คัดลอกแล้ว";
    ToastCapsOff => "CapsLock was on — word fixed, CapsLock off · Shift+Backspace if you meant capitals", "CapsLock ค้าง — แก้คำและปิด CapsLock แล้ว · ตั้งใจพิมพ์ตัวใหญ่? กด Shift+Backspace";
    ToastCapsKept => "Capitals kept, CapsLock on again", "คืนตัวพิมพ์ใหญ่และเปิด CapsLock ให้แล้ว";
    ToastModeCode => "Code mode: only what is clearly a slip — never names, Thai only in comments and strings", "โหมดโค้ด: แก้เฉพาะที่พิมพ์ผิดแป้นชัด ๆ ไม่แตะชื่อตัวแปร และแปลงเป็นไทยเฉพาะใน comment/string";
    RowSyncSettings => "Sync settings and snippets too", "ซิงก์การตั้งค่าและคำย่อด้วย";
    SubSyncSettingsShort => "Share modes, apps, hotkeys and snippets too.", "ซิงก์โหมด แอป ปุ่มลัด และคำย่อด้วย";
    SubSyncSettings => "Modes, apps, hotkeys and snippets, in the same folder, for every PC that uses it. Off by default.", "โหมด รายการแอป ปุ่มลัด และคำย่อ เก็บในโฟลเดอร์เดียวกัน ใช้ร่วมกันทุกเครื่องที่ใช้โฟลเดอร์นี้ (ปิดไว้เป็นค่าเริ่มต้น)";
    SyncNeedsFolder => "Choose a sync folder first.", "เลือกโฟลเดอร์สำหรับซิงก์ก่อน";
    NavSnippets => "Snippets", "คำย่อ";
    SnippetsIntro => "Type a short trigger, then Space or Enter, to expand into full text. Shift+Backspace undoes it.", "พิมพ์คำย่อแล้วกด Space หรือ Enter เพื่อแปลงเป็นข้อความเต็ม กด Shift+Backspace เพื่อย้อนคืน";
    ColTrigger => "Trigger", "คำย่อ";
    ColText => "Text", "ข้อความ";
    ColKeyboard => "Keyboard", "แป้นพิมพ์";
    ScopeThai => "Thai only", "แป้นไทย";
    ScopeEnglish => "English only", "แป้นอังกฤษ";
    ScopeEither => "Either", "ทั้งสองแป้น";
    BtnSaveSnippet => "Add / save", "เพิ่ม / บันทึก";
    BtnInsertDate => "Date / time…", "วันที่ / เวลา…";
    SnippetsNote => "Tip: Start triggers with ; (e.g. ;email) to avoid clashing with words. Use “My typo” to auto-correct frequent slips.", "คำแนะนำ: ควรขึ้นต้นคำย่อด้วย ; (เช่น ;email) เพื่อไม่ให้ชนกับคำทั่วไป หรือใช้ “คำผิดของฉัน” เพื่อแก้คำสะกดผิดอัตโนมัติ";
    SnipTriggerLength => "A trigger is 2 to 32 characters.", "คำย่อต้องยาว 2–32 ตัวอักษร";
    SnipTriggerSpace => "A trigger has no spaces.", "คำย่อต้องไม่มีช่องว่าง";
    SnipTextEmpty => "Write the text it becomes.", "ใส่ข้อความที่ต้องการ";
    SnipMacroStep => "A step in braces can't run. Keys as {press Ctrl+B} or {press Enter}, waits up to {wait 5000}, and {command …} or {style …} with a name.", "คำสั่งในวงเล็บปีกกาใช้ไม่ได้ ปุ่มเขียนแบบ {กด Ctrl+B} หรือ {กด Enter} รอได้ไม่เกิน {รอ 5000} และ {คำสั่ง …} หรือ {สไตล์ …} ต้องมีชื่อ";
    SnipTextLong => "The text is too long (1,000 characters at most).", "ข้อความยาวเกินไป (ไม่เกิน 1,000 ตัวอักษร)";
    SnipTooMany => "200 snippets at most.", "มีคำย่อได้ไม่เกิน 200 รายการ";
    SnipRemoved => "Removed.", "ลบแล้ว";
    AppsIntro => "Configure correction mode per application. Unlisted apps follow the default mode.", "กำหนดโหมดการทำงานแยกตามโปรแกรม แอปที่ไม่อยู่ในรายการจะใช้โหมดหลัก";
    ColApp => "App", "แอป";
    ColMode => "Mode", "โหมด";
    ColSetBy => "Set by", "ตั้งโดย";
    ColWhere => "Location", "ตำแหน่งไฟล์";
    SetByForNow => "This time only", "เฉพาะครั้งนี้";
    SetByYou => "You", "คุณ";
    SetByYouBlocked => "You (blocked)", "คุณ (บล็อก)";
    SetByDefault => "Default: code editor", "ค่าเริ่มต้น: โปรแกรมเขียนโค้ด";
    SetBySafety => "Safety (built in)", "ความปลอดภัย (ในตัว)";
    ModeAlwaysOff => "Always off", "ปิดเสมอ";
    ModeBlocked => "Off (blocked)", "ปิด (บล็อก)";
    BtnAddApp => "Add an app that is open…", "เพิ่มจากแอปที่เปิดอยู่…";
    BtnKeepMode => "Keep for good", "ใช้ตลอด";
    BtnRemoveApp => "Remove", "ลบออก";
    BtnShowFile => "Show the program file", "เปิดตำแหน่งไฟล์";
    AppsPickFirst => "Select an app in the list first.", "เลือกแอปในรายการก่อน";
    AppsBuiltIn => "Built in — RightType keeps this one as it is.", "ตั้งไว้ในตัว เปลี่ยนไม่ได้";
    AppsRemoved => "{app} uses the general mode again.", "{app} กลับไปใช้โหมดหลักแล้ว";
    AppsKeepWhat => "Only a mode chosen “this time only” can be kept.", "ใช้ได้กับแอปที่ตั้งไว้ “เฉพาะครั้งนี้” เท่านั้น";
    AppsNoneRunning => "No other app with a window is open.", "ไม่มีแอปอื่นที่เปิดหน้าต่างอยู่";
    AppsAdded => "Added {app} — now pick its mode below.", "เพิ่ม {app} แล้ว เลือกโหมดด้านล่างได้เลย";
    RowRestart => "Start again after a crash", "เปิดใหม่เองเมื่อโปรแกรมล่ม";
    SubRestartShort => "Not after you quit it or end it.", "ไม่เปิดเองหลังคุณสั่งปิด";
    SubRestart => "Not after you quit it or end it in Task Manager. From the next start.", "ไม่เปิดเองหลังจากคุณสั่งปิด หรือปิดจาก Task Manager มีผลตั้งแต่เปิดครั้งถัดไป";
    ToastOfferMode => "Fixes keep being taken back in {app} — {keys} to switch it to {mode}", "ใน {app} มีการย้อนคำที่แก้บ่อย กด {keys} เพื่อเปลี่ยนเป็นโหมด{mode}";
    PaletteOfferForNow => "{mode} in {app}, only this time", "ใช้โหมด{mode}ใน {app} เฉพาะครั้งนี้";
    PaletteOfferKeep => "{mode} in {app} from now on (add to the Apps list)", "ใช้โหมด{mode}ใน {app} ตลอด (เพิ่มในรายการแอป)";
    ToastModeForNow => "{mode} in {app} until RightType restarts", "ใช้โหมด{mode}ใน {app} จนกว่าจะปิด RightType";
    TrayCode => "Code (for code editors)", "โค้ด (สำหรับเขียนโปรแกรม)";
    ModeCode => "Code", "โค้ด";
    SayFixed => "Fixed: {word}", "แก้เป็น {word}";
    PaletteSecWords => "Words just typed", "คำที่เพิ่งพิมพ์";
    PaletteSecFix => "Fix text", "แก้ข้อความ";
    PaletteSecSelection => "Selected text", "ข้อความที่เลือก";
    PaletteSecHere => "Here", "ที่นี่";
    PaletteSecHereApp => "Here · {app}", "ที่นี่ · {app}";
    PaletteSecMode => "Mode", "โหมด";
    PaletteSecOptions => "Options", "ตัวเลือก";
    PaletteMoreOptions => "Options and settings", "ตัวเลือกและการตั้งค่า";
    PaletteAppOffShort => "Off in this app", "ปิดในแอปนี้";
    PaletteAppOnShort => "On again in this app", "เปิดในแอปนี้อีกครั้ง";
    PaletteHyphens => "Hyphens after prefixes", "ใส่ขีดหลังคำนำหน้า";
    HintOn => "On", "เปิด";
    HintOff => "Off", "ปิด";
    HintInUse => "in use", "ใช้อยู่";
    PaletteHistoryHint => "Recent words · Space ticks more than one · Enter flips them", "คำล่าสุด · Space เลือกหลายคำ · Enter แปลงคำที่เลือก";
    ToastSpellingFixed => "Spelling: {wrong} → {right} · Backspace or Shift+Backspace puts it back", "แก้คำสะกด: {wrong} → {right} · กด Backspace หรือ Shift+Backspace เพื่อคืนคำเดิม";
    ToastSpellingKept => "Put back as typed — “{word}” will not be changed again", "คืนคำที่พิมพ์ไว้แล้ว จะไม่แก้ “{word}” อีก";
    PaletteSpelling => "Fix common Thai misspellings", "แก้คำไทยที่สะกดผิดบ่อย";
    RowSpelling => "Fix common Thai misspellings", "แก้คำไทยที่สะกดผิดบ่อย";
    SubSpellingShort => "Fixes about 60 common misspellings in Auto.", "แก้คำสะกดผิดที่พบบ่อยราว 60 คำ ในโหมดอัตโนมัติ";
    InfoMore => "More information", "ข้อมูลเพิ่มเติม";
    SubSpelling => "อนุญาติ → อนุญาต and about 60 more, in Auto. Each fix is shown; Backspace right after puts it back.", "เช่น อนุญาติ → อนุญาต และอีกราว 60 คำ (โหมดอัตโนมัติ) แจ้งทุกครั้งที่แก้ กด Backspace ทันทีเพื่อคืนคำเดิม";
    PaletteFieldOff => "Off in this field", "ปิดเฉพาะช่องนี้";
    PaletteFieldOn => "On again in this field", "เปิดในช่องนี้อีกครั้ง";
    ToastFieldOff => "Off in this field until RightType restarts — the rest of the app still works", "ปิดในช่องนี้แล้ว (จนกว่าจะปิด RightType) ช่องอื่นในแอปยังทำงานตามปกติ";
    ToastFieldOn => "On again in this field", "เปิดในช่องนี้แล้ว";
    ToastAlreadyRunning => "RightType is already running — it's in the tray (bottom right)", "RightType เปิดอยู่แล้ว อยู่ที่ tray มุมขวาล่าง";
    ToastReplaced => "Now running RightType {v} (closed {old})", "เปลี่ยนเป็น RightType {v} แล้ว (ปิดตัว {old})";
    ToastReplacedUnknown => "Now running RightType {v} (closed the copy that was running)", "เปลี่ยนเป็น RightType {v} แล้ว (ปิดตัวที่เปิดอยู่ก่อน)";
    ToastRestarted => "RightType stopped unexpectedly and was started again", "RightType หยุดทำงานกะทันหัน จึงเปิดใหม่ให้แล้ว";
    ToastVerifyDiffers => "This app showed the fix differently — check the last word (Ctrl+Z undoes it). RightType will type slower here.", "แอปนี้แสดงคำที่แก้ไม่ตรง ลองดูคำล่าสุด (Ctrl+Z ย้อนได้) ต่อไป RightType จะพิมพ์ช้าลงในแอปนี้";
    ToastLearnedCleared => "Learned words cleared", "ล้างคำที่เรียนรู้แล้ว";
    ToastSuggestAccept => "Alt+CapsLock", "Alt+CapsLock";
    ErrUndoInject => "RightType: could not undo", "RightType: ย้อนกลับไม่สำเร็จ";
    ErrSuggestInject => "RightType: could not apply the suggestion", "RightType: ใช้คำแนะนำไม่สำเร็จ";
    ErrCorrectionInject => "RightType: could not fix the word", "RightType: แก้คำไม่สำเร็จ";
    ErrClipboardBusy => "RightType: the clipboard is busy — try again", "RightType: คลิปบอร์ดกำลังถูกใช้ ลองอีกครั้ง";
    ErrSelectionNotShared => "RightType: this app does not share the selected text — use Fix text… in the tray", "RightType: แอปนี้ไม่ส่งข้อความที่เลือกให้ ใช้ ซ่อมข้อความ… ที่ถาดไอคอนแทน";
    ErrClipboardNotPlain => "RightType: converting a selection needs a plain-text clipboard", "RightType: การแปลงข้อความที่เลือกต้องใช้คลิปบอร์ดที่เป็นข้อความธรรมดา";
    ErrModifiers => "RightType: could not release held keys", "RightType: ปล่อยปุ่มที่กดค้างไม่สำเร็จ";
    ErrCopy => "RightType: could not copy the selection", "RightType: คัดลอกข้อความที่เลือกไม่สำเร็จ";
    ErrNothingCopied => "RightType: nothing was selected", "RightType: ไม่ได้เลือกข้อความ";
    ErrNotUnicode => "RightType: the selection is not text", "RightType: สิ่งที่เลือกไม่ใช่ข้อความ";
    ErrRestoreClipboard => "RightType: could not restore the clipboard", "RightType: คืนค่าคลิปบอร์ดไม่สำเร็จ";
    ErrInjectConversion => "RightType: could not type the conversion", "RightType: พิมพ์ข้อความที่แปลงไม่สำเร็จ";
    ErrSelectionUndo => "RightType: could not undo the selection", "RightType: ย้อนกลับข้อความที่เลือกไม่สำเร็จ";

    // Settings window.
    SettingsTitle => "RightType — Settings", "RightType — ตั้งค่า";
    NavGeneral => "General", "ทั่วไป";
    NavHotkeys => "Hotkeys", "ปุ่มลัด";
    NavKeyboard => "Keyboard", "แป้นพิมพ์";
    HeadWhileTyping => "While you type", "ระหว่างพิมพ์";
    HeadStateKeys => "Keys that change how you type", "ปุ่มที่เปลี่ยนวิธีพิมพ์";
    RowAddresses => "Web addresses, email and numbers", "ที่อยู่เว็บ อีเมล และตัวเลข";
    SubAddressesShort => "Fixes name@gmail.com, www.…, 100, 10:30.", "แก้ name@gmail.com, www.…, 100, 10:30 ให้ถูก";
    SubAddresses => "Typed with the Thai keyboard on, they come back: name@gmail.com, www.…, 100, 10:30.", "พิมพ์ตอนแป้นไทยเปิดอยู่ แก้กลับให้ เช่น name@gmail.com, www.…, 100, 10:30";
    RowComplete => "Finish long Thai words with Tab", "เติมคำไทยยาวด้วย Tab";
    SubCompleteShort => "Shows the rest of a long word; Tab takes it.", "แสดงส่วนที่เหลือของคำยาว กด Tab เพื่อเติม";
    SubComplete => "When only one word can follow, the rest is shown next to the cursor; Tab takes it.", "เมื่อเหลือคำเดียวที่เป็นไปได้ จะแสดงส่วนที่เหลือข้างเคอร์เซอร์ กด Tab เพื่อเติม";
    RowGrave => "Grave key (`) types its character", "ปุ่มตัวหนอน (`) พิมพ์ตัวอักษร";
    SubGraveShort => "Switch language with Alt+Shift or CapsLock.", "สลับภาษาด้วย Alt+Shift หรือ CapsLock แทน";
    SubGrave => "Instead of switching language. Switch with Alt+Shift, or let CapsLock switch it.", "แทนการสลับภาษา ใช้ Alt+Shift หรือ CapsLock สลับภาษาแทน";
    RowGuardSwitch => "Undo a language switch made by a shortcut", "เปลี่ยนภาษากลับเมื่อสลับเพราะปุ่มลัด";
    SubGuardSwitchShort => "Puts the language back after a stray shortcut.", "เปลี่ยนภาษากลับเมื่อกดปุ่มลัดพลาด";
    SubGuardSwitch => "Ctrl/Alt + Shift + a key can switch the language by accident; it is put back.", "Ctrl/Alt + Shift + ปุ่ม อาจสลับภาษาโดยไม่ตั้งใจ RightType จะเปลี่ยนกลับให้";
    RowPasswordTag => "TH / CAPS tag at password fields", "แท็ก TH / CAPS ที่ช่องรหัสผ่าน";
    SubPasswordTagShort => "Shows language and CapsLock at password fields.", "บอกภาษาและ CapsLock ที่ช่องรหัสผ่าน";
    SubPasswordTag => "Shows the language and CapsLock before you type a password. Nothing typed is read.", "บอกภาษาและ CapsLock ก่อนพิมพ์รหัสผ่าน ไม่อ่านสิ่งที่พิมพ์";
    RowNumLock => "Keypad with NumLock off", "แป้นตัวเลขตอน NumLock ปิด";
    SubNumLock => "Moves the cursor, not digits.", "เลื่อนเคอร์เซอร์ ไม่พิมพ์ตัวเลข";
    RowInsertKey => "Insert key in text", "ปุ่ม Insert ในช่องข้อความ";
    SubInsertKey => "Some apps overtype letters.", "บางแอปพิมพ์ทับตัวอักษร";
    GuardOff => "Off", "ปิด";
    GuardWarn => "Warn", "เตือน";
    GuardFix => "Fix", "แก้ให้";
    GuardBlock => "Block", "กันไว้";
    ToolClean => "Clean", "ทำความสะอาด";
    ToolTest => "Test keys", "ทดสอบปุ่ม";
    CleanTitle => "Clean the keyboard", "ทำความสะอาดคีย์บอร์ด";
    CleanIntro => "The keyboard is locked while you wipe it: no key reaches any app. It unlocks by itself when the time is up.", "ล็อกคีย์บอร์ดระหว่างเช็ด ไม่มีปุ่มไหนส่งไปถึงแอปเลย และปลดล็อกเองเมื่อหมดเวลา";
    CleanFor => "Lock for", "ล็อกนาน";
    Clean30s => "30 seconds", "30 วินาที";
    Clean1m => "1 minute", "1 นาที";
    Clean2m => "2 minutes", "2 นาที";
    RowLockMouse => "Lock touchpad and mouse", "ล็อกทัชแพดและเมาส์";
    SubLockMouse => "Then only the time unlocks it.", "เมื่อล็อกแล้วจะปลดได้เมื่อหมดเวลาเท่านั้น";
    BtnStartLock => "Lock and start", "ล็อกและเริ่มเช็ด";
    CleanStarting => "Locking in {n}…", "จะล็อกใน {n}…";
    CleanLocked => "Locked: wipe away", "ล็อกแล้ว เช็ดได้เลย";
    CleanLeft => "{t} left", "เหลือ {t}";
    CleanWiped => "{n} of {all} keys pressed", "กดไปแล้ว {n} จาก {all} ปุ่ม";
    BtnUnlock => "Unlock", "ปลดล็อก";
    CleanLimits => "Fn, brightness, Wi-Fi and some laptop keys work inside the laptop itself: no program can hold them. Ctrl+Alt+Del always works.", "ปุ่ม Fn ความสว่าง Wi-Fi และปุ่มพิเศษบางปุ่มบนโน้ตบุ๊กทำงานในตัวเครื่อง ไม่มีโปรแกรมใดกันได้ · Ctrl+Alt+Del ใช้ได้เสมอ";
    CleanDone => "Done: the keyboard is unlocked", "เสร็จแล้ว ปลดล็อกคีย์บอร์ดแล้ว";
    BtnTestKeys => "Test the keys", "ทดสอบปุ่ม";
    KeyTestTitle => "Test the keyboard", "ทดสอบคีย์บอร์ด";
    KeyTestIntro => "Press each key once: it lights up when it works. Keys pressed here go nowhere else.", "กดทีละปุ่ม ปุ่มที่ทำงานจะเปลี่ยนสี ปุ่มที่กดในหน้านี้ไม่ส่งไปที่อื่น";
    KeyTestLast => "Last key: {k} (scan code {code})", "ปุ่มล่าสุด: {k} (รหัส {code})";
    KeyTestNone => "No key pressed yet", "ยังไม่ได้กดปุ่มใด";
    KeyTestEsc => "Hold Esc for 2 seconds to close.", "กด Esc ค้าง 2 วินาทีเพื่อปิด";
    KeyTestCount => "{n} of {all} keys work", "ทำงาน {n} จาก {all} ปุ่ม";
    BtnStartOver => "Start over", "เริ่มใหม่";
    PaletteClean => "Clean the keyboard", "ทำความสะอาดคีย์บอร์ด";
    PaletteKeyTest => "Test the keyboard", "ทดสอบคีย์บอร์ด";
    NavTools => "Tools", "เครื่องมือ";
    HeadHealth => "Keyboard health", "สุขภาพคีย์บอร์ด";
    BtnCheckAgain => "Check again", "ตรวจอีกครั้ง";
    HealthStuck => "Keys held down", "ปุ่มค้าง";
    HealthStuckOk => "None: Shift, Ctrl, Alt and Windows are up.", "ไม่มี: Shift Ctrl Alt และปุ่ม Windows ไม่ค้าง";
    HealthStuckBad => "{keys} counts as held down (often after Remote Desktop).", "{keys} ค้างอยู่ (มักเกิดหลังใช้ Remote Desktop)";
    HealthRelease => "Release", "ปล่อยให้";
    HealthSticky => "Sticky Keys", "Sticky Keys (ปุ่มติด)";
    HealthStickyOk => "Off, and pressing Shift five times does not turn it on.", "ปิดอยู่ และกด Shift 5 ครั้งจะไม่เปิดขึ้นมาเอง";
    HealthStickyOn => "On: Shift, Ctrl and Alt stay down after one press.", "เปิดอยู่: Shift Ctrl Alt จะค้างหลังกดครั้งเดียว";
    HealthStickyHotkey => "Off, but pressing Shift five times turns it on.", "ปิดอยู่ แต่กด Shift 5 ครั้งจะเปิดขึ้นมาเอง";
    HealthFilter => "Filter Keys", "Filter Keys (กรองปุ่ม)";
    HealthFilterOk => "Off, and holding right Shift does not turn it on.", "ปิดอยู่ และกด Shift ขวาค้างจะไม่เปิดขึ้นมาเอง";
    HealthFilterOn => "On: quick or repeated presses are ignored.", "เปิดอยู่: การกดเร็วหรือกดซ้ำจะไม่ถูกนับ";
    HealthFilterHotkey => "Off, but holding right Shift for 8 seconds turns it on.", "ปิดอยู่ แต่กด Shift ขวาค้าง 8 วินาทีจะเปิดขึ้นมาเอง";
    HealthTurnOff => "Turn off", "ปิด";
    HealthKeyboards => "Keyboards installed", "แป้นที่ติดตั้ง";
    HealthThai => "Thai", "ไทย";
    HealthEnglish => "English", "อังกฤษ";
    HealthNoThai => "no Thai keyboard", "ไม่มีแป้นไทย";
    HealthManyThai => "more than one Thai keyboard, so switching can land on the wrong one", "มีแป้นไทยมากกว่าหนึ่งแบบ สลับภาษาอาจไปผิดแป้น";
    HealthLanguageSettings => "Language settings", "ตั้งค่าภาษา";
    HealthSwitch => "Language switch keys", "ปุ่มสลับภาษา";
    HealthSwitchCtrlShift => "Ctrl+Shift: the start of many app shortcuts (Ctrl+Shift+T…), so the language can switch by accident.", "Ctrl+Shift: ตรงกับปุ่มลัดของหลายแอป (Ctrl+Shift+T…) ภาษาอาจสลับโดยไม่ตั้งใจ";
    HealthSwitchNone => "None set (Windows+Space still works).", "ไม่ได้ตั้ง (ยังใช้ Windows+Space ได้)";
    HealthSwitchGrave => "The grave key (`)", "ปุ่มตัวหนอน (`)";
    HealthUseAltShift => "Use Alt+Shift", "ใช้ Alt+Shift";
    HealthAutocorrect => "Windows' own autocorrect", "การแก้คำอัตโนมัติของ Windows";
    HealthAutocorrectOn => "On: it may change an English word again after RightType.", "เปิดอยู่: อาจแก้คำอังกฤษซ้ำหลัง RightType";
    HealthTypingSettings => "Typing settings", "ตั้งค่าการพิมพ์";
    HealthCaps => "CapsLock", "ปุ่ม CapsLock";
    HealthCapsOn => "On: letters come out in capitals.", "เปิดอยู่: ตัวอักษรจะเป็นตัวพิมพ์ใหญ่";
    HealthOff => "Off.", "ปิดอยู่";
    HealthChatter => "Keys typing twice", "ปุ่มเบิ้ล";
    HealthChatterOk => "None seen since RightType started.", "ยังไม่พบตั้งแต่เปิด RightType";
    HealthChatterBad => "{keys} typed twice by itself ({n} times): a worn or dusty switch.", "{keys} พิมพ์ซ้ำเอง ({n} ครั้ง) สวิตช์อาจสึกหรือมีฝุ่น";
    HealthChatterFiltered => "Filtered: {keys}. A second press right after letting go is dropped.", "กรองอยู่: {keys} การกดซ้ำทันทีหลังปล่อยจะไม่ถูกนับ";
    HealthFilterThem => "Filter", "กรองให้";
    HealthStopFilter => "Stop filtering", "เลิกกรอง";
    HeadDevices => "Devices", "อุปกรณ์";
    PracticeTitle => "Typing practice", "ฝึกพิมพ์";
    PracticeKedmanee => "Kedmanee", "เกษมณี";
    PracticePattachote => "Pattachote", "ปัตตะโชติ";
    PracticeManoonchai => "Manoonchai", "มนูญชัย";
    PracticeEnglish => "English", "อังกฤษ";
    PracticeLesson => "Lesson", "บทเรียน";
    PracticeTimed => "60 seconds", "60 วินาที";
    PracticeGame => "Falling words", "คำตก";
    LessonHome => "Home row", "แถวกลาง";
    LessonTop => "+ Top", "+ แถวบน";
    LessonBottom => "+ Bottom", "+ แถวล่าง";
    LessonNumbers => "+ Numbers", "+ ตัวเลข";
    LessonEdges => "+ Edges", "+ ขอบ";
    LessonShift => "+ Shift", "+ Shift";
    LessonMarks => "Tone marks", "วรรณยุกต์";
    LessonDigits => "Thai digits", "เลขไทย";
    PracticeSpeed => "{n} per minute", "{n} ตัว/นาที";
    PracticeAccuracy => "{n}% right", "ถูก {n}%";
    PracticeLeft => "{t} left", "เหลือ {t}";
    ByAppTitle => "How each app is doing", "แก้คำได้ดีแค่ไหนในแต่ละแอป";
    ByAppButton => "By app", "แยกตามแอป";
    ByAppApp => "App", "แอป";
    ByAppFixed => "Fixed", "แก้แล้ว";
    ByAppRight => "As sent", "แสดงถูก";
    ByAppWrong => "Differs", "แสดงเพี้ยน";
    ByAppUndoneCol => "Undone", "ถูกย้อน";
    ByAppNote => "Note", "หมายเหตุ";
    ByAppGood => "Working well", "ทำงานดี";
    ByAppUndone => "Often undone: try Suggest mode", "ย้อนบ่อย ลองโหมดแนะนำ";
    ByAppShownWrong => "Shows fixes oddly: typing slower", "แสดงเพี้ยน จะพิมพ์ช้าลง";
    ByAppUnknown => "Too few fixes to tell", "ยังแก้น้อยเกินจะบอกได้";
    ByAppEmpty => "No fixes yet in this session", "รอบนี้ยังไม่ได้แก้คำ";
    ByAppFoot => "Since RightType started. App names and counts only, never saved; not every app lets RightType read a fix back.", "นับตั้งแต่เปิด RightType มีแค่ชื่อแอปกับจำนวน ไม่บันทึก และบางแอปไม่ให้อ่านคำที่แก้กลับมาตรวจ";
    CaretListTitle => "Search at the cursor", "ค้นที่เคอร์เซอร์";
    CaretListHint => "Search characters and snippets: arrow, degree, ลูกศร…", "ค้นอักขระและคำย่อ: ลูกศร, องศา, arrow…";
    CaretListNone => "Nothing found", "ไม่พบ";
    CaretListNextTime => "Next time: {keys}", "ครั้งหน้ากด {keys} ได้เลย";
    CaretListAgain => "Enter again: {what}", "กด Enter อีกครั้ง: {what}";
    CaretListKeysRun => "↑↓ choose · Enter run · Esc close", "↑↓ เลือก · Enter สั่งงาน · Esc ปิด";
    CaretListKeysGrid => "←→↑↓ choose · Enter type · Ctrl+P pin to a key · type to search", "←→↑↓ เลือก · Enter พิมพ์ · Ctrl+P ผูกกับปุ่ม · พิมพ์เพื่อค้น";
    CaretListPinHint => "Press the key to pin {ch} to · Esc cancels", "กดปุ่มที่จะผูก {ch} ไว้ · Esc ยกเลิก";
    CaretListPinned => "{ch} on {key}: type {key}, then tap Right Alt", "ผูก {ch} กับปุ่ม {key} แล้ว: พิมพ์ {key} แล้วแตะ Right Alt";
    CaretListUnpinned => "{ch} taken off {key}", "เอา {ch} ออกจากปุ่ม {key} แล้ว";
    CaretListKeys => "↑↓ choose · Enter type · Ctrl+P pin · Esc close", "↑↓ เลือก · Enter พิมพ์ · Ctrl+P ผูกกับปุ่ม · Esc ปิด";
    HeadCaret => "At the text cursor", "ที่เคอร์เซอร์";
    RowCaretList => "Tap Shift twice to search", "แตะ Shift สองครั้งเพื่อค้น";
    RowCycleChars => "Tap Right Alt to change the last character", "แตะ Right Alt เพื่อเปลี่ยนอักขระล่าสุด";
    SubCycleCharsShort => "2 → ² → ₂ → ⅔, e → é → è. Ctrl+P pins.", "2 → ² → ₂ → ⅔, e → é → è ผูกด้วย Ctrl+P";
    SubCycleChars => "2 → ² → ₂ → ⅔, e → é → è. Pin your own from the list (Ctrl+P).", "2 → ² → ₂ → ⅔, e → é → è ผูกเพิ่มเองได้จากรายการ (Ctrl+P)";
    RowGhosts => "Offer symbols for what you type, Tab takes it", "เสนอสัญลักษณ์จากที่พิมพ์ กด Tab เพื่อใช้";
    SubGhostsShort => "-> becomes →, != becomes ≠. Not in code editors.", "-> เป็น →  != เป็น ≠ ไม่ทำในโปรแกรมโค้ด";
    SubGhosts => "-> becomes →, != becomes ≠, x^2 becomes x². Not in code editors.", "-> เป็น →  != เป็น ≠  x^2 เป็น x² ไม่ทำในโปรแกรมเขียนโค้ด";
    SubCaretListShort => "Search characters and snippets.", "ค้นอักขระพิเศษและคำย่อ";
    SubCaretList => "Characters and snippets. The search never lands in your text.", "อักขระพิเศษและคำย่อ พิมพ์ลงตรงที่ใช้อยู่ คำที่ค้นไม่ลงไปในข้อความ";
    HeadComfort => "On screen and comfort", "บนจอและสุขภาพมือ";
    RowShowKeys => "Show shortcuts on screen", "แสดงปุ่มลัดบนจอ";
    SubShowKeysShort => "Ctrl+C, Alt+Tab… for teaching. Never letters.", "Ctrl+C, Alt+Tab… สำหรับสอน ไม่แสดงตัวอักษร";
    SubShowKeys => "For teaching or recording: Ctrl+C, Alt+Tab… Never letters.", "Ctrl+C, Alt+Tab, Enter… สำหรับสอนหรืออัดคลิป ไม่แสดงตัวอักษรหรือรหัสผ่าน";
    RowRest => "Remind me to rest my hands", "เตือนให้พักมือ";
    SubRestShort => "After 50 minutes of typing with no 5-minute break.", "เมื่อพิมพ์ 50 นาทีโดยไม่พัก 5 นาที";
    SubRest => "After 50 minutes of typing with no 5-minute break. Counts only.", "เมื่อพิมพ์ต่อเนื่อง 50 นาทีโดยไม่พัก 5 นาที นับแค่จำนวน";
    RestDue => "{minutes} minutes of typing ({keys} keys) — rest your hands for a few minutes", "พิมพ์มา {minutes} นาทีแล้ว ({keys} ปุ่ม) พักมือสักครู่นะ";
    PracticeOnScreen => "Keep this keyboard on screen", "เปิดแป้นนี้ค้างไว้บนจอ";
    PracticeOnScreenTip => "While you work, the keyboard map stays on top; keys you know from practice fade.", "ระหว่างทำงาน แผนผังแป้นอยู่บนสุด ปุ่มที่คล่องจากการฝึกจะจางลง";
    PracticeHelp => "Type the highlighted text. Tab: new text · Esc: start over. Works without the keyboard installed.", "พิมพ์ตามข้อความที่ไฮไลต์ Tab: ข้อความใหม่ · Esc: เริ่มใหม่ ฝึกได้แม้ยังไม่ได้ติดตั้งแป้นนั้น";
    PracticeDone => "Done: {speed}, {acc}", "เสร็จแล้ว: {speed} {acc}";
    PracticeBestToday => "Best today: {speed}", "ดีที่สุดวันนี้: {speed}";
    PracticeGameScore => "Score {n}", "คะแนน {n}";
    PracticeLevel => "Level {n}", "ระดับ {n}";
    PracticeCombo => "{n} in a row", "ต่อเนื่อง {n} คำ";
    PracticeBestCombo => "Best run {n}", "ต่อเนื่องสูงสุด {n} คำ";
    PracticeGameOver => "Game over: {n} points. Tab plays again.", "จบเกม: {n} คะแนน กด Tab เพื่อเล่นใหม่";
    RowKeepScores => "Keep a score per day", "เก็บคะแนนรายวัน";
    SubKeepScores => "Speed and accuracy per lesson and day, and which keys are missed. Never the text.", "ความเร็วและความถูกต้องต่อบทต่อวัน และปุ่มที่พลาดบ่อย ไม่เก็บข้อความ";
    PalettePractice => "Typing practice", "ฝึกพิมพ์";
    SheetTitle => "Shortcuts: {app}", "ปุ่มลัด: {app}";
    SheetSearch => "Search", "ค้นหา";
    SheetKeys => "Keys", "ปุ่ม";
    SheetWhat => "What it does", "ทำอะไร";
    SheetNote => "Enter or a double-click uses the shortcut in the app. Esc closes.", "กด Enter หรือดับเบิลคลิกเพื่อใช้ปุ่มลัดนั้นในแอป กด Esc เพื่อปิด";
    PaletteSheet => "This app's shortcuts", "ปุ่มลัดของแอปนี้";
    RowCtrlHold => "Hold Ctrl to see the app's shortcuts", "กด Ctrl ค้างเพื่อดูปุ่มลัดของแอป";
    SubCtrlHoldShort => "Hold Ctrl for a second, with no other key.", "กด Ctrl ค้าง 1 วินาที โดยไม่กดปุ่มอื่น";
    SubCtrlHold => "A second with no other key. The list is also in the command palette.", "ค้างไว้ 1 วินาทีโดยไม่กดปุ่มอื่น (เปิดจากเมนูคำสั่งได้ด้วย)";
    RowShortcutsEnglish => "Ctrl and Alt shortcuts in English", "ปุ่มลัด Ctrl / Alt เป็นภาษาอังกฤษ";
    SubShortcutsEnglishShort => "Ctrl+C stays Ctrl+C with the Thai keyboard on.", "Ctrl+C ยังเป็น Ctrl+C แม้เปิดแป้นไทย";
    SubShortcutsEnglish => "With the Thai keyboard on, Ctrl+C stays Ctrl+C in web apps that read the letter.", "เมื่อแป้นไทยเปิดอยู่ Ctrl+C จะยังเป็น Ctrl+C ในเว็บแอปที่อ่านตัวอักษร";
    RowHoldAccents => "Hold a key for more characters", "กดค้างเพื่อเลือกอักขระพิเศษ";
    SubHoldAccentsShort => "Hold . for …, e for é, a digit for a numeral.", "กดค้าง . ได้ … กด e ได้ é กดเลขได้เลขไทย";
    SubHoldAccents => "Hold . for … ·, e for é, a digit for its Thai numeral, then press a number.", "กดค้าง . ได้ … · กดค้าง e ได้ é กดค้างตัวเลขได้เลขไทย แล้วกดเลขเพื่อเลือก";
    RowScanner => "Barcode scanners", "เครื่องสแกนบาร์โค้ด";
    SubScannerShort => "A scan comes back as its digits and letters.", "ค่าที่สแกนกลับเป็นตัวเลขและตัวอักษรเดิม";
    SubScanner => "With the Thai keyboard on, a scan comes back as its digits and letters.", "เมื่อแป้นไทยเปิดอยู่ ค่าที่สแกนจะกลับเป็นตัวเลขและตัวอักษรเดิม";
    RowFakeKeyboard => "Block fake keyboards", "กันคีย์บอร์ดปลอม";
    SubFakeKeyboardShort => "Holds back a device typing faster than a hand.", "กันอุปกรณ์ที่พิมพ์เร็วเกินมือคน";
    SubFakeKeyboard => "A device typing into the Run box faster than any hand is held back.", "อุปกรณ์ที่พิมพ์ลงช่อง Run เร็วเกินมือคนจะถูกกันไว้ รวมถึง Enter";
    ToastFakeKeyboard => "A device typed into the Run box faster than any hand: its keys, Enter included, were held back. Close that box if you did not mean it.", "มีอุปกรณ์พิมพ์ลงช่อง Run เร็วเกินมือคน RightType กันปุ่มไว้แล้วรวมถึง Enter ถ้าไม่ได้ตั้งใจให้ปิดหน้าต่างนั้น";
    BrandTagline => "Type in the right layout automatically", "พิมพ์ถูกแป้น ไม่ต้องสลับเอง";
    NavLearned => "Learned words", "คำที่จำไว้";
    NavActivity => "Recent messages", "ข้อความล่าสุด";
    ActivityIntro => "What the pill said lately, newest first. Kept in memory only, forgotten when RightType closes.", "สิ่งที่ pill แจ้งล่าสุด เรียงใหม่สุดก่อน เก็บในหน่วยความจำเท่านั้น ลืมเมื่อปิด RightType";
    ActivityEmpty => "No messages yet.", "ยังไม่มีข้อความ";
    ActivityCleared => "Cleared.", "ล้างแล้ว";
    ColWhen => "Time", "เวลา";
    ColKind => "Kind", "ชนิด";
    ColMessage => "Message", "ข้อความ";
    KindDone => "Done", "สำเร็จ";
    KindUndo => "Undone", "ย้อนกลับ";
    KindInfo => "Info", "ข้อมูล";
    KindAttention => "Check", "ควรดู";
    KindProblem => "Problem", "ปัญหา";
    RowKeepMessages => "Remember recent messages", "จำข้อความล่าสุด";
    SubKeepMessages => "The last 50, in memory only. Never saved to disk.", "50 ข้อความล่าสุด ในหน่วยความจำเท่านั้น ไม่เขียนลงดิสก์";
    NavBlocked => "Ignored apps", "แอปที่ไม่ใช้";
    NavAbout => "Privacy", "ความเป็นส่วนตัว";
    NavSponsor => "Support", "สนับสนุน";
    HeadMode => "Correction mode", "วิธีแก้คำ";
    DescAuto => "Fixes words as you type — Thai as soon as your keys spell a Thai word, English when you press Space.", "แก้ให้ทันทีขณะพิมพ์ — เป็นภาษาไทยทันทีที่ปุ่มที่กดสะกดเป็นคำไทย และเป็นภาษาอังกฤษเมื่อกดเว้นวรรค";
    DescSuggest => "Shows the fix in a small hint next to the cursor. Press Tab (or Alt+CapsLock) to use it.", "แสดงคำที่ถูกเป็นคำแนะนำเล็ก ๆ ข้างเคอร์เซอร์ กด Tab (หรือ Alt+CapsLock) เพื่อใช้คำนั้น";
    DescManual => "Never changes text by itself. Press Shift+Backspace to fix the last word.", "ไม่แก้ข้อความเอง กด Shift+Backspace เพื่อแก้คำล่าสุด";
    HeadBehaviour => "Behaviour", "การทำงาน";
    RowEnabled => "RightType is on", "เปิดใช้งาน RightType";
    SubEnabled => "Turn it off or on anytime with Ctrl+Alt+CapsLock.", "เปิดหรือปิดได้ทุกเมื่อด้วย Ctrl+Alt+CapsLock";
    RowStartup => "Start with Windows", "เปิดพร้อม Windows";
    SubStartup => "Runs quietly in the notification area when you sign in.", "ทำงานเงียบ ๆ ที่มุมจอเมื่อเข้าสู่ระบบ";
    RowLearn => "Learn new words", "เรียนรู้คำใหม่";
    SubLearnShort => "Remembers words you keep, on this PC only.", "จำคำที่คุณใช้บ่อย เก็บในเครื่องนี้เท่านั้น";
    SubLearn => "Remembers words you type often or put back. Saved on this PC only.", "จำคำที่คุณพิมพ์บ่อยหรือแก้กลับ บันทึกไว้ในเครื่องนี้เท่านั้น";
    RowCaret => "Hints at the text cursor", "แสดงป้ายข้างเคอร์เซอร์";
    SubCaretShort => "A TH / EN tag at the cursor, and Suggest hints.", "ป้าย TH / EN ข้างเคอร์เซอร์ และคำแนะนำ";
    SubCaret => "A TH / EN tag when the language switches, and Suggest hints (Tab to use).", "ป้าย TH / EN เมื่อสลับภาษา และคำแนะนำ (กด Tab เพื่อใช้)";
    LearnedCount => "Learned words: {n}", "คำที่เรียนรู้แล้ว: {n}";
    BtnEditLearned => "Edit…", "แก้ไข…";
    HeadLanguage => "Language", "ภาษา";
    HeadHotkeys => "Hotkeys", "ปุ่มลัด";
    HeadThaiKeyboard => "Keyboards", "แป้นพิมพ์";
    NoteKeyboards => "Found automatically. Thai: Kedmanee, Pattachote, Manoonchai. English: US, UK, Dvorak, Colemak. Any other keyboard: RightType stays off.", "ตรวจให้อัตโนมัติ ไทย: Kedmanee, Pattachote, Manoonchai อังกฤษ: US, UK, Dvorak, Colemak แป้นอื่น RightType จะหยุดทำงาน";
    KbDetected => "Detected: {thai} · {english}", "ตรวจพบ: {thai} · {english}";
    KbKedmanee => "Thai Kedmanee", "ไทย Kedmanee";
    KbPattachote => "Thai Pattachote", "ไทย Pattachote";
    KbManoonchai => "Thai Manoonchai", "ไทย Manoonchai";
    KbUs => "English US", "อังกฤษ US";
    KbUk => "English UK", "อังกฤษ UK";
    KbDvorak => "English Dvorak", "อังกฤษ Dvorak";
    KbColemak => "English Colemak", "อังกฤษ Colemak";
    NoteHotkeys => "Hotkeys work in every app. To change one, click Change and press the new keys.", "ปุ่มลัดใช้ได้ทุกแอป กด เปลี่ยน แล้วกดปุ่มใหม่ที่ต้องการ";
    HkPalette => "Open the command palette", "เปิดเมนูคำสั่ง";
    BtnChange => "Change", "เปลี่ยน";
    BtnResetKeys => "Reset all", "คืนค่าเดิม";
    HkPress => "Press the new keys for: {v} — Esc cancels.", "กดปุ่มใหม่สำหรับ: {v} — Esc เพื่อยกเลิก";
    HkUnusable => "That would get in the way of typing — use Ctrl or Alt with it.", "ปุ่มนี้จะรบกวนการพิมพ์ — ใช้ร่วมกับ Ctrl หรือ Alt";
    HkTaken => "Already used for: {v}", "ใช้กับ: {v} อยู่แล้ว";
    HkSavedCommon => "Saved. Most apps use these keys for {v}: RightType now takes them first.", "บันทึกแล้ว แอปส่วนใหญ่ใช้ปุ่มนี้สำหรับ{v} ตอนนี้ RightType จะได้ปุ่มนี้ก่อน";
    HkSavedElsewhere => "Saved. Another program uses these keys too: RightType takes them first.", "บันทึกแล้ว มีโปรแกรมอื่นใช้ปุ่มนี้อยู่ด้วย RightType จะได้ปุ่มนี้ก่อน";
    ScCopy => "copying", "คัดลอก";
    ScPaste => "pasting", "วาง";
    ScCut => "cutting", "ตัด";
    ScUndo => "undo", "เลิกทำ";
    ScRedo => "redo", "ทำซ้ำ";
    ScSelectAll => "selecting all", "เลือกทั้งหมด";
    ScSave => "saving", "บันทึก";
    ScFind => "finding", "ค้นหา";
    ScPrint => "printing", "สั่งพิมพ์";
    ScNew => "a new file or window", "สร้างใหม่";
    ScNewTab => "a new tab", "เปิดแท็บใหม่";
    ScCloseTab => "closing a tab", "ปิดแท็บ";
    ScReopenTab => "reopening a closed tab", "เปิดแท็บที่ปิดไปกลับมา";
    ScTaskManager => "Task Manager", "เปิดตัวจัดการงาน";
    ScCloseWindow => "closing the window", "ปิดหน้าต่าง";
    ScSwitchWindow => "switching windows", "สลับหน้าต่าง";
    HkReset => "All hotkeys are back to the defaults.", "คืนปุ่มลัดทั้งหมดเป็นค่าเดิมแล้ว";
    PaletteHead => "↑↓ Enter · 1–9 · type to search · Esc", "↑↓ Enter · 1–9 · พิมพ์เพื่อค้นหา · Esc";
    PaletteBackHead => "‹ Backspace back · ↑↓ Enter · 1–9 · Esc", "‹ Backspace ย้อนกลับ · ↑↓ Enter · 1–9 · Esc";
    PaletteFiltering => "Search: {text}", "ค้นหา: {text}";
    HkKeyMap => "Keyboard map", "แผนผังแป้นพิมพ์";
    KeyMapTitle => "Keyboard map", "แผนผังแป้นพิมพ์";
    KeyMapHead => "Click a key to type it · Shift for the upper characters", "คลิกปุ่มเพื่อพิมพ์ตัวนั้น · Shift สำหรับตัวบน";
    PaletteKeyMap => "Keyboard map (Ctrl+Alt+K)", "แผนผังแป้นพิมพ์ (Ctrl+Alt+K)";
    PaletteCompleteThai => "Complete long Thai words with Tab", "เติมคำไทยยาวด้วย Tab";
    ScopeTypo => "My typo", "คำผิดของฉัน";
    PaletteWhy => "Why? (the last word)", "ทำไม? (คำล่าสุด)";
    WhyNothing => "No word typed here yet to explain", "ยังไม่มีคำที่พิมพ์ตรงนี้ให้อธิบาย";
    WhyFixedWord => "Fixed: read with the other keyboard it is a dictionary word", "แก้แล้ว: อ่านเป็นอีกแป้นแล้วได้คำในพจนานุกรม";
    WhyFixedWords => "Fixed: read with the other keyboard it is known words in a row", "แก้แล้ว: อ่านเป็นอีกแป้นแล้วได้คำที่รู้จักต่อกันทั้งหมด";
    WhyFixedAddress => "Fixed: an email or web address typed with the Thai keyboard on", "แก้แล้ว: เป็นอีเมลหรือเว็บที่พิมพ์ตอนเปิดแป้นไทย";
    WhyFixedNumber => "Fixed: a number typed with the Thai keyboard on", "แก้แล้ว: เป็นตัวเลขที่พิมพ์ตอนเปิดแป้นไทย";
    WhyFixedSpelling => "Fixed: a common misspelling (or an English prefix's hyphen)", "แก้แล้ว: คำที่มักสะกดผิด (หรือขีดหลังคำนำหน้าภาษาอังกฤษ)";
    WhyFixedCaps => "Fixed: typed with CapsLock left on by accident", "แก้แล้ว: พิมพ์ตอน CapsLock ค้าง";
    WhyFixedCode => "Fixed by Code mode: Thai keys typed for code", "แก้แล้วโดยโหมดโค้ด: พิมพ์โค้ดด้วยแป้นไทย";
    WhySuggested => "Offered, not fixed: Suggest mode (Tab takes it)", "เสนอแต่ไม่แก้: โหมดแนะนำ (กด Tab เพื่อรับ)";
    WhyKeptManual => "Not fixed: Manual mode — Shift+Backspace fixes it", "ไม่แก้: โหมดกดแก้เอง — กด Shift+Backspace เพื่อแก้";
    WhyKeptEnglish => "Left as typed: it is an English word (in the dictionary or learned)", "ไม่แก้: เป็นคำอังกฤษ (อยู่ในพจนานุกรมหรือที่เรียนรู้ไว้)";
    WhyKeptThai => "Left as typed: it is Thai", "ไม่แก้: เป็นภาษาไทยอยู่แล้ว";
    WhyKeptUnknown => "Left as typed: neither keyboard gives a known word", "ไม่แก้: อ่านได้ทั้งสองแป้นแต่ไม่พบคำที่รู้จัก";
    WhyKeptSeed => "Left as typed: it may be part of a recovery phrase, so RightType stays out", "ไม่แก้: อาจเป็นส่วนหนึ่งของวลีกู้คืนกระเป๋าเงิน RightType จึงไม่แตะ";
    WhyHerePassword => "Nothing is fixed here: a password field", "ช่องนี้ไม่แก้อะไร: เป็นช่องรหัสผ่าน";
    WhyHereOff => "Nothing is fixed here: RightType is off in this app", "ไม่แก้อะไรในแอปนี้: ปิด RightType ไว้";
    PaletteSpacing => "Thai spacing (ๆ ฯลฯ brackets)", "จัดเว้นวรรคแบบไทย (ๆ ฯลฯ วงเล็บ)";
    ToastSwitchUndone => "The language switched with a shortcut: put back", "ภาษาเปลี่ยนไปพร้อมปุ่มลัด: เปลี่ยนกลับให้แล้ว";
    PaletteGraveTypes => "The ` key types ` (no language switch)", "ปุ่ม ` พิมพ์ ` (ไม่สลับภาษา)";
    PaletteFixAddresses => "Fix web addresses and numbers on the Thai keyboard", "แก้เว็บ อีเมล ตัวเลขที่พิมพ์บนแป้นไทย";
    PaletteGuardSwitch => "Undo a language switch made by a shortcut", "ย้อนการสลับภาษาที่ติดมากับปุ่มลัด";
    PaletteAppKeyboard => "Keyboard in this app", "แป้นในแอปนี้";
    KeyboardNone => "Not set", "ไม่กำหนด";
    KeyboardThai => "Thai", "ไทย";
    KeyboardEnglish => "English", "อังกฤษ";
    KeyboardOutsideText => "English outside text", "อังกฤษนอกช่องพิมพ์";
    ToastGraveTypes => "The grave key now types `: switch language with Alt+Shift, or turn on CapsLock switches language", "ปุ่มตัวหนอนพิมพ์ ` แล้ว: สลับภาษาด้วย Alt+Shift หรือเปิด CapsLock สลับภาษา";
    AboutWindowsAutocorrect => "Windows' own autocorrect for the keyboard is on (Settings → Time & language → Typing): it may change an English word again after RightType.", "การแก้คำอัตโนมัติของ Windows เปิดอยู่ (Settings → Time & language → Typing) อาจแก้คำอังกฤษซ้ำหลัง RightType";
    ToastGoogleDocsTip => "Google Docs shares its text only with screen-reader support on: Tools → Accessibility", "Google Docs ให้อ่านข้อความเมื่อเปิดการรองรับโปรแกรมอ่านหน้าจอ: เครื่องมือ → การช่วยเหลือพิเศษ";
    PalettePasswordHint => "TH / CAPS tag at password fields", "แท็ก TH / CAPS ที่ช่องรหัสผ่าน";
    PaletteNumLock => "Keypad with NumLock off", "แป้นตัวเลขตอน NumLock ปิด";
    PaletteInsertKey => "Insert key in text", "ปุ่ม Insert ในช่องข้อความ";
    HintWarn => "Warn", "เตือน";
    HintFix => "Fix", "แก้ให้";
    ToastNumLockOff => "NumLock is off: the keypad moves the cursor instead of typing digits", "NumLock ปิดอยู่: แป้นตัวเลขด้านขวาจะเลื่อนเคอร์เซอร์แทนการพิมพ์ตัวเลข";
    ToastInsertPressed => "Insert pressed: some apps now type over the text · press it again to go back", "กด Insert แล้ว: บางแอปจะพิมพ์ทับข้อความ · กดอีกครั้งเพื่อกลับ";
    ToastInsertBlocked => "Insert held back (it turns on typing over the text)", "ไม่ให้ Insert ทำงาน (ปุ่มนี้เปิดการพิมพ์ทับข้อความ)";
    PaletteSecReview => "Check before fixing · {n} words", "ตรวจก่อนแก้ · {n} คำ";
    PaletteReviewHint => "Space ticks or unticks · Enter fixes the ticked · Esc leaves it", "Space ติ๊ก/เอาออก · Enter แก้คำที่ติ๊ก · Esc ไม่แก้";
    PaletteApplyReview => "Fix the ticked words", "แก้คำที่ติ๊กไว้";
    PaletteSecSymbols => "Special characters", "อักขระพิเศษ";
    PaletteMoreSymbols => "Special characters (฿ ๆ ฯ …)", "อักขระพิเศษ (฿ ๆ ฯ …)";
    PaletteMoreSelection => "More changes to the selection", "เปลี่ยนข้อความที่เลือกแบบอื่น";
    PaletteRepairSelection => "Fix only the wrong-keyboard words", "แก้เฉพาะคำที่พิมพ์ผิดแป้น";
    PaletteNormalize => "Thai in standard form", "จัดอักษรไทยเป็นมาตรฐาน";
    PaletteEra => "Year พ.ศ. ↔ ค.ศ.", "ปี พ.ศ. ↔ ค.ศ.";
    PaletteNumberWords => "Number in Thai words", "ตัวเลขเป็นคำอ่าน";
    PaletteBahtWords => "Amount in words (baht)", "จำนวนเงินเป็นตัวอักษร (บาท)";
    PaletteTypeClipboard => "Type the copied text key by key", "พิมพ์ข้อความที่คัดลอกทีละปุ่ม";
    PaletteFixWindow => "Fix text window…", "หน้าต่างซ่อมข้อความ…";
    PaletteEnterGuard => "Check chat messages before sending", "ตรวจข้อความแชทก่อนส่ง";
    PaletteThaiWordDelete => "Ctrl+Backspace deletes one Thai word", "Ctrl+Backspace ลบทีละคำไทย";
    ToastNothingToChange => "Nothing to change in the selection", "ไม่มีอะไรต้องเปลี่ยนในข้อความที่เลือก";
    ToastFieldChanged => "The text changed meanwhile; nothing was fixed", "ข้อความเปลี่ยนไประหว่างนั้น จึงยังไม่ได้แก้";
    ToastOpenedFixWindow => "This app does not share its text: Fix text is open instead", "แอปนี้ไม่ให้อ่านข้อความ จึงเปิดหน้าต่างซ่อมข้อความแทน";
    ToastMacroStopped => "Macro stopped", "หยุด Macro แล้ว";
    ToastMacroNoCommand => "Macro stopped: this app has no command “{name}”", "หยุด Macro: แอปนี้ไม่มีคำสั่ง “{name}”";
    ToastTyping => "Typing {n} characters · Esc stops", "กำลังพิมพ์ {n} ตัวอักษร · Esc เพื่อหยุด";
    ToastTypingStopped => "Typing stopped", "หยุดพิมพ์แล้ว";
    ErrClipboardTooLong => "The copied text is too long to type (over {n} characters)", "ข้อความที่คัดลอกยาวเกินจะพิมพ์ (เกิน {n} ตัวอักษร)";
    ToastEnterHeld => "Not sent: it looks typed on the wrong keyboard · Enter sends it anyway · Shift+Backspace fixes", "ยังไม่ส่ง: ดูเหมือนพิมพ์ผิดแป้น · กด Enter อีกครั้งเพื่อส่ง · Shift+Backspace เพื่อแก้";
    PalettePause => "Pause for 30 minutes", "หยุดชั่วคราว 30 นาที";
    HkFlip => "Fix or flip back the last word", "แก้หรือสลับคำล่าสุดกลับ";
    HkSelection => "Convert the selected text", "แปลงข้อความที่เลือก";
    HkCycle => "Switch correction mode", "สลับโหมดการแก้คำ";
    HkUndo => "Undo the last fix", "ย้อนการแก้ล่าสุด";
    HkAccept => "Use the suggestion", "ใช้คำแนะนำ";
    HkPanic => "Turn RightType off / on", "ปิด / เปิด RightType";
    LearnedIntro => "Words RightType leaves exactly as typed. Add one below, or select a word and press Delete.", "คำที่ RightType จะไม่แก้ ปล่อยไว้ตามที่พิมพ์ เพิ่มคำได้ด้านล่าง หรือเลือกคำแล้วกด Delete เพื่อลบ";
    ColWord => "Word", "คำ";
    BtnAddWord => "Add", "เพิ่ม";
    LearnedPickFirst => "Select a word in the list first.", "เลือกคำในรายการก่อน";
    BtnClearAll => "Clear all", "ล้างทั้งหมด";
    BtnImport => "Import…", "นำเข้า…";
    BtnExport => "Export…", "ส่งออก…";
    LearnedHere => "Saved on this PC only.", "บันทึกไว้ในเครื่องนี้เท่านั้น";
    LearnedInFolder => "Kept in: {v}", "เก็บไว้ที่: {v}";
    BtnSyncFolder => "Sync folder…", "โฟลเดอร์ซิงก์…";
    BtnThisPc => "This PC only", "เฉพาะเครื่องนี้";
    LearnedMoved => "{n} words, merged with any already there.", "{n} คำ รวมกับที่มีอยู่แล้ว";
    LearnedImported => "Added {n} words from the file.", "เพิ่ม {n} คำจากไฟล์แล้ว";
    LearnedImportSkipped => "Added {n} words from the file. Skipped {k} lines that are not a single word.", "เพิ่ม {n} คำจากไฟล์แล้ว ข้าม {k} บรรทัดที่ไม่ใช่คำเดียว";
    LearnedExported => "Saved {n} words to the file.", "บันทึก {n} คำลงไฟล์แล้ว";
    ErrFile => "Could not use that file.", "ใช้ไฟล์นั้นไม่ได้";
    FileFilter => "Text files (*.txt)", "ไฟล์ข้อความ (*.txt)";
    AppsSaved => "Saved.", "บันทึกแล้ว";
    RowPredict => "Guess each field's language", "เดาภาษาของแต่ละช่อง";
    SubPredictShort => "Switches language as you click into a field.", "สลับภาษาเมื่อคลิกเข้าช่องที่ใช้ภาษาเดียว";
    SubPredict => "Switches language as you click into a field you use for one. Counts only.", "สลับภาษาให้เมื่อคลิกเข้าช่องที่ใช้ภาษาเดียว เก็บแค่จำนวนคำ";
    BtnClearHabits => "Clear", "ล้าง";
    ToastHabitsCleared => "Field habits cleared", "ล้างข้อมูลการเดาภาษาแล้ว";
    BlockedAlways => "RightType stays out of password fields, terminals, wallets, and full-screen games automatically.", "RightType ยกเว้นช่องรหัสผ่าน, เทอร์มินัล, กระเป๋าคริปโต และเกมเต็มจอโดยอัตโนมัติ";
    HeadPrivacy => "Privacy", "ความเป็นส่วนตัว";
    PrivacyLicense => "Free software under MIT or Apache-2.0", "ซอฟต์แวร์ฟรี ภายใต้ MIT หรือ Apache-2.0";
    Privacy1 => "Never connects to the internet", "ไม่ต่ออินเทอร์เน็ต";
    SubPrivacy1 => "Everything runs locally. No data is ever sent out.", "ทุกอย่างทำงานบนเครื่องคุณ ไม่มีการส่งข้อมูลออก";
    Privacy2 => "Keeps no typed text", "ไม่เก็บข้อความที่พิมพ์";
    SubPrivacy2 => "Password fields and protected apps are bypassed.", "ช่องรหัสผ่านและแอปที่อยู่ในรายการป้องกันจะถูกข้าม";
    Privacy3 => "Clear remembered data anytime", "ล้างข้อมูลที่จำไว้ได้ทุกเมื่อ";
    SubPrivacy3 => "View and manage in the “Learned words” page.", "ดูและลบได้ที่หน้า “คำที่จำไว้”";
    HeadUpdates => "Updates", "อัปเดต";
    AboutUpdates => "Open download page in browser (app never connects).", "เปิดหน้าดาวน์โหลดบนเว็บเบราว์เซอร์ (ตัวแอปไม่ต่อเน็ต)";
    BtnCheckUpdates => "Check for updates", "ตรวจสอบอัปเดต";
    AboutVersion => "RightType {v}", "RightType {v}";
    HeadSponsor => "Support RightType", "สนับสนุน RightType";
    SponsorFreeHead => "Free and open source", "แอปนี้ฟรีและเปิดซอร์ส";
    SponsorFreeSub => "Support a coffee, or contribute on GitHub. Every feature stays 100% free.", "ร่วมสนับสนุนค่ากาแฟ หรือร่วมพัฒนาบน GitHub ได้ตามสะดวก ทุกฟีเจอร์ใช้งานฟรี 100% เสมอ";
    SponsorChannelHead => "Support options", "ช่องทางสนับสนุน";
    SponsorPpDesc => "PromptPay QR · scan with any Thai banking app", "PromptPay QR · สแกนด้วยแอปธนาคารไทยได้ทุกธนาคาร";
    SponsorLnDesc => "Lightning Network · small tips, instant, low fee", "Lightning Network · ทิปเล็กน้อย โอนไว ค่าธรรมเนียมต่ำ";
    SponsorBtcDesc => "Bitcoin on-chain · for larger amounts", "Bitcoin on-chain · สำหรับยอดใหญ่";
    SponsorQrNote => "QR generated locally. App never connects and does not check balances.", "QR สร้างในเครื่อง RightType ไม่ต่อเน็ตและไม่เช็คยอดเงิน";
    BtnGitHub => "GitHub", "GitHub";
    VersionLabel => "Version {v}", "เวอร์ชัน {v}";
    BtnClose => "Close", "ปิด";

    // Welcome / help window.
    WelcomeTitle => "Welcome to RightType", "ยินดีต้อนรับสู่ RightType";
    HelpTitle => "RightType — Hotkeys", "RightType — ปุ่มลัด";
    WelcomeHeadline => "Wrong layout? Fixed — no retyping.", "พิมพ์ผิดภาษา? แก้ให้ทันที ไม่ต้องพิมพ์ใหม่";
    WelcomeSub => "Thai Kedmanee ↔ US English, in every app. Password fields, terminals and wallets are always left alone.", "ไทยเกษมณี ↔ อังกฤษ US ใช้ได้ทุกแอป เว้นช่องรหัสผ่าน เทอร์มินัล และกระเป๋าคริปโต";
    WelcomeExample => "l;ylfu  →  สวัสดี", "l;ylfu  →  สวัสดี";
    WelcomeTry => "Try it: type l;ylfu and a space on the English keyboard here", "ลองเลย: เปิดแป้นอังกฤษ แล้วพิมพ์ l;ylfu ตามด้วยเว้นวรรคในช่องนี้";
    WelcomeTryManual => "Try it: type l;ylfu on the English keyboard, then press Shift+Backspace", "ลองเลย: เปิดแป้นอังกฤษ พิมพ์ l;ylfu แล้วกด Shift+Backspace ในช่องนี้";
    WelcomeKeyboards => "Keyboards found: {kb}", "แป้นที่พบในเครื่อง: {kb}";
    WelcomeNoThai => "No Thai keyboard is installed. Add one in Windows (Settings → Time & language → Language & region) so RightType can fix English typed by mistake.", "ยังไม่มีแป้นไทยในเครื่อง เพิ่มได้ที่ Windows (ตั้งค่า → เวลาและภาษา → ภาษาและภูมิภาค) เพื่อให้ RightType แก้ข้อความที่พิมพ์ผิดได้";
    WelcomeTryOk => "✓ Fixed. This is what RightType does in every app.", "✓ แก้แล้ว RightType ทำแบบนี้ได้ในทุกแอป";
    WelcomeTryNot => "Not fixed? Make sure the English (US) keyboard is active (Alt+Shift switches), then type l;ylfu and a space again.", "ยังไม่เปลี่ยน? ตรวจว่าเปิดแป้นอังกฤษ (US) อยู่ (สลับด้วย Alt+Shift) แล้วพิมพ์ l;ylfu ตามด้วยเว้นวรรคอีกครั้ง";
    HelpHeadline => "Hotkeys", "ปุ่มลัด";
    WelcomeMode => "Current mode: {mode}. Switch anytime with Ctrl+CapsLock.", "โหมดตอนนี้: {mode} สลับได้ทุกเมื่อด้วย Ctrl+CapsLock";
    BtnGetStarted => "Get started", "เริ่มใช้งาน";
    BtnWelcomeTour => "Show me around (1 min)", "พาดูรอบๆ (1 นาที)";
    WelcomeInTray => "RightType is running — start typing. Right-click its icon for the tour and Settings.", "RightType ทำงานอยู่ พิมพ์ได้เลย คลิกขวาที่ไอคอนเพื่อดูทัวร์และตั้งค่า";
    TourTitle => "RightType tour", "ทัวร์ RightType";
    TourStepOf => "{n} of {total}", "{n} จาก {total}";
    BtnTourNext => "Next", "ถัดไป";
    BtnTourBack => "Back", "ก่อนหน้า";
    BtnTourSkip => "Skip tour", "ข้ามทัวร์";
    BtnTourDone => "Done", "เสร็จแล้ว";
    TourA1Title => "Three ways to fix", "แก้ได้ 3 แบบ";
    TourA1Body => "Auto fixes as you type. Suggest shows the fix and waits for Tab. Manual changes nothing until you press Shift+Backspace.", "อัตโนมัติ: แก้ให้ทันทีที่พิมพ์ แนะนำ: โชว์คำที่แก้แล้วรอให้กด Tab กดแก้เอง: ไม่แก้อะไรจนกว่าจะกด Shift+Backspace";
    TourA2Title => "Off in one keystroke", "ปิดได้ในปุ่มเดียว";
    TourA2Body => "Ctrl+Alt+CapsLock turns RightType off or on in any app. It already stays out of password fields, terminals and wallets.", "Ctrl+Alt+CapsLock เปิดหรือปิด RightType ในทุกแอป และมันเลี่ยงช่องรหัสผ่าน เทอร์มินัล และกระเป๋าคริปโตให้เองอยู่แล้ว";
    TourA3Title => "Every key is yours", "ปุ่มลัดตั้งเองได้ทุกปุ่ม";
    TourA3Body => "Press the button beside a hotkey, then the keys you want. Hotkeys work in every app.", "กดปุ่มข้างปุ่มลัดที่ต้องการเปลี่ยน แล้วกดชุดปุ่มใหม่ ปุ่มลัดใช้ได้ในทุกแอป";
    TourB1Title => "Words it never touches", "คำที่ไม่แตะต้อง";
    TourB1Body => "Words you add here stay exactly as typed. Import or export the list to move it to another PC.", "คำที่เพิ่มไว้ที่นี่จะอยู่ตามที่พิมพ์เสมอ นำเข้าหรือส่งออกรายการเพื่อย้ายไปอีกเครื่องได้";
    TourB2Title => "Short codes that expand", "คำย่อที่ขยายเป็นข้อความ";
    TourB2Body => "Type ;addr and a space and it becomes your address. Dates, key steps and macros work too.", "พิมพ์ ;addr แล้วเว้นวรรค จะกลายเป็นที่อยู่ของคุณ ใช้กับวันที่ ขั้นตอนปุ่ม และ macro ได้ด้วย";
    TourB3Title => "A mode for each app", "แต่ละแอปมีโหมดของตัวเอง";
    TourB3Body => "Pick an app, then a mode: Code leaves everything alone, Off switches RightType off there. Add an app that is open to give it its own.", "เลือกแอป แล้วเลือกโหมด: Code ไม่แตะต้องอะไรเลย ปิด คือปิด RightType ในแอปนั้น เพิ่มจากแอปที่เปิดอยู่ได้เลย";
    TourC1Title => "Four tools for the keyboard", "เครื่องมือสำหรับแป้นพิมพ์ 4 อย่าง";
    TourC1Body => "Clean locks the keys while you wipe it. Test keys shows each key you press. The map shows where every key is. Practice builds speed.", "ทำความสะอาดล็อกปุ่มระหว่างเช็ดแป้น ทดสอบปุ่มแสดงปุ่มที่กด แผนผังแสดงตำแหน่งทุกปุ่ม ฝึกพิมพ์ช่วยให้เร็วขึ้น";
    TourC2Title => "A health check", "ตรวจสุขภาพแป้นพิมพ์";
    TourC2Body => "It looks for stuck keys, Sticky Keys, Filter Keys and the language switch keys, and tells you what to change.", "ตรวจปุ่มค้าง Sticky Keys Filter Keys และปุ่มสลับภาษา แล้วบอกว่าควรแก้อะไร";
    TourD1Title => "What it never does", "สิ่งที่ไม่ทำเด็ดขาด";
    TourD1Body => "Nothing you type is written to disk or sent anywhere. Passwords, terminals and wallets are left alone.", "ไม่บันทึกสิ่งที่พิมพ์ลงดิสก์และไม่ส่งไปที่ไหน ช่องรหัสผ่าน เทอร์มินัล และกระเป๋าคริปโตจะถูกเว้นไว้";
    TourD2Title => "Updates only when you ask", "อัปเดตเมื่อคุณสั่งเท่านั้น";
    TourD2Body => "Check for updates opens the download page in your browser. RightType itself never goes online.", "ปุ่มตรวจสอบอัปเดตเปิดหน้าดาวน์โหลดในเบราว์เซอร์ ตัวแอปไม่เชื่อมต่ออินเทอร์เน็ตเอง";

    // Fix text window.
    FixTitle => "RightType — Fix text", "RightType — ซ่อมข้อความ";
    FixHead => "Fix text", "ซ่อมข้อความ";
    FixIntro => "Paste text typed in the wrong layout. Each word is fixed the same way RightType fixes it while you type. Nothing is saved.", "วางข้อความที่พิมพ์ผิดภาษา แล้วแต่ละคำจะถูกแก้แบบเดียวกับตอนพิมพ์ ไม่มีการบันทึกข้อความเลย";
    FixInput => "Your text", "ข้อความของคุณ";
    FixOutput => "Fixed", "ข้อความที่แก้แล้ว";
    BtnFix => "Fix", "แก้";
    BtnPasteFix => "Paste and fix", "วางแล้วแก้";
    BtnCopy => "Copy", "คัดลอก";
    FixChanged => "Words fixed ({n}):", "คำที่แก้ ({n}):";
    FixNothing => "Nothing needed fixing.", "ไม่มีคำที่ต้องแก้";
    FixCopied => "Copied — paste it where you need it.", "คัดลอกแล้ว วางได้เลย";
    ErrClipboardEmpty => "The clipboard has no text.", "คลิปบอร์ดไม่มีข้อความ";

    // Statistics window.
    StatsTitle => "RightType — Statistics", "RightType — สถิติ";
    StatsHead => "Since RightType started", "ตั้งแต่เปิดโปรแกรม";
    StatsAuto => "Fixed automatically", "แก้อัตโนมัติ";
    StatsManual => "Fixed with a hotkey", "แก้ด้วยปุ่มลัด";
    StatsLearned => "Learned words", "คำที่เรียนรู้";
    StatsSaved => "Time saved (estimate)", "เวลาที่ประหยัด (ประมาณ)";
    StatsSavedValue => "{n} min", "{n} นาที";
    StatsSavedSeconds => "{n} s", "{n} วินาที";
    StatsWeekHead => "Last 7 days", "7 วันที่ผ่านมา";
    StatsWeekTotal => "{n} words fixed this week, about {m} min saved.", "สัปดาห์นี้แก้ {n} คำ ประหยัดเวลาราว {m} นาที";
    StatsWeekOff => "Turn on daily counts below to see the last 7 days.", "เปิดการเก็บจำนวนรายวันด้านล่างเพื่อดู 7 วันที่ผ่านมา";
    RowKeepStats => "Keep daily counts on this PC", "เก็บจำนวนรายวันไว้ในเครื่องนี้";
    SubKeepStats => "Two numbers per day, never what you type. Off deletes them.", "เก็บแค่ตัวเลขวันละสองค่า ไม่เก็บสิ่งที่พิมพ์ ปิดแล้วลบทิ้ง";
    StatsNote => "Counts start again when RightType restarts. Nothing you type is saved.", "ตัวเลขจะเริ่มใหม่เมื่อเปิดโปรแกรมใหม่ และไม่มีการบันทึกสิ่งที่คุณพิมพ์";
}

/// `key` in the current interface language.
pub fn tr(key: T) -> &'static str {
    key.get(lang())
}

/// `key` with `{name}` placeholders filled in.
pub fn trf(key: T, args: &[(&str, &str)]) -> String {
    let mut text = tr(key).to_string();
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_thai(c: char) -> bool {
        ('\u{0E00}'..='\u{0E7F}').contains(&c)
    }

    #[test]
    fn every_string_exists_in_both_languages() {
        for key in T::ALL {
            assert!(!key.get(Lang::En).is_empty(), "{key:?} en");
            assert!(!key.get(Lang::Th).is_empty(), "{key:?} th");
        }
    }

    #[test]
    fn thai_strings_are_actually_thai() {
        // Strings that are only key names, a product name or an example may
        // stay identical; everything else must contain Thai.
        const SAME: &[T] = &[
            T::ToastSuggestAccept,
            T::AboutVersion,
            T::WelcomeExample,
            T::PaletteHead,
            T::LessonShift,
            T::BtnGitHub,
        ];
        for key in T::ALL {
            if SAME.contains(key) {
                continue;
            }
            assert!(
                key.get(Lang::Th).chars().any(is_thai),
                "{key:?} has no Thai text"
            );
        }
    }

    #[test]
    fn placeholders_match_between_languages() {
        for key in T::ALL {
            for name in ["{n}", "{k}", "{v}", "{mode}", "{app}", "{m}"] {
                assert_eq!(
                    key.get(Lang::En).contains(name),
                    key.get(Lang::Th).contains(name),
                    "{key:?} {name}"
                );
            }
        }
    }

    #[test]
    fn language_codes_round_trip_and_windows_thai_is_detected() {
        for lang in [Lang::En, Lang::Th] {
            assert_eq!(Lang::from_code(lang.code()), Some(lang));
        }
        assert_eq!(Lang::from_code("auto"), None);
        assert_eq!(Lang::from_windows_langid(0x041E), Lang::Th);
        assert_eq!(Lang::from_windows_langid(0x0409), Lang::En);
    }

    #[test]
    fn placeholders_are_filled() {
        set_lang(Lang::En);
        assert_eq!(trf(T::LearnedCount, &[("n", "12")]), "Learned words: 12");
    }
}
