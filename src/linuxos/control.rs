//! The control socket the tray and the settings window talk to.
//!
//! `/run/user/1000/openrtlinux.sock`, one request per line, one JSON reply per line
//! (the same shape the earlier Thai layout fixer on this host used, because the
//! GUI side is Python and JSON keeps both ends honest):
//!
//! ```text
//! status                     -> {"mode":"auto","dry_run":true,...}
//! set mode manual            -> {"ok":true,"mode":"manual"}
//! set dry_run false          -> {"ok":true,"dry_run":false}
//! reload                     -> re-read config.toml (the GUI just wrote it)
//! quit                       -> the daemon exits
//! ```
//!
//! The daemon owns the state; the socket only asks it to change.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::linuxos::config::{Config, Mode};
use crate::policy::InputLayout;

/// What the tray can see and change.
#[derive(Debug, Clone)]
pub struct Status {
    pub mode: Mode,
    pub dry_run: bool,
    pub layout: InputLayout,
    pub committed: usize,
    pub skipped: usize,
    pub last: String,
    pub backend: String,
    pub version: String,
}

#[derive(Debug)]
pub struct Shared {
    pub status: Status,
    /// The daemon re-reads config.toml when this is set.
    pub reload: bool,
    pub quit: bool,
}

pub fn socket_path() -> PathBuf {
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(runtime).join("openrtlinux.sock")
}

pub struct Control {
    pub shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
}

impl Control {
    pub fn start(initial: Status) -> std::io::Result<Control> {
        let path = socket_path();
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
        let listener = UnixListener::bind(&path)?;
        let shared = Arc::new(Mutex::new(Shared {
            status: initial,
            reload: false,
            quit: false,
        }));
        let stop = Arc::new(AtomicBool::new(false));

        let thread_shared = Arc::clone(&shared);
        let thread_stop = Arc::clone(&stop);
        thread::Builder::new()
            .name("openrtlinux-control".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    if thread_stop.load(Ordering::Relaxed) {
                        break;
                    }
                    match stream {
                        Ok(stream) => {
                            let shared = Arc::clone(&thread_shared);
                            thread::spawn(move || serve(stream, shared));
                        }
                        Err(_) => break,
                    }
                }
            })?;

        Ok(Control { shared, stop })
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = std::fs::remove_file(socket_path());
    }

    /// Read-only snapshot for the daemon's own loop.
    pub fn status(&self) -> Status {
        self.shared.lock().map(|s| s.status.clone()).unwrap_or(Status {
            mode: Mode::Auto,
            dry_run: true,
            layout: InputLayout::UsQwerty,
            committed: 0,
            skipped: 0,
            last: String::new(),
            backend: String::new(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }

    pub fn update<F: FnOnce(&mut Status)>(&self, f: F) {
        if let Ok(mut shared) = self.shared.lock() {
            f(&mut shared.status);
        }
    }

    /// True once (and only once) after the tray asked for a reload.
    pub fn take_reload(&self) -> bool {
        match self.shared.lock() {
            Ok(mut shared) if shared.reload => {
                shared.reload = false;
                true
            }
            _ => false,
        }
    }

    pub fn quitting(&self) -> bool {
        self.shared.lock().map(|s| s.quit).unwrap_or(false)
    }
}

fn serve(stream: UnixStream, shared: Arc<Mutex<Shared>>) {
    let reader = BufReader::new(match stream.try_clone() {
        Ok(clone) => clone,
        Err(_) => return,
    });
    let mut writer = stream;
    for line in reader.lines().map_while(Result::ok) {
        let request = line.trim().to_string();
        if request.is_empty() {
            continue;
        }
        let reply = handle(&request, &shared);
        if writer.write_all(reply.as_bytes()).is_err() {
            return;
        }
        if writer.write_all(b"\n").is_err() {
            return;
        }
    }
}

fn handle(request: &str, shared: &Arc<Mutex<Shared>>) -> String {
    let mut parts = request.split_whitespace();
    let command = parts.next().unwrap_or("status");
    let arg = parts.next().unwrap_or("");
    let rest: String = parts.collect::<Vec<_>>().join(" ");
    match command {
        "status" => match shared.lock() {
            Ok(shared) => status_json(&shared.status, true),
            Err(_) => "{\"error\":\"state poisoned\"}".into(),
        },
        "set" => set(&arg.to_string(), &rest, shared),
        "reload" => {
            if let Ok(mut shared) = shared.lock() {
                shared.reload = true;
            }
            "{\"ok\":true,\"reloading\":true}".into()
        }
        "quit" => {
            if let Ok(mut shared) = shared.lock() {
                shared.quit = true;
            }
            "{\"ok\":true,\"bye\":true}".into()
        }
        other => format!("{{\"error\":\"unknown command {other:?}\"}}"),
    }
}

fn set(key: &str, value: &str, shared: &Arc<Mutex<Shared>>) -> String {
    let Ok(mut shared) = shared.lock() else {
        return "{\"error\":\"state poisoned\"}".into();
    };
    match key {
        "mode" => match Mode::parse(value) {
            Some(mode) => {
                shared.status.mode = mode;
                format!("{{\"ok\":true,\"mode\":\"{}\"}}", mode.as_str())
            }
            None => "{\"error\":\"mode must be auto, manual or off\"}".into(),
        },
        "dry_run" => {
            let on = matches!(value.trim().to_lowercase().as_str(), "1" | "true" | "yes" | "on");
            shared.status.dry_run = on;
            format!("{{\"ok\":true,\"dry_run\":{on}}}")
        }
        other => format!("{{\"error\":\"cannot set {other:?}\"}}"),
    }
}

pub fn status_json(status: &Status, pretty: bool) -> String {
    let last = status.last.replace('\\', "\\\\").replace('"', "\\\"");
    let value = format!(
        "{{\"mode\":\"{}\",\"dry_run\":{},\"layout\":\"{}\",\"committed\":{},\"skipped\":{},\
         \"last\":\"{}\",\"backend\":\"{}\",\"version\":\"{}\"}}",
        status.mode.as_str(),
        status.dry_run,
        match status.layout {
            InputLayout::ThaiKedmanee => "thai",
            InputLayout::UsQwerty => "us",
        },
        status.committed,
        status.skipped,
        last,
        status.backend,
        status.version,
    );
    if pretty {
        value
    } else {
        value
    }
}

/// Load the config the daemon should run with (file first, defaults after).
pub fn config_or_default() -> Config {
    Config::load()
}
