<div align="center">

<img src="docs/img/icon.png" width="88" alt="openRTLinux">

# openRTLinux

**พิมพ์ผิดแป้น ไทย ↔ อังกฤษ — แก้ให้ ไม่ต้องลบแล้วพิมพ์ใหม่**

`fvdw,h` → **ดอกไม้** · `แนพพำแะ` → **correct** · `l;ylfu` → **สวัสดี** · `ก` → **d**

คืออะไร · ติดตั้ง · ใช้งาน · แก้ปัญหา · สถานะวันนี้

**รองรับเฉพาะ Ubuntu** — ทดสอบบน Ubuntu 26.04 + GNOME · ดิสโทรอื่นและเดสก์ท็อปอื่นยังไม่รองรับ

</div>

---

## openRTLinux คืออะไร

โปรแกรมเล็ก ๆ ที่คอยดูว่าพิมพ์อะไรอยู่ แล้วรู้ว่า "นี่พิมพ์ผิดแป้น" — พิมพ์ไทยทั้งที่แป้นเป็นอังกฤษ
หรือกลับกัน — แล้ว **แก้คำนั้นให้ทันที** พร้อมสลับแป้นให้ตรงกับข้อความที่แก้ (จะได้พิมพ์ต่อได้เลย)

ทำงานเบื้องหลังเป็น service ของผู้ใช้ อ่านปุ่มจาก `/dev/input` โดยตรง (ใช้ได้บน Wayland)
แล้วพิมพ์ทดแทนผ่านคีย์บอร์ดเสมือน (`uinput`) — **ไม่แก้ไฟล์ของโปรแกรมอื่น ไม่ส่งข้อมูลออกเน็ต**

ใช้ได้กับ **ไทย (Kedmanee) ↔ อังกฤษ (US QWERTY)**

## ติดตั้ง

**ใช้ได้กับ Ubuntu เท่านั้น** (ทดสอบบน Ubuntu 26.04 · GNOME/Wayland)

ต้องมี: **Ubuntu** + ไดรเวอร์ `uinput`, **GNOME** (ใช้สลับแป้น), Rust ≥ 1.75, และ GTK3 + libappindicator
สำหรับไอคอนถาด (ตัวไดมอนไม่ต้องใช้ GTK)

```bash
git clone https://github.com/yumehermes8-spec/openRTLinux.git
cd openRTLinux
bash packaging/linux/install.sh --release --with-tray
```

สคริปต์จะ build แล้วติดตั้งให้เป็น *ผู้ใช้* ทั้งหมด (**ไม่ต้องใช้ sudo**):

| สิ่งที่ติดตั้ง | ที่อยู่ |
| --- | --- |
| ไดมอน | `~/.local/bin/openrtlinux` |
| หน้าตั้งค่า/ถาด | `~/.local/bin/openrtlinux-tray` |
| ตั้งค่า | `~/.config/openrtlinux/config.toml` |
| service ของผู้ใช้ | `~/.config/systemd/user/openrtlinux.service`, `openrtlinux-tray.service` |
| เมนู + ไอคอน | `~/.local/share/applications/`, `~/.local/share/icons/hicolor/` |

ตัวเลือกของสคริปต์: `--release` (build แบบ optimize) · `--no-build` · `--with-tray` · `--no-service` · `--no-extension`

## ใช้งาน

เปิด/ปิดได้จาก **ไอคอนถาด** (หรือคลิกขวา → ตั้งค่า) มี 3 โหมด

| โหมด | ทำงานเมื่อไหร่ |
| --- | --- |
| **auto** | แก้ให้เองทันทีที่เคาะวรรค/Enter — พิมพ์ผิดแล้วพิมพ์ต่อได้เลย |
| **manual** | แก้เมื่อ**กด hotkey** เท่านั้น (คุมเองทั้งหมด) |
| **off** | หยุดทำงาน (ยังดูคีย์อยู่ แต่ไม่ทำอะไร) |

