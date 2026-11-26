// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The session being locked and unlocked on Linux, where Windows hears
//! `WTS_SESSION_LOCK` and `WTS_SESSION_UNLOCK`. There is no one signal for
//! it, so three are listened to, and the first to report a change wins:
//!
//! - systemd-logind's `Lock` and `Unlock` (and the session's `LockedHint`),
//!   which `loginctl lock-session` and the desktops' lockers send,
//! - a screensaver service's `ActiveChanged`, which GNOME, KDE, XFCE and
//!   others send,
//! - a screen locker program that starts and ends (`xsecurelock`, `i3lock`,
//!   ...), for the lockers that tell no one, which X11 users start from a key
//!   or a script.
//!
//! The lock starts the "while you were away" card's time away (#215), the
//! unlock wakes the companion and shows the card. The minutes without input
//! the card also counts come from [`idle_seconds`].

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};
use zbus::MatchRule;
use zbus::blocking::{Connection, MessageIterator, Proxy, connection::Builder};
use zbus::message::Type;
use zbus::zvariant::{ObjectPath, OwnedValue};

use crate::session_lock::{EVENT, LOCKED};

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

/// Reports of one change are one lock or unlock, however many of the
/// signals heard it.
const ONE_CHANGE: Duration = Duration::from_secs(5);

struct Notifier {
    app: AppHandle,
    last: Mutex<Option<(bool, Instant)>>,
}

impl Notifier {
    /// The session was locked (`true`) or unlocked (`false`).
    fn changed(&self, locked: bool) {
        if !self.news(locked, Instant::now()) {
            return;
        }
        let _ = self.app.emit(if locked { LOCKED } else { EVENT }, ());
    }

    fn news(&self, locked: bool, at: Instant) -> bool {
        news(&mut self.last.lock().unwrap(), locked, at)
    }
}

/// Whether a report is news: not the same change heard moments ago.
fn news(last: &mut Option<(bool, Instant)>, locked: bool, at: Instant) -> bool {
    if last
        .is_some_and(|(was, then)| was == locked && at.saturating_duration_since(then) < ONE_CHANGE)
    {
        return false;
    }
    *last = Some((locked, at));
    true
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

/// Calls `on_change` with each lock (`true`) and unlock (`false`) logind
/// reports for the session at `path`, until the connection ends.
fn watch_logind(
    connection: &Connection,
    path: &str,
    mut on_change: impl FnMut(bool),
) -> Option<()> {
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
            Some("Lock") => on_change(true),
            Some("Unlock") => on_change(false),
            Some("PropertiesChanged") => {
                let Ok((_, changed, _)) = message.body().deserialize::<(
                    String,
                    std::collections::HashMap<String, OwnedValue>,
                    Vec<String>,
                )>() else {
                    continue;
                };
                if let Some(zbus::zvariant::Value::Bool(locked)) =
                    changed.get("LockedHint").map(|v| &**v)
                {
                    on_change(*locked);
                }
            }
            _ => {}
        }
    }
    Some(())
}

