// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The session being unlocked on Linux, where Windows hears
//! `WTS_SESSION_UNLOCK`. There is no one signal for it, so three are listened
//! to, and the first to report the unlock wins:
//!
//! - systemd-logind's `Unlock` (and the session's `LockedHint` going false),
//!   which `loginctl unlock-session` and the desktops' lockers send,
//! - a screensaver service's `ActiveChanged(false)`, which GNOME, KDE, XFCE
//!   and others send,
//! - a screen locker program that was running and is gone (`xsecurelock`,
//!   `i3lock`, ...), for the lockers that tell no one, which X11 users start
//!   from a key or a script.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};
use zbus::MatchRule;
use zbus::blocking::{Connection, MessageIterator, Proxy, connection::Builder};
use zbus::message::Type;
use zbus::zvariant::{ObjectPath, OwnedValue};

use crate::session_lock::EVENT;

/// Programs that cover the screen while it is locked and exit when it is
/// not, by `comm`. A desktop's own screensaver is not here: its daemon never
/// exits (and the kernel's cut of its name cannot tell it from its lock
/// dialog), but it tells the bus.
const LOCKERS: [&str; 8] = [
    "xsecurelock",
    "i3lock",
    "i3lock-color",
    "slock",
    "swaylock",
    "waylock",
    "physlock",
    "xtrlock",
];

/// Reports are one unlock, however many of the signals heard it.
const ONE_UNLOCK: Duration = Duration::from_secs(5);

struct Notifier {
    app: AppHandle,
    last: Mutex<Option<Instant>>,
}

impl Notifier {
    fn unlocked(&self) {
        let mut last = self.last.lock().unwrap();
        if last.is_some_and(|at| at.elapsed() < ONE_UNLOCK) {
            return;
        }
        *last = Some(Instant::now());
        let _ = self.app.emit(EVENT, ());
    }
}

/// Whether a screen locker runs, from the process list at `proc`.
fn locker_running(proc: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(proc) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let name = entry.file_name();
        if !name.to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        std::fs::read_to_string(entry.path().join("comm"))
            .is_ok_and(|comm| LOCKERS.contains(&comm.trim()))
    })
}

/// The object path of this session at logind.
fn session_path(connection: &Connection) -> Option<String> {
    let manager = Proxy::new(
        connection,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
    .ok()?;
    let path: zbus::zvariant::OwnedObjectPath = manager
        .call("GetSessionByPID", &(std::process::id(),))
        .ok()?;
    Some(path.as_str().to_string())
}

/// Calls `on_unlock` for each unlock logind reports for the session at
/// `path`, until the connection ends.
fn watch_logind(connection: &Connection, path: &str, mut on_unlock: impl FnMut()) -> Option<()> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender("org.freedesktop.login1")
        .ok()?
        .path(ObjectPath::try_from(path).ok()?)
        .ok()?
        .build();
    for message in MessageIterator::for_match_rule(rule, connection, None).ok()? {
        let Ok(message) = message else { continue };
        let header = message.header();
        match header.member().map(|m| m.as_str()) {
            Some("Unlock") => on_unlock(),
            Some("PropertiesChanged") => {
                let Ok((_, changed, _)) = message.body().deserialize::<(
                    String,
                    std::collections::HashMap<String, OwnedValue>,
                    Vec<String>,
                )>() else {
                    continue;
                };
                if changed
                    .get("LockedHint")
                    .is_some_and(|v| matches!(&**v, zbus::zvariant::Value::Bool(false)))
                {
                    on_unlock();
                }
            }
            _ => {}
        }
    }
    Some(())
}

/// Calls `on_unlock` for each `ActiveChanged(false)` a screensaver service
/// (of any desktop) sends, until the connection ends.
fn watch_screensaver(connection: &Connection, mut on_unlock: impl FnMut()) -> Option<()> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .member("ActiveChanged")
        .ok()?
        .build();
    for message in MessageIterator::for_match_rule(rule, connection, None).ok()? {
        let Ok(message) = message else { continue };
        let header = message.header();
        let from_a_screensaver = header
            .interface()
            .is_some_and(|i| i.as_str().to_lowercase().contains("screensaver"));
        if from_a_screensaver
            && message
                .body()
                .deserialize::<bool>()
                .is_ok_and(|active| !active)
        {
            on_unlock();
        }
    }
    Some(())
}