### hotkey แก้คำ

เลือกได้ในหน้าตั้งค่า (dropdown) 2 แบบ

1. **Shift+Backspace** (ค่าเริ่มต้น) — พิมพ์ผิดแล้วกดทันที ตัวอักษร/คำจะถูกแก้ตรงนั้น
2. **Shift+Spacebar** — กด Shift ค้างแล้วเคาะ Space (วรรคที่เกิดขึ้นจะถูกลบให้ด้วย)

กด hotkey ได้แม้ **ตัวอักษรเดียว** — และใช้ได้ทั้งตอนพิมพ์ค้างอยู่ (mid-word) และหลังเคาะวรรคไปแล้ว

### หน้าตั้งค่า (คลิกถาด → ตั้งค่า)

| ช่อง | ความหมาย |
| --- | --- |
| โหมด | `auto` / `manual` / `off` |
| โหมดทดลอง (dry-run) | ตัดสินใจและบันทึกให้ดู แต่**ไม่พิมพ์/ไม่สลับแป้น** |
| hotkey แก้คำ | Shift+Backspace หรือ Shift+Spacebar |
| ปุ่มสลับแป้นของเดสก์ท็อป | คำสั่งอ่านคีย์สลับแป้น (ค่าเริ่มต้นดึงจาก GNOME เอง) |
| คำสั่งอ่านแป้นปัจจุบัน | คำสั่งที่ใช้ดูว่าแป้นปัจจุบันคืออะไร (gsettings) |
| เปิดถาดอัตโนมัติเมื่อ login | ใส่ autostart ให้ |

แก้แล้วกดบันทึก → ไดมอนรับค่าใหม่ทันที (ไม่ต้องรีสตาร์ท)

### ตัวอย่าง

| พิมพ์ (แป้นผิด) | ได้ |
| --- | --- |
| `fvdw,h` | ดอกไม้ |
| `dki[hko` | การบ้าน |
| `แนพพำแะ` | correct |
| `l;ylfu` | สวัสดี |
| `ก` (ต้องการ `d`) | d |
| `869` | คุต |

**แก้ได้แค่ไหน** (วัดจากพจนานุกรมทั้งสองภาษาจริง — ดู `tests/short_words.rs`)

* **1 ตัวอักษร** — แก้ให้เสมอ (อ่านอีกแป้น) ยกเว้นตัวเลข/สัญลักษณ์ที่เหมือนกันทั้งสองแป้น
* **2–3 ตัวอักษร** — กด hotkey แล้วแก้ให้ (พจนานุกรมตัดสินคำสั้นไม่ได้จริง)
* **4 ตัวขึ้นไป** — แก้เมื่ออีกแป้นอ่านแล้วเป็นคำในพจนานุกรม (แม่น ~100%) และ**ไม่แตะคำที่พิมพ์ถูกอยู่แล้ว**
* โหมด `auto` ใช้เกณฑ์พจนานุกรมเท่านั้น (เข้มกว่า) เพราะไม่มีใครสั่ง

## แก้ปัญหา

**ดูว่ากำลังทำอะไรอยู่**

```bash
journalctl --user -u openrtlinux -f          # ทุกครั้งที่แก้ จะบอก "fvdw,h -> ดอกไม้" + ใช้เวลา
openrtlinux --debug --mode manual --fix      # พิมพ์ทุกปุ่มที่เห็น (รันเองในเทอร์มินัล)
RTLINUX_TIMINGS=1 openrtlinux --debug        # แสดงเวลาที่ใช้แต่ละขั้น (สลับแป้น/ลบ/พิมพ์)
```

**ไม่แก้ให้เลย**

