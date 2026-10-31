; RightType — Inno Setup script (optional richer installer).
; Build:  ISCC packaging\RightType.iss   (requires Inno Setup 6.5+ for Thai.isl)
; Produces dist\RightType-<version>-setup.exe. build_release.ps1 passes the
; version from Cargo.toml as /DMyAppVersion=...; the fallback below is only
; for a manual ISCC run and must match Cargo.toml.

#define MyAppName "RightType"
#ifndef MyAppVersion
  #define MyAppVersion "2.6.5"
#endif
#define MyAppExe "righttype.exe"
; x64 (default) or arm64; build_release.ps1 -Arch passes it, with where the exe is.
#ifndef Arch
  #define Arch "x64"
#endif
#ifndef ExeDir
  #define ExeDir "target\release"
#endif

[Setup]
AppId={{8C6B9A2E-52C1-4E63-9B7A-7C1F4A2B9D33}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
; Setup (and the uninstaller) wait for RightType to be closed rather than kill it.
AppMutex=Local\RightType.Running
CloseApplications=no
DefaultDirName={autopf}\RightType
PrivilegesRequired=lowest
OutputDir=..\dist
#if Arch == "arm64"
OutputBaseFilename=RightType-{#MyAppVersion}-arm64-setup
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
OutputBaseFilename=RightType-{#MyAppVersion}-setup
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif
Compression=lzma2
SolidCompression=yes
UninstallDisplayIcon={app}\{#MyAppExe}
SetupIconFile=..\assets\icon.ico
; Thai on Thai Windows, English elsewhere; ask only when neither matches.
ShowLanguageDialog=auto

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
; Official Inno Setup Thai translation (by Satakun Utama), vendored unmodified
; from jrsoftware/issrc Files/Languages/Thai.isl so the build does not depend
; on which Inno Setup version the runner installs. Inno Setup licence.
Name: "th"; MessagesFile: "windows\Thai.isl"

[CustomMessages]
en.Shortcuts=Shortcuts:
th.Shortcuts=ทางลัด:
en.DesktopIcon=Create a &desktop shortcut
th.DesktopIcon=สร้างทางลัดบนเดสก์ท็อป
en.Startup=Startup:
th.Startup=เมื่อเข้าสู่ระบบ:
en.AutoStart=Start RightType when I sign in
th.AutoStart=เปิด RightType ทุกครั้งที่เข้าสู่ระบบ
en.Launch=Launch RightType
th.Launch=เปิด RightType เลย

[Files]
Source: "..\{#ExeDir}\{#MyAppExe}"; DestDir: "{app}"; Flags: ignoreversion
; The embedded interface typeface (IBM Plex Sans Thai) is SIL OFL: ship its licence.
Source: "..\assets\fonts\OFL.txt"; DestDir: "{app}"; DestName: "FONT-LICENSE-OFL.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\RightType"; Filename: "{app}\{#MyAppExe}"
Name: "{autodesktop}\RightType"; Filename: "{app}\{#MyAppExe}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "{cm:DesktopIcon}"; GroupDescription: "{cm:Shortcuts}"
Name: "autostart"; Description: "{cm:AutoStart}"; GroupDescription: "{cm:Startup}"

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; \
    ValueName: "RightType"; ValueData: "{app}\{#MyAppExe}"; Flags: uninsdeletevalue; Tasks: autostart

[Run]
Filename: "{app}\{#MyAppExe}"; Description: "{cm:Launch}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{cmd}"; Parameters: "/C taskkill /IM righttype.exe /F"; Flags: runhidden; RunOnceId: "KillApp"