/// Whether the screen is locked now (#231): logind's `LockedHint` for this
/// session, a screensaver service that says it is active, or a locker
/// program running. Any of them is enough.
pub fn locked_now() -> bool {
    let system = Builder::system().and_then(|b| b.build()).ok();
    let session = Builder::session().and_then(|b| b.build()).ok();
    system.as_ref().is_some_and(logind_locked)
        || session.as_ref().is_some_and(screensaver_active)
        || locker_running(Path::new("/proc"))
}

fn logind_locked(connection: &Connection) -> bool {
    let Some(path) = session_path(connection) else {
        return false;
    };
    Proxy::new(
        connection,
        "org.freedesktop.login1",
        path,
        "org.freedesktop.login1.Session",
    )
    .and_then(|session| session.get_property::<bool>("LockedHint"))
    .unwrap_or(false)
}

/// The screensaver services the desktops run, any of which may answer.
const SCREENSAVERS: [(&str, &str, &str); 4] = [
    (
        "org.freedesktop.ScreenSaver",
        "/org/freedesktop/ScreenSaver",
        "org.freedesktop.ScreenSaver",
    ),
    (
        "org.gnome.ScreenSaver",
        "/org/gnome/ScreenSaver",
        "org.gnome.ScreenSaver",
    ),
    (
        "org.cinnamon.ScreenSaver",
        "/org/cinnamon/ScreenSaver",
        "org.cinnamon.ScreenSaver",
    ),
    (
        "org.mate.ScreenSaver",
        "/org/mate/ScreenSaver",
        "org.mate.ScreenSaver",
    ),
];

fn screensaver_active(connection: &Connection) -> bool {
    SCREENSAVERS.iter().any(|(name, path, interface)| {
        Proxy::new(connection, *name, *path, *interface)
            .and_then(|proxy| proxy.call::<_, _, bool>("GetActive", &()))
            .unwrap_or(false)
    })
}

/// Whether Do Not Disturb is on (#231), from what the desktop exposes:
/// the notification server's `Inhibited` (KDE Plasma and others), then the
/// desktops' own settings (GNOME, Cinnamon, MATE, XFCE). None when nothing
/// tells.
pub fn do_not_disturb() -> Option<bool> {
    let inhibited = Builder::session()
        .and_then(|b| b.build())
        .ok()
        .and_then(|connection| notifications_inhibited(&connection));
    if inhibited == Some(true) {
        return Some(true);
    }
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    desktop_dnd(&desktop, read_setting).or(inhibited)
}

fn notifications_inhibited(connection: &Connection) -> Option<bool> {
    Proxy::new(
        connection,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )
    .ok()?
    .get_property::<bool>("Inhibited")
    .ok()
}