1. เช็คว่า service ทำงาน: `systemctl --user status openrtlinux.service`
2. เช็คโหมด: ไอคอนถาด หรือ `python3 tools/linux/rtctl.py status` (โหมด `manual` ต้องกด hotkey เอง)
3. สิทธิ์คีย์บอร์ดเสมือน: ไดมอนต้องเขียน `/dev/uinput` ได้ (`ls -l /dev/uinput` — ปกติสิทธิ์ของคนที่นั่งหน้าเครื่องใช้ได้อยู่แล้ว)
4. ถ้า log ขึ้น **"holding the modifier, not typing"** = ตอนนั้นยังกด Shift ค้างอยู่ → ปล่อย Shift แล้วกดใหม่ (ระบบเลือกที่จะไม่พิมพ์ ดีกว่าพิมพ์ออกมาเพี้ยน)

**แป้นไม่สลับตาม**

ไดมอนสลับแป้นโดย "กดปุ่มสลับแป้นของเดสก์ท็อปให้" (วิธีเขียนค่าลง gsettings ไม่มีผลบน GNOME 45+)
ถ้าแป้นไม่ยอมสลับ ให้เช็คว่าตั้งปุ่มสลับแป้นไว้: `Settings → Keyboard → View and Customize Shortcuts → Typing → Switch to next input source`

**ปิด / ถอนการติดตั้ง**

```bash
systemctl --user disable --now openrtlinux.service openrtlinux-tray.service
rm -f ~/.local/bin/openrtlinux ~/.local/bin/openrtlinux-tray
rm -rf ~/.config/openrtlinux ~/.local/share/applications/openrtlinux*.desktop
rm -f ~/.config/systemd/user/openrtlinux*.service && systemctl --user daemon-reload
rm -rf ~/.local/share/icons/hicolor/*/apps/openrtlinux*       # ไอคอน
```

## สถานะวันนี้

**ใช้ได้จริงและทดสอบแล้ว** (Ubuntu 26.04 · GNOME/Wayland · ThinkPad X13)

* แก้คำ/ตัวอักษรผิดแป้น ทั้งโหมด auto และ manual · hotkey 2 แบบ · สลับแป้นไทย↔อังกฤษให้เอง
* เร็ว: แก้หนึ่งครั้งใช้ **13–27 ms** (วัดจาก log จริง)
* **unit test 314 ตัวผ่าน** (`cargo test --features linuxos`) — เทสต์ของ upstream ไม่ถูกแตะ

**ยังไม่มี**

* ghost suggestion / palette บน Wayland (ต้องเขียน GNOME Shell extension เพิ่ม)
* กฎเฉพาะบางโปรแกรม (focused-app rules) — บน Wayland ยังไม่รู้ว่าแอปไหนโฟกัส
* **Ubuntu เท่านั้น** — ดิสโทรอื่น (Fedora, Arch, …) และเดสก์ท็อปอื่น (KDE, XFCE, …) ยังไม่รองรับ
  แม้แต่ X11 ของ GNOME เองก็ยังไม่ได้ทดสอบ (โค้ดอาจรันได้ แต่ยังไม่รับประกัน)
* แพ็กเกจ `.deb` / Flatpak (ตอนนี้ติดตั้งจาก source)

## เครดิต & สัญญาอนุญาต

เอนจินตรวจจับคำ (ส่วนที่ไม่ผูกกับระบบปฏิบัติการ) มาจาก
**[RightType](https://github.com/k-affan-th/RightType)** โดย k-affan-th — ประวัติการพัฒนาเดิม
ยังอยู่ครบใน repo นี้ merge งานใหม่ของเขาเข้ามาได้ตามปกติ

ชั้น Linux ทั้งหมด (`src/linuxos`, ไดมอน, ถาด/หน้าตั้งค่า, ตัวติดตั้ง) เขียนเพิ่มในโปรเจกต์นี้
ภายใต้สัญญาอนุญาตเดิม: **MIT / Apache-2.0** (ดู `LICENSE-MIT`, `LICENSE-APACHE`, `THIRD_PARTY.md`)