/// Calls `on_change` with each `ActiveChanged` a screensaver service (of any
/// desktop) sends, until the connection ends: active is away, as a lock is.
fn watch_screensaver(connection: &Connection, mut on_change: impl FnMut(bool)) -> Option<()> {
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
        if from_a_screensaver && let Ok(active) = message.body().deserialize::<bool>() {
            on_change(active);
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

/// Seconds since the last keyboard or mouse input, for the away card's idle
/// threshold (#215), where Windows asks `GetLastInputInfo`: the X server's
/// screen saver extension on X11; on Wayland GNOME's idle monitor, else
/// logind's idle hint, which the desktops set after their own idle delay.
pub fn idle_seconds() -> Option<f64> {
    let session = || Builder::session().and_then(|b| b.build()).ok();
    let system = || Builder::system().and_then(|b| b.build()).ok();
    if !crate::linux::wm::native_wayland()
        && let Some(seconds) = x11_idle()
    {
        return Some(seconds);
    }
    session()
        .as_ref()
        .and_then(mutter_idle)
        .or_else(|| system().as_ref().and_then(logind_idle))
}

fn x11_idle() -> Option<f64> {
    use x11rb::connection::Connection as _;
    use x11rb::protocol::screensaver::ConnectionExt as _;
    let (connection, screen) = x11rb::connect(None).ok()?;
    let root = connection.setup().roots.get(screen)?.root;
    let info = connection.screensaver_query_info(root).ok()?.reply().ok()?;
    Some(f64::from(info.ms_since_user_input) / 1000.0)
}

/// GNOME's own count (Mutter), in milliseconds.
fn mutter_idle(connection: &Connection) -> Option<f64> {
    Proxy::new(
        connection,
        "org.gnome.Mutter.IdleMonitor",
        "/org/gnome/Mutter/IdleMonitor/Core",
        "org.gnome.Mutter.IdleMonitor",
    )
    .ok()?
    .call::<_, _, u64>("GetIdletime", &())
    .ok()
    .map(|ms| ms as f64 / 1000.0)
}

/// logind's hint for this session: idle since a moment (microseconds of the
/// wall clock), or not idle at all.
fn logind_idle(connection: &Connection) -> Option<f64> {
    let path = session_path(connection)?;
    let session = Proxy::new(
        connection,
        "org.freedesktop.login1",
        path,
        "org.freedesktop.login1.Session",
    )
    .ok()?;
    let idle = session.get_property::<bool>("IdleHint").ok()?;
    let since = session.get_property::<u64>("IdleSinceHint").ok()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_micros() as u64;
    Some(hint_seconds(idle, since, now))
}

fn hint_seconds(idle: bool, since_us: u64, now_us: u64) -> f64 {
    if !idle || since_us == 0 {
        return 0.0;
    }
    now_us.saturating_sub(since_us) as f64 / 1_000_000.0
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
                    watch_logind(&connection, &path, |locked| n.changed(locked));
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
                    watch_screensaver(&connection, |locked| n.changed(locked));
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
                if locked != now {
                    notifier.changed(now);
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
    fn logind_reports_locks_and_unlocks_and_the_locked_hint() {
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
            watch_logind(&listener, &session, |locked| {
                let _ = tx.send(locked);
            })
        });
        std::thread::sleep(Duration::from_millis(300));
        let signal = |member: &str| {
            logind
                .emit_signal(
                    None::<&str>,
                    path.as_str(),
                    "org.freedesktop.login1.Session",
                    member,
                    &(),
                )
                .unwrap();
        };
        signal("Lock");
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)), Ok(true));
        signal("Unlock");
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)), Ok(false));
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
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)), Ok(true));
        hint(false);
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)), Ok(false));
        // Another property changing says nothing.
        let other =
            std::collections::HashMap::from([("Active", zbus::zvariant::Value::from(true))]);
        logind
            .emit_signal(
                None::<&str>,
                path.as_str(),
                "org.freedesktop.DBus.Properties",
                "PropertiesChanged",
                &(
                    "org.freedesktop.login1.Session",
                    other,
                    Vec::<String>::new(),
                ),
            )
            .unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(400)).is_err());
    }

    #[test]
    fn a_screensaver_that_starts_is_away_and_one_that_stops_is_back() {
        let Some(bus) = PrivateBus::start() else {
            eprintln!("skipped: dbus-daemon is not installed");
            return;
        };
        let saver = bus.serve("org.freedesktop.ScreenSaver", |b| b);
        let client = bus.connect();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            watch_screensaver(&client, |locked| {
                let _ = tx.send(locked);
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
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)), Ok(true));
        active("org.freedesktop.ScreenSaver", false);
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)), Ok(false));
        // Another desktop's name for the same thing.
        active("org.gnome.ScreenSaver", false);
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)), Ok(false));
        // A signal of that name from something that is not a screensaver.
        active("org.example.Other", false);
        assert!(rx.recv_timeout(Duration::from_millis(400)).is_err());
    }

    struct IdleMonitor;

    #[zbus::interface(name = "org.gnome.Mutter.IdleMonitor")]
    impl IdleMonitor {
        fn get_idletime(&self) -> u64 {
            93_500
        }
    }

    #[derive(Default)]
    struct Session;

    #[zbus::interface(name = "org.freedesktop.login1.Session")]
    impl Session {
        #[zbus(property)]
        fn idle_hint(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn idle_since_hint(&self) -> u64 {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64
                - 120_000_000
        }
    }

    #[test]
    fn idle_time_comes_from_gnome_and_from_loginds_hint() {
        let Some(bus) = PrivateBus::start() else {
            eprintln!("skipped: dbus-daemon is not installed");
            return;
        };
        let client = bus.connect();
        assert_eq!(mutter_idle(&client), None);
        let _mutter = bus.serve("org.gnome.Mutter.IdleMonitor", |b| {
            b.serve_at("/org/gnome/Mutter/IdleMonitor/Core", IdleMonitor)
                .unwrap()
        });
        assert_eq!(mutter_idle(&client), Some(93.5));
        let _logind = bus.serve("org.freedesktop.login1", |b| {
            b.serve_at("/org/freedesktop/login1", Manager)
                .unwrap()
                .serve_at("/org/freedesktop/login1/session/_33", Session)
                .unwrap()
        });
        let idle = logind_idle(&client).unwrap();
        assert!((119.0..125.0).contains(&idle), "{idle}");
    }

    #[test]
    fn a_session_that_is_not_idle_has_no_idle_time() {
        assert_eq!(hint_seconds(false, 5_000_000, 9_000_000), 0.0);
        assert_eq!(hint_seconds(true, 0, 9_000_000), 0.0);
        assert_eq!(hint_seconds(true, 5_000_000, 9_000_000), 4.0);
        // A clock set back never makes a negative time.
        assert_eq!(hint_seconds(true, 9_000_000, 5_000_000), 0.0);
    }

    #[test]
    fn one_change_heard_by_several_signals_is_reported_once() {
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        let mut last = None;
        assert!(news(&mut last, true, at(0)));
        // logind's Lock, then its hint, then the screensaver: one lock.
        assert!(!news(&mut last, true, at(100)));
        assert!(!news(&mut last, true, at(900)));
        // The unlock right after is news, and so is a lock again.
        assert!(news(&mut last, false, at(1_000)));
        assert!(!news(&mut last, false, at(1_200)));
        assert!(news(&mut last, true, at(1_500)));
        // The same report long after is a new lock.
        assert!(news(&mut last, true, at(10_000)));
    }
}
