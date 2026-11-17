// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Keep Awake on Linux, where Windows holds a power request: the session asks
//! for the PC and its display not to idle to sleep in the two ways desktops
//! listen for, and holds both until it ends.
//!
//! - systemd-logind's `Inhibit`, which returns a file that, while it is open,
//!   keeps the system from idling to sleep (`systemd-inhibit` does the same),
//! - the desktop's own power manager and screensaver, which keep the display
//!   from blanking and the screen from locking while a cookie they gave is
//!   held (`org.freedesktop.ScreenSaver` and, for XFCE's power manager,
//!   `org.freedesktop.PowerManagement`).
//!
//! Like the Mac's idle assertions this does not stop a closed lid or the power
//! button from sleeping the PC. `keep_awake.rs` includes this file as `imp`,
//! for the request it builds is that module's own.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, Ordering};

use zbus::blocking::{Connection, Proxy, connection::Builder};
use zbus::zvariant::OwnedFd;

const WHO: &str = "Openbar";
const WHY: &str = "Keep awake is on";

/// Where a desktop takes a request for the display to stay on: its name, the
/// path and interface of the object, with `Inhibit` and `UnInhibit` on it.
const DESKTOPS: [(&str, &str, &str); 3] = [
    (
        "org.freedesktop.ScreenSaver",
        "/org/freedesktop/ScreenSaver",
        "org.freedesktop.ScreenSaver",
    ),
    (
        "org.freedesktop.ScreenSaver",
        "/ScreenSaver",
        "org.freedesktop.ScreenSaver",
    ),
    (
        "org.freedesktop.PowerManagement",
        "/org/freedesktop/PowerManagement/Inhibit",
        "org.freedesktop.PowerManagement.Inhibit",
    ),
];

/// What a session holds while it runs.
struct Held {
    /// The file logind watches: closing it ends the inhibitor.
    #[allow(dead_code)] // held for what dropping it does
    logind: Option<OwnedFd>,
    /// The session bus connection the cookies belong to; they end with it.
    session: Option<Connection>,
    cookies: Vec<(usize, u32)>,
}

static HELD: Mutex<Option<HashMap<isize, Held>>> = Mutex::new(None);
static NEXT: AtomicIsize = AtomicIsize::new(1);

fn logind(system: &Connection) -> zbus::Result<OwnedFd> {
    Proxy::new(
        system,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )?
    .call("Inhibit", &("idle:sleep", WHO, WHY, "block"))
}

fn cookie(session: &Connection, desktop: usize) -> zbus::Result<u32> {
    let (name, path, interface) = DESKTOPS[desktop];
    Proxy::new(session, name, path, interface)?.call("Inhibit", &(WHO, WHY))
}

/// Takes the requests the two buses will give; an error only when neither does.
fn acquire_with(system: Option<&Connection>, session: Option<&Connection>) -> Result<Held, String> {
    let logind = system.and_then(|bus| logind(bus).ok());
    let mut cookies = Vec::new();
    if let Some(bus) = session {
        // One request to each desktop service that answers, but only one to a
        // name that shows up under two paths.
        let mut asked = Vec::new();
        for (index, (name, _, _)) in DESKTOPS.iter().enumerate() {
            if asked.contains(name) {
                continue;
            }
            if let Ok(cookie) = cookie(bus, index) {
                asked.push(name);
                cookies.push((index, cookie));
            }
        }
    }
    if logind.is_none() && cookies.is_empty() {
        return Err(
            "Neither systemd-logind nor the desktop's power manager took the request".into(),
        );
    }
    Ok(Held {
        logind,
        session: session.cloned().filter(|_| !cookies.is_empty()),
        cookies,
    })
}

impl Held {
    fn release(self) {
        if let Some(bus) = &self.session {
            for (desktop, cookie) in &self.cookies {
                let (name, path, interface) = DESKTOPS[*desktop];
                if let Ok(proxy) = Proxy::new(bus, name, path, interface) {
                    let _ = proxy.call::<_, _, ()>("UnInhibit", &(*cookie,));
                }
            }
        }
        // Dropping the rest closes logind's file and the connection.
    }
}

/// A session's request, to give back with `release`.
pub fn acquire() -> Result<isize, String> {
    let system = Builder::system().and_then(|b| b.build()).ok();
    let session = Builder::session().and_then(|b| b.build()).ok();
    let held = acquire_with(system.as_ref(), session.as_ref())?;
    let id = NEXT.fetch_add(1, Ordering::SeqCst);
    HELD.lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(id, held);
    Ok(id)
}