/// A desktop setting through its own tool: `gsettings get` or `xfconf-query`.
fn read_setting(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Do Not Disturb by the desktop named in `XDG_CURRENT_DESKTOP`, read with
/// `read`; None for a desktop that keeps no such setting, or can't be read.
fn desktop_dnd(desktop: &str, read: impl Fn(&str, &[&str]) -> Option<String>) -> Option<bool> {
    let desktop = desktop.to_ascii_lowercase();
    let has = |name: &str| desktop.split(':').any(|part| part == name);
    // A banner switch: false is Do Not Disturb.
    let banners_off = |schema: &str, key: &str| {
        read("gsettings", &["get", schema, key]).map(|value| value == "false")
    };
    if has("gnome") || has("ubuntu") || has("unity") || has("pop") || has("budgie") {
        return banners_off("org.gnome.desktop.notifications", "show-banners");
    }
    if has("x-cinnamon") || has("cinnamon") {
        return banners_off(
            "org.cinnamon.desktop.notifications",
            "display-notifications",
        );
    }
    if has("mate") {
        return read(
            "gsettings",
            &["get", "org.mate.NotificationDaemon", "do-not-disturb"],
        )
        .map(|value| value == "true");
    }
    if has("xfce") {
        return read(
            "xfconf-query",
            &["-c", "xfce4-notifyd", "-p", "/do-not-disturb"],
        )
        .map(|value| value == "true");
    }
    None
}

pub fn start(app: AppHandle) {
    let notifier = Arc::new(Notifier {
        app,
        last: Mutex::new(None),
    });
    let spawn = |name: &str, job: Box<dyn FnOnce() + Send>| {
        let _ = std::thread::Builder::new().name(name.into()).spawn(job);
    };
    let n = notifier.clone();
    spawn(
        "session-lock-logind",
        Box::new(move || {
            loop {
                if let Ok(connection) = Builder::system().and_then(|b| b.build())
                    && let Some(path) = session_path(&connection)
                {
                    watch_logind(&connection, &path, || n.unlocked());
                }
                std::thread::sleep(Duration::from_secs(30));
            }
        }),
    );
    let n = notifier.clone();
    spawn(
        "session-lock-screensaver",
        Box::new(move || {
            loop {
                if let Ok(connection) = Builder::session().and_then(|b| b.build()) {
                    watch_screensaver(&connection, || n.unlocked());
                }
                std::thread::sleep(Duration::from_secs(30));
            }
        }),
    );
    spawn(
        "session-lock-locker",
        Box::new(move || {
            let proc = Path::new("/proc");
            let mut locked = locker_running(proc);
            loop {
                std::thread::sleep(Duration::from_secs(2));
                let now = locker_running(proc);
                if locked && !now {
                    notifier.unlocked();
                }
                locked = now;
            }
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linux::testbus::PrivateBus;
    use std::sync::mpsc;

    fn process(proc: &Path, pid: &str, comm: &str) {
        let dir = proc.join(pid);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("comm"), format!("{comm}\n")).unwrap();
    }

    #[test]
    fn a_screen_locker_in_the_process_list_is_found_by_its_name() {
        let proc = std::env::temp_dir().join(format!("openbar-proc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&proc);
        std::fs::create_dir_all(&proc).unwrap();
        process(&proc, "100", "bash");
        process(&proc, "101", "xss-lock");
        // Not a process: a folder that is not a number.
        process(&proc, "self", "xsecurelock");
        assert!(!locker_running(&proc));
        process(&proc, "200", "xsecurelock");
        assert!(locker_running(&proc));
        std::fs::remove_dir_all(proc.join("200")).unwrap();
        assert!(!locker_running(&proc));
        // A desktop's screensaver daemon is not a locker.
        process(&proc, "300", "xfce4-screensav");
        assert!(!locker_running(&proc));
        assert!(!locker_running(Path::new("/nonexistent")));
        std::fs::remove_dir_all(proc).unwrap();
    }

    #[test]
    fn do_not_disturb_is_read_from_each_desktop_setting() {
        let reads = |answer: &str| {
            let answer = answer.to_string();
            move |program: &str, args: &[&str]| -> Option<String> {
                let key = format!("{program} {}", args.join(" "));
                answer
                    .split(';')
                    .find_map(|pair| pair.split_once('=').filter(|(k, _)| *k == key))
                    .map(|(_, v)| v.to_string())
            }
        };
        let gnome = "gsettings get org.gnome.desktop.notifications show-banners";
        assert_eq!(
            desktop_dnd("ubuntu:GNOME", reads(&format!("{gnome}=false"))),
            Some(true)
        );
        assert_eq!(
            desktop_dnd("GNOME", reads(&format!("{gnome}=true"))),
            Some(false)
        );
        assert_eq!(
            desktop_dnd(
                "X-Cinnamon",
                reads(
                    "gsettings get org.cinnamon.desktop.notifications display-notifications=false"
                )
            ),
            Some(true)
        );
        assert_eq!(
            desktop_dnd(
                "MATE",
                reads("gsettings get org.mate.NotificationDaemon do-not-disturb=true")
            ),
            Some(true)
        );
        assert_eq!(
            desktop_dnd(
                "XFCE",
                reads("xfconf-query -c xfce4-notifyd -p /do-not-disturb=false")
            ),
            Some(false)
        );
        // A setting that can't be read, or a desktop with none, says nothing.
        assert_eq!(desktop_dnd("GNOME", reads("")), None);
        assert_eq!(desktop_dnd("sway", reads("")), None);
        assert_eq!(desktop_dnd("", reads("")), None);
    }

    struct Notifications {
        inhibited: bool,
    }

    #[zbus::interface(name = "org.freedesktop.Notifications")]
    impl Notifications {
        #[zbus(property)]
        fn inhibited(&self) -> bool {
            self.inhibited
        }
    }

    struct ScreenSaver;

    #[zbus::interface(name = "org.gnome.ScreenSaver")]
    impl ScreenSaver {
        fn get_active(&self) -> bool {
            true
        }
    }

    #[test]
    fn the_notification_server_and_a_screensaver_are_asked() {
        let Some(bus) = PrivateBus::start() else {
            eprintln!("skipped: dbus-daemon is not installed");
            return;
        };
        let client = bus.connect();
        assert_eq!(notifications_inhibited(&client), None);
        assert!(!screensaver_active(&client));
        let _notifications = bus.serve("org.freedesktop.Notifications", |b| {
            b.serve_at(
                "/org/freedesktop/Notifications",
                Notifications { inhibited: true },
            )
            .unwrap()
        });
        assert_eq!(notifications_inhibited(&client), Some(true));
        let _saver = bus.serve("org.gnome.ScreenSaver", |b| {
            b.serve_at("/org/gnome/ScreenSaver", ScreenSaver).unwrap()
        });
        assert!(screensaver_active(&client));
    }

    struct Manager;

    #[zbus::interface(name = "org.freedesktop.login1.Manager")]
    impl Manager {
        #[zbus(name = "GetSessionByPID")]
        fn get_session_by_pid(&self, _pid: u32) -> zbus::zvariant::OwnedObjectPath {
            zbus::zvariant::OwnedObjectPath::try_from("/org/freedesktop/login1/session/_33")
                .unwrap()
        }
    }

    #[test]
    fn logind_reports_an_unlock_and_a_locked_hint_that_goes_false() {
        let Some(bus) = PrivateBus::start() else {
            eprintln!("skipped: dbus-daemon is not installed");
            return;
        };
        let logind = bus.serve("org.freedesktop.login1", |b| {
            b.serve_at("/org/freedesktop/login1", Manager).unwrap()
        });
        let client = bus.connect();
        let path = session_path(&client).unwrap();
        assert_eq!(path, "/org/freedesktop/login1/session/_33");
        let (tx, rx) = mpsc::channel();
        let listener = client.clone();
        let session = path.clone();
        std::thread::spawn(move || {
            watch_logind(&listener, &session, || {
                let _ = tx.send(());
            })
        });
        std::thread::sleep(Duration::from_millis(300));
        let emit =
            |interface: &str, member: &str, body: &(dyn zbus::zvariant::DynamicType + Sync)| {
                let _ = body;
                (interface.to_string(), member.to_string())
            };
        let _ = emit;
        // Lock: not news. Unlock: news.
        logind
            .emit_signal(
                None::<&str>,
                path.as_str(),
                "org.freedesktop.login1.Session",
                "Lock",
                &(),
            )
            .unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(400)).is_err());
        logind
            .emit_signal(
                None::<&str>,
                path.as_str(),
                "org.freedesktop.login1.Session",
                "Unlock",
                &(),
            )
            .unwrap();
        assert!(rx.recv_timeout(Duration::from_secs(2)).is_ok());
        // The hint going true is the lock, false is the unlock.
        let hint = |on: bool| {
            let changed =
                std::collections::HashMap::from([("LockedHint", zbus::zvariant::Value::from(on))]);
            logind
                .emit_signal(
                    None::<&str>,
                    path.as_str(),
                    "org.freedesktop.DBus.Properties",
                    "PropertiesChanged",
                    &(
                        "org.freedesktop.login1.Session",
                        changed,
                        Vec::<String>::new(),
                    ),
                )
                .unwrap();
        };
        hint(true);
        assert!(rx.recv_timeout(Duration::from_millis(400)).is_err());
        hint(false);
        assert!(rx.recv_timeout(Duration::from_secs(2)).is_ok());
    }

    #[test]
    fn a_screensaver_that_stops_is_an_unlock_and_one_that_starts_is_not() {
        let Some(bus) = PrivateBus::start() else {
            eprintln!("skipped: dbus-daemon is not installed");
            return;
        };
        let saver = bus.serve("org.freedesktop.ScreenSaver", |b| b);
        let client = bus.connect();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            watch_screensaver(&client, || {
                let _ = tx.send(());
            })
        });
        std::thread::sleep(Duration::from_millis(300));
        let active = |interface: &str, on: bool| {
            saver
                .emit_signal(
                    None::<&str>,
                    "/org/freedesktop/ScreenSaver",
                    interface,
                    "ActiveChanged",
                    &on,
                )
                .unwrap();
        };
        active("org.freedesktop.ScreenSaver", true);
        assert!(rx.recv_timeout(Duration::from_millis(400)).is_err());
        active("org.freedesktop.ScreenSaver", false);
        assert!(rx.recv_timeout(Duration::from_secs(2)).is_ok());
        // Another desktop's name for the same thing.
        active("org.gnome.ScreenSaver", false);
        assert!(rx.recv_timeout(Duration::from_secs(2)).is_ok());
        // A signal of that name from something that is not a screensaver.
        active("org.example.Other", false);
        assert!(rx.recv_timeout(Duration::from_millis(400)).is_err());
    }
}