pub fn release(id: isize) {
    let held = HELD
        .lock()
        .unwrap()
        .as_mut()
        .and_then(|held| held.remove(&id));
    if let Some(held) = held {
        held.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linux::testbus::PrivateBus;
    use std::io::Read;
    use std::os::fd::{FromRawFd, OwnedFd as StdFd};
    use std::sync::Arc;

    type Log = Arc<Mutex<Vec<String>>>;

    /// logind, which hands out the write end of a pipe the test reads.
    struct Logind(Log, Arc<Mutex<Option<StdFd>>>);

    #[zbus::interface(name = "org.freedesktop.login1.Manager")]
    impl Logind {
        fn inhibit(&self, what: String, who: String, why: String, mode: String) -> OwnedFd {
            self.0
                .lock()
                .unwrap()
                .push(format!("logind Inhibit {what} {who} {why} {mode}"));
            self.1
                .lock()
                .unwrap()
                .take()
                .expect("one inhibitor a test")
                .into()
        }
    }

    /// A desktop's inhibit service, counting its cookies from 100.
    struct Desktop(Log, &'static str);

    #[zbus::interface(name = "org.freedesktop.ScreenSaver")]
    impl Desktop {
        fn inhibit(&self, application: String, reason: String) -> u32 {
            self.0
                .lock()
                .unwrap()
                .push(format!("{} Inhibit {application} {reason}", self.1));
            100
        }
        fn un_inhibit(&self, cookie: u32) {
            self.0
                .lock()
                .unwrap()
                .push(format!("{} UnInhibit {cookie}", self.1));
        }
    }

    struct Power(Log);

    #[zbus::interface(name = "org.freedesktop.PowerManagement.Inhibit")]
    impl Power {
        fn inhibit(&self, application: String, reason: String) -> u32 {
            self.0
                .lock()
                .unwrap()
                .push(format!("power Inhibit {application} {reason}"));
            7
        }
        fn un_inhibit(&self, cookie: u32) {
            self.0
                .lock()
                .unwrap()
                .push(format!("power UnInhibit {cookie}"));
        }
    }

    /// A pipe: its read end for the test, its write end for logind to give away.
    fn pipe() -> (std::fs::File, StdFd) {
        let mut ends = [0i32; 2];
        // SAFETY: `ends` has room for the two descriptors pipe(2) writes.
        assert_eq!(unsafe { libc::pipe(ends.as_mut_ptr()) }, 0);
        // SAFETY: both descriptors are new and owned by nothing else.
        unsafe {
            (
                std::fs::File::from_raw_fd(ends[0]),
                StdFd::from_raw_fd(ends[1]),
            )
        }
    }

    #[test]
    fn a_session_asks_logind_and_the_desktop_and_gives_both_back() {
        let (Some(system), Some(session)) = (PrivateBus::start(), PrivateBus::start()) else {
            eprintln!("skipped: dbus-daemon is not installed");
            return;
        };
        let log = Log::default();
        let (mut watcher, writer) = pipe();
        let fd = Arc::new(Mutex::new(Some(writer)));
        let _logind = system.serve("org.freedesktop.login1", |b| {
            b.serve_at("/org/freedesktop/login1", Logind(log.clone(), fd.clone()))
                .unwrap()
        });
        let _saver = session.serve("org.freedesktop.ScreenSaver", |b| {
            b.serve_at(
                "/org/freedesktop/ScreenSaver",
                Desktop(log.clone(), "saver"),
            )
            .unwrap()
        });
        let _power = session.serve("org.freedesktop.PowerManagement", |b| {
            b.serve_at(
                "/org/freedesktop/PowerManagement/Inhibit",
                Power(log.clone()),
            )
            .unwrap()
        });
        let held = acquire_with(Some(&system.connect()), Some(&session.connect())).unwrap();
        // The pipe is still open: the one write end is held.
        let mut open = [0u8; 1];
        // SAFETY: a plain fcntl on a descriptor this test owns.
        unsafe {
            libc::fcntl(
                std::os::fd::AsRawFd::as_raw_fd(&watcher),
                libc::F_SETFL,
                libc::O_NONBLOCK,
            )
        };
        assert_eq!(
            watcher.read(&mut open).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        let calls = log.lock().unwrap().clone();
        assert_eq!(
            calls,
            [
                "logind Inhibit idle:sleep Openbar Keep awake is on block",
                "saver Inhibit Openbar Keep awake is on",
                "power Inhibit Openbar Keep awake is on",
            ]
        );
        held.release();
        // The cookies went back, and logind's file is closed: the pipe reads as ended.
        let calls = log.lock().unwrap().clone();
        assert_eq!(calls[3..], ["saver UnInhibit 100", "power UnInhibit 7"]);
        for _ in 0..50 {
            if watcher.read(&mut open).is_ok_and(|n| n == 0) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("logind's file was not closed");
    }

    #[test]
    fn either_request_alone_is_enough_and_neither_is_an_error() {
        let Some(session) = PrivateBus::start() else {
            eprintln!("skipped: dbus-daemon is not installed");
            return;
        };
        // No logind at all (no system bus), only a power manager.
        let log = Log::default();
        let _power = session.serve("org.freedesktop.PowerManagement", |b| {
            b.serve_at(
                "/org/freedesktop/PowerManagement/Inhibit",
                Power(log.clone()),
            )
            .unwrap()
        });
        let held = acquire_with(None, Some(&session.connect())).unwrap();
        assert!(held.logind.is_none() && held.cookies == [(2, 7)]);
        held.release();
        assert_eq!(
            log.lock().unwrap().last().map(String::as_str),
            Some("power UnInhibit 7")
        );
        // Neither: nothing keeps the PC awake, and that is said.
        let empty = PrivateBus::start().unwrap();
        assert!(acquire_with(None, Some(&empty.connect())).is_err());
        assert!(acquire_with(None, None).is_err());
    }

    #[test]
    #[ignore = "asks this desktop's logind and power manager to keep it awake for a moment"]
    fn this_desktop_takes_the_request_and_gives_it_back() {
        let list = || {
            let out = std::process::Command::new("systemd-inhibit")
                .arg("--list")
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).into_owned()
        };
        assert!(
            !list().contains(WHO),
            "an Openbar inhibitor is already held:\n{}",
            list()
        );
        let id = acquire().expect("neither logind nor a power manager took it");
        let held = list();
        println!("{held}");
        let cookies = HELD.lock().unwrap().as_ref().unwrap()[&id].cookies.clone();
        println!("the desktop's cookies: {cookies:?}");
        // logind lists what is held in its own order ("sleep:idle").
        assert!(
            held.contains("Openbar") && held.contains("sleep:idle") && held.contains("block"),
            "logind does not list it"
        );
        release(id);
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(!list().contains(WHO), "still listed after release");
    }
}
