// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Notification history and new-message banners on Linux, where Windows asks
//! its Notification Center. A Linux desktop has no list of other apps'
//! notifications to read, so the island listens to the calls apps make to
//! the notification service (`org.freedesktop.Notifications.Notify`) the way
//! `dbus-monitor` does: a monitor connection to the session bus, which the bus
//! allows the person's own programs. Nothing is changed or intercepted, the
//! desktop's own popup shows as it did, and what arrived while the island
//! was not running is not in the history.
//!
//! `notifications.rs` includes this file as `imp`, since it keeps its state
//! private. Opening an app from a notification goes through `media::open_player`,
//! which starts what the island has shown the person.

use std::collections::HashMap;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Manager, Runtime};
use zbus::blocking::{Connection, MessageIterator, connection::Builder};
use zbus::message::Type;
use zbus::zvariant::{OwnedValue, Value};

use super::{Access, Item, Shared};

/// What the bus is asked to hand over: the calls, and the service's replies
/// (which carry the number it gave each notification).
const RULES: [&str; 2] = [
    "type='method_call',interface='org.freedesktop.Notifications',member='Notify'",
    "type='method_return',sender='org.freedesktop.Notifications'",
];

/// The most of replies and of numbers kept to match them up.
const KEPT: usize = 512;

/// What a `Notify` call said.
#[derive(Clone, Debug, Default, PartialEq)]
struct Call {
    app: String,
    /// The number of the notification this one replaces, or 0.
    replaces: u32,
    icon: String,
    summary: String,
    body: String,
    hints: HashMap<String, HintValue>,
}

/// The few hint values that matter, read once.
#[derive(Clone, Debug, PartialEq)]
enum HintValue {
    Text(String),
    Flag(bool),
    Picture(Picture),
}

#[derive(Clone, Debug, PartialEq)]
struct Picture {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// What was seen on the bus.
#[derive(Clone, Debug, PartialEq)]
enum Observed {
    Notified { key: (String, u32), call: Call },
    Assigned { key: (String, u32), number: u32 },
}

fn text_of(value: &Value<'_>) -> Option<String> {
    match value {
        Value::Str(s) => Some(s.to_string()),
        Value::Value(inner) => text_of(inner),
        _ => None,
    }
}

fn int_of(value: &Value<'_>) -> Option<i32> {
    match value {
        Value::I32(n) => Some(*n),
        Value::Value(inner) => int_of(inner),
        _ => None,
    }
}

/// `image-data`: width, height, row stride, alpha, bits, channels, pixels.
fn picture_of(value: &Value<'_>) -> Option<Picture> {
    let Value::Structure(fields) = value else {
        return None;
    };
    let fields = fields.fields();
    let [width, height, stride, alpha, bits, channels, data] = fields else {
        return None;
    };
    let (width, height, stride) = (int_of(width)?, int_of(height)?, int_of(stride)?);
    let (bits, channels) = (int_of(bits)?, int_of(channels)?);
    let has_alpha = matches!(alpha, Value::Bool(true));
    let Value::Array(data) = data else {
        return None;
    };
    let bytes: Vec<u8> = data
        .iter()
        .filter_map(|b| if let Value::U8(b) = b { Some(*b) } else { None })
        .collect();
    // 8 bits of 3 or 4 channels, a size a notification would have, and all of it there.
    let sane = bits == 8
        && (channels == 3 || channels == 4)
        && channels == if has_alpha { 4 } else { 3 }
        && (1..=512).contains(&width)
        && (1..=512).contains(&height)
        && stride >= width * channels;
    if !sane || bytes.len() < (stride * (height - 1) + width * channels) as usize {
        return None;
    }
    let (width, height, stride, channels) = (
        width as usize,
        height as usize,
        stride as usize,
        channels as usize,
    );
    let mut rgba = Vec::with_capacity(width * height * 4);
    for row in 0..height {
        for column in 0..width {
            let at = row * stride + column * channels;
            rgba.extend_from_slice(&bytes[at..at + 3]);
            rgba.push(if channels == 4 { bytes[at + 3] } else { 255 });
        }
    }
    Some(Picture {
        width: width as u32,
        height: height as u32,
        rgba,
    })
}

fn hint(name: &str, value: &OwnedValue) -> Option<HintValue> {
    match name {
        "desktop-entry" | "image-path" | "image_path" | "x-canonical-private-synchronous" => {
            text_of(value).map(HintValue::Text)
        }
        "transient" => match &**value {
            Value::Bool(b) => Some(HintValue::Flag(*b)),
            _ => None,
        },
        "image-data" | "image_data" | "icon_data" => picture_of(value).map(HintValue::Picture),
        _ => None,
    }
}

/// The call, from a `Notify` message's body: `(susssasa{sv}i)`.
fn call_of(message: &zbus::Message) -> Option<Call> {
    let (app, replaces, icon, summary, body, _actions, hints, _timeout) = message
        .body()
        .deserialize::<(
            String,
            u32,
            String,
            String,
            String,
            Vec<String>,
            HashMap<String, OwnedValue>,
            i32,
        )>()
        .ok()?;
    Some(Call {
        app,
        replaces,
        icon,
        summary,
        body,
        hints: hints
            .iter()
            .filter_map(|(name, value)| Some((name.clone(), hint(name, value)?)))
            .collect(),
    })
}

fn observe(message: &zbus::Message) -> Option<Observed> {
    let header = message.header();
    match header.message_type() {
        Type::MethodCall if header.member().is_some_and(|m| m.as_str() == "Notify") => {
            let sender = header.sender()?.to_string();
            let serial = header.primary().serial_num().get();
            Some(Observed::Notified {
                key: (sender, serial),
                call: call_of(message)?,
            })
        }
        Type::MethodReturn => {
            let to = header.destination()?.to_string();
            let reply = header.reply_serial()?.get();
            Some(Observed::Assigned {
                key: (to, reply),
                number: message.body().deserialize::<u32>().ok()?,
            })
        }
        _ => None,
    }
}

/// Plain text from a body that may carry the markup the specification allows
/// (`<b>`, `<i>`, `<u>`, `<a href>`, `<img>`) and its entities.
fn plain(markup: &str) -> String {
    let mut out = String::with_capacity(markup.len());
    let mut chars = markup.chars().peekable();
    while let Some(c) = chars.next() {
        // A tag opens with a letter or a slash; "5 < 6" and "<3" are text.
        if c == '<'
            && chars
                .peek()
                .is_some_and(|n| n.is_ascii_alphabetic() || *n == '/')
        {
            let mut tag = String::new();
            let mut closed = false;
            for t in chars.by_ref() {
                if t == '>' {
                    closed = true;
                    break;
                }
                tag.push(t);
            }
            if !closed {
                // Not a tag after all.
                out.push('<');
                out.push_str(&tag);
            } else if tag
                .trim_start_matches('/')
                .trim()
                .to_lowercase()
                .starts_with("br")
            {
                out.push('\n');
            }
        } else {
            out.push(c);
        }
    }
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&amp;", "&")
}

/// A notification that is not for the history: one the app marks as not to
/// be kept, and the on-screen displays of volume or brightness.
fn passing(call: &Call) -> bool {
    matches!(call.hints.get("transient"), Some(HintValue::Flag(true)))
        || call.hints.contains_key("x-canonical-private-synchronous")
        || (call.summary.is_empty() && call.body.is_empty())
}

/// What the app is called to open it: its desktop entry, else its name made
/// into one word.
fn app_id(call: &Call) -> String {
    if let Some(HintValue::Text(entry)) = call.hints.get("desktop-entry")
        && !entry.trim().is_empty()
    {
        return crate::linux::desktop::entry_id(entry);
    }
    call.app
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// What the island shows beside a notification, as a data URL: the picture
/// the sender sent, else the sender's icon.
fn icon_of(call: &Call, app: &str) -> Option<String> {
    if let Some(HintValue::Picture(picture)) = call
        .hints
        .get("image-data")
        .or_else(|| call.hints.get("image_data"))
        .or_else(|| call.hints.get("icon_data"))
        && let Some(png) = crate::shell::encode_png(&picture.rgba, picture.width, picture.height)
    {
        return Some(crate::shell::png_data_url(&png));
    }
    let named = |name: &str| -> Option<String> {
        if name.starts_with('/') || name.starts_with("file://") {
            crate::linux::image::from_path_or_uri(name)
        } else if name.is_empty() {
            None
        } else {
            crate::shell::icon(&format!("icon:{name}"), 64)
        }
    };
    let path = match call
        .hints
        .get("image-path")
        .or_else(|| call.hints.get("image_path"))
    {
        Some(HintValue::Text(path)) => Some(path.as_str()),
        _ => None,
    };
    path.and_then(named)
        .or_else(|| named(&call.icon))
        .or_else(|| named(app))
}

/// Matches the calls to the numbers the service gave them, so a notification
/// an app updates (a download, a chat) is one entry and not many.
struct Tracker {
    next: u32,
    pending: HashMap<(String, u32), u32>,
    given: HashMap<u32, u32>,
}

impl Tracker {
    fn new(items: &[Item]) -> Self {
        Tracker {
            next: items
                .iter()
                .map(|i| i.id)
                .max()
                .unwrap_or(0)
                .wrapping_add(1)
                .max(1),
            pending: HashMap::new(),
            given: HashMap::new(),
        }
    }

    /// The entry this call is: the one it replaces, or a new one.
    fn notified(&mut self, key: (String, u32), replaces: u32) -> u32 {
        let id = match self.given.get(&replaces).copied().filter(|_| replaces != 0) {
            Some(existing) => existing,
            None => {
                let id = self.next;
                self.next = self.next.wrapping_add(1).max(1);
                id
            }
        };
        if self.pending.len() >= KEPT {
            self.pending.clear();
        }
        self.pending.insert(key, id);
        id
    }

    /// The service's answer: the number it gave the notification.
    fn assigned(&mut self, key: &(String, u32), number: u32) {
        if let Some(id) = self.pending.remove(key) {
            if self.given.len() >= KEPT {
                self.given.clear();
            }
            self.given.insert(number, id);
        }
    }
}

/// Turns one sighting into a change of the inbox: the item to keep and show.
fn item_of(call: &Call, id: u32) -> Item {
    let aumid = app_id(call);
    let app = if call.app.is_empty() {
        aumid.clone()
    } else {
        call.app.clone()
    };
    Item {
        id,
        icon: icon_of(call, &aumid),
        aumid,
        app,
        time: now(),
        title: plain(&call.summary),
        body: plain(&call.body),
    }
}

/// Listens until the connection ends, handing each sighting to `handle`.
fn listen(connection: &Connection, mut handle: impl FnMut(Observed)) -> zbus::Result<()> {
    connection.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus.Monitoring"),
        "BecomeMonitor",
        &(&RULES[..], 0u32),
    )?;
    for message in MessageIterator::from(connection.clone()) {
        if let Some(seen) = message.ok().as_ref().and_then(observe) {
            handle(seen);
        }
    }
    Ok(())
}

pub fn start<R: Runtime>(app: AppHandle<R>, shared: Shared) {
    {
        let mut state = shared.lock().unwrap();
        let prefs = app.state::<crate::prefs::Prefs>().get();
        let flag = |path| {
            prefs
                .pointer(path)
                .and_then(|v| v.as_bool())
                .unwrap_or(true)
        };
        state.enabled = flag("/island/enabled")
            && flag("/island/sections/notifications")
            && !prefs
                .pointer("/island/hiddenModules")
                .and_then(|v| v.as_array())
                .is_some_and(|v| v.iter().any(|id| id.as_str() == Some("notifications")));
        match app.path().app_config_dir() {
            Ok(path) => {
                let (history, loaded) = crate::notification_history::History::open(
                    path.join("notifications").join("history.json"),
                );
                match loaded {
                    Ok(document) => {
                        state.items = document.items;
                        state.seen = document.seen;
                    }
                    Err(error) => state.error = Some(error),
                }
                state.history = Some(history);
            }
            Err(_) => state.error = Some("The notification history folder is unavailable.".into()),
        }
    }
    thread::Builder::new()
        .name("notifications".into())
        .spawn(move || {
            let mut tracker = Tracker::new(&shared.lock().unwrap().items);
            loop {
                let connection = Builder::session().and_then(|b| b.build());
                let listening = connection.as_ref().is_ok_and(|connection| {
                    set_access(&app, &shared, Access::Allowed);
                    listen(connection, |seen| {
                        arrived(&app, &shared, &mut tracker, seen);
                    })
                    .is_ok()
                });
                if !listening {
                    // No bus, or one that does not let this be a monitor.
                    set_access(&app, &shared, Access::Unavailable);
                }
                thread::sleep(Duration::from_secs(5));
            }
        })
        .expect("notifications thread");
}

/// A content-free diagnostic: whether this session lets the island listen.
pub fn probe() -> serde_json::Value {
    let allowed = Builder::session()
        .and_then(|b| b.build())
        .and_then(|connection| {
            connection.call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus.Monitoring"),
                "BecomeMonitor",
                &(&RULES[..], 0u32),
            )
        })
        .is_ok();
    serde_json::json!({ "access": if allowed { "allowed" } else { "unavailable" } })
}

fn set_access<R: Runtime>(app: &AppHandle<R>, shared: &Shared, access: Access) {
    let mut state = shared.lock().unwrap();
    if state.access != access {
        state.access = access;
        let _ = app.emit("notifications://inbox", state.inbox());
    }
}

fn arrived<R: Runtime>(app: &AppHandle<R>, shared: &Shared, tracker: &mut Tracker, seen: Observed) {
    match seen {
        Observed::Assigned { key, number } => tracker.assigned(&key, number),
        Observed::Notified { key, call } => {
            if passing(&call) || !shared.lock().unwrap().enabled {
                return;
            }
            let id = tracker.notified(key, call.replaces);
            let item = item_of(&call, id);
            // Opening it later starts the app, which the island may do for what it has shown.
            crate::media::offer(&item.aumid);
            let mut state = shared.lock().unwrap();
            // Preferences can change while an icon is drawn.
            if state.enabled {
                state.add(item.clone());
                state.save();
                let _ = app.emit("notifications://new", &item);
                let _ = app.emit("notifications://inbox", state.inbox());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linux::testbus::PrivateBus;
    use std::sync::mpsc;
    use zbus::blocking::Proxy;

    fn call(app: &str, summary: &str, body: &str) -> Call {
        Call {
            app: app.into(),
            summary: summary.into(),
            body: body.into(),
            ..Default::default()
        }
    }

    #[test]
    fn markup_and_entities_become_plain_text() {
        assert_eq!(
            plain("<b>Ada</b> wrote: 1 &lt; 2 &amp; 3"),
            "Ada wrote: 1 < 2 & 3"
        );
        assert_eq!(plain("line<br/>two<br>three"), "line\ntwo\nthree");
        assert_eq!(
            plain("<a href=\"https://x.test/?a=1&amp;b=2\">link</a> it&apos;s"),
            "link it's"
        );
        assert_eq!(plain("a < b and c"), "a < b and c");
        assert_eq!(plain("5 < 6 > 4"), "5 < 6 > 4");
        assert_eq!(plain("I <3 it </b>"), "I <3 it ");
        assert_eq!(plain(""), "");
        // A stray bracket at the end is text, not a tag that eats it.
        assert_eq!(plain("x <"), "x <");
    }

    #[test]
    fn the_app_is_known_by_its_desktop_entry_else_its_name() {
        let mut c = call("Telegram Desktop", "t", "b");
        assert_eq!(app_id(&c), "telegram-desktop");
        c.hints.insert(
            "desktop-entry".into(),
            HintValue::Text("org.telegram.desktop".into()),
        );
        assert_eq!(app_id(&c), "org.telegram.desktop");
        c.hints
            .insert("desktop-entry".into(), HintValue::Text(String::new()));
        assert_eq!(app_id(&c), "telegram-desktop");
        assert_eq!(app_id(&call("", "t", "b")), "");
        assert_eq!(app_id(&call("  Éclair!  ", "t", "b")), "clair");
    }

    #[test]
    fn displays_and_transient_notifications_are_not_kept() {
        let mut c = call("Volume", "Volume 40%", "");
        assert!(!passing(&c));
        c.hints.insert(
            "x-canonical-private-synchronous".into(),
            HintValue::Text("volume".into()),
        );
        assert!(passing(&c));
        let mut t = call("App", "Heads up", "");
        t.hints.insert("transient".into(), HintValue::Flag(true));
        assert!(passing(&t));
        t.hints.insert("transient".into(), HintValue::Flag(false));
        assert!(!passing(&t));
        assert!(passing(&call("App", "", "")), "nothing to show");
    }

    #[test]
    fn a_notification_an_app_updates_is_one_entry() {
        let mut tracker = Tracker::new(&[]);
        let first = tracker.notified((":1.5".into(), 10), 0);
        tracker.assigned(&(":1.5".into(), 10), 77);
        // The same app replaces number 77: the same entry.
        assert_eq!(tracker.notified((":1.5".into(), 11), 77), first);
        // A replacement of a number never seen is a new one; so is 0.
        let other = tracker.notified((":1.5".into(), 12), 999);
        assert_ne!(other, first);
        assert_ne!(tracker.notified((":1.5".into(), 13), 0), other);
        // A reply to nothing known changes nothing.
        tracker.assigned(&(":1.9".into(), 1), 5);
        assert!(!tracker.given.contains_key(&5));
        // New ids go on from the history's.
        let mut resumed = Tracker::new(&[Item {
            id: 41,
            app: String::new(),
            aumid: String::new(),
            time: 0.0,
            title: String::new(),
            body: String::new(),
            icon: None,
        }]);
        assert_eq!(resumed.notified((":1.1".into(), 1), 0), 42);
    }

    fn picture_value(
        width: i32,
        height: i32,
        stride: i32,
        alpha: bool,
        channels: i32,
        data: Vec<u8>,
    ) -> OwnedValue {
        let structure = zbus::zvariant::StructureBuilder::new()
            .append_field(Value::from(width))
            .append_field(Value::from(height))
            .append_field(Value::from(stride))
            .append_field(Value::from(alpha))
            .append_field(Value::from(8i32))
            .append_field(Value::from(channels))
            .append_field(Value::from(data))
            .build()
            .unwrap();
        OwnedValue::try_from(Value::Structure(structure)).unwrap()
    }

    #[test]
    fn an_image_hint_becomes_rgba_rows_without_their_padding() {
        // 2x2, 3 channels, rows padded to 8 bytes.
        let data = vec![1, 2, 3, 4, 5, 6, 0, 0, 7, 8, 9, 10, 11, 12, 0, 0];
        let picture = picture_of(&picture_value(2, 2, 8, false, 3, data)).unwrap();
        assert_eq!((picture.width, picture.height), (2, 2));
        assert_eq!(
            picture.rgba,
            [1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]
        );
        // With alpha the fourth channel is kept.
        let with_alpha = picture_of(&picture_value(1, 1, 4, true, 4, vec![9, 8, 7, 6])).unwrap();
        assert_eq!(with_alpha.rgba, [9, 8, 7, 6]);
    }

    #[test]
    fn a_picture_that_does_not_add_up_is_ignored() {
        // Too little data, a huge size, alpha that disagrees with the channels, a stride below the width.
        assert!(picture_of(&picture_value(2, 2, 6, false, 3, vec![0; 5])).is_none());
        assert!(
            picture_of(&picture_value(
                100_000,
                100_000,
                400_000,
                true,
                4,
                vec![0; 4]
            ))
            .is_none()
        );
        assert!(picture_of(&picture_value(1, 1, 4, true, 3, vec![0; 4])).is_none());
        assert!(picture_of(&picture_value(2, 1, 3, false, 3, vec![0; 6])).is_none());
        assert!(picture_of(&Value::from(5i32)).is_none());
    }

    #[test]
    fn an_item_has_the_apps_name_the_plain_text_and_its_id() {
        let c = call("Mail", "<b>Ada</b>", "Meeting &amp; lunch");
        let item = item_of(&c, 7);
        assert_eq!(
            (item.id, item.app.as_str(), item.aumid.as_str()),
            (7, "Mail", "mail")
        );
        assert_eq!(
            (item.title.as_str(), item.body.as_str()),
            ("Ada", "Meeting & lunch")
        );
        assert!(item.time > 1.0e9);
        // No app name: the entry stands in.
        let mut anonymous = call("", "t", "b");
        anonymous
            .hints
            .insert("desktop-entry".into(), HintValue::Text("thing".into()));
        assert_eq!(item_of(&anonymous, 1).app, "thing");
    }

    /// A notification service that numbers what it is given from 40.
    struct Daemon(std::sync::Arc<std::sync::Mutex<u32>>);

    #[zbus::interface(name = "org.freedesktop.Notifications")]
    impl Daemon {
        #[allow(clippy::too_many_arguments)]
        fn notify(
            &self,
            _app: String,
            replaces: u32,
            _icon: String,
            _summary: String,
            _body: String,
            _actions: Vec<String>,
            _hints: HashMap<String, OwnedValue>,
            _timeout: i32,
        ) -> u32 {
            if replaces != 0 {
                return replaces;
            }
            let mut next = self.0.lock().unwrap();
            *next += 1;
            *next
        }
    }

    #[test]
    fn the_calls_to_the_service_and_its_answers_are_seen_on_a_bus() {
        let Some(bus) = PrivateBus::start() else {
            eprintln!("skipped: dbus-daemon is not installed");
            return;
        };
        let _daemon = bus.serve("org.freedesktop.Notifications", |b| {
            b.serve_at(
                "/org/freedesktop/Notifications",
                Daemon(std::sync::Arc::new(std::sync::Mutex::new(39))),
            )
            .unwrap()
        });
        let watcher = bus.connect();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = listen(&watcher, |seen| {
                let _ = tx.send(seen);
            });
        });
        std::thread::sleep(Duration::from_millis(400));
        let sender = bus.connect();
        let notify = |summary: &str, replaces: u32| -> u32 {
            let hints = HashMap::from([
                ("desktop-entry", Value::from("chat")),
                ("transient", Value::from(false)),
            ]);
            Proxy::new(
                &sender,
                "org.freedesktop.Notifications",
                "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications",
            )
            .unwrap()
            .call(
                "Notify",
                &(
                    "Chat",
                    replaces,
                    "chat-icon",
                    summary,
                    "<b>hi</b>",
                    Vec::<String>::new(),
                    hints,
                    5000i32,
                ),
            )
            .unwrap()
        };
        let number = notify("first", 0);
        assert_eq!(number, 40);
        let heard = |wait: u64| rx.recv_timeout(Duration::from_secs(wait)).ok();
        let Some(Observed::Notified { key, call }) = heard(3) else {
            panic!("the call was not seen");
        };
        assert_eq!(
            (call.app.as_str(), call.summary.as_str(), call.body.as_str()),
            ("Chat", "first", "<b>hi</b>")
        );
        assert_eq!((call.icon.as_str(), call.replaces), ("chat-icon", 0));
        assert_eq!(
            call.hints.get("desktop-entry"),
            Some(&HintValue::Text("chat".into()))
        );
        assert_eq!(call.hints.get("transient"), Some(&HintValue::Flag(false)));
        // The service's answer carries the number, for the same caller and call.
        assert_eq!(heard(3), Some(Observed::Assigned { key, number: 40 }));
        // An update of it is seen as a call that replaces 40.
        assert_eq!(notify("second", 40), 40);
        let Some(Observed::Notified { call, .. }) = heard(3) else {
            panic!("the update was not seen");
        };
        assert_eq!((call.summary.as_str(), call.replaces), ("second", 40));
    }

    #[test]
    #[ignore = "listens on this desktop's session bus and sends one notification through its own service"]
    fn a_real_notification_is_heard_on_the_desktops_own_bus() {
        let connection = Builder::session()
            .and_then(|b| b.build())
            .expect("no session bus");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = listen(&connection, |seen| {
                let _ = tx.send(seen);
            });
        });
        std::thread::sleep(Duration::from_millis(500));
        // Transient, one second: the desktop shows it briefly and keeps nothing.
        let status = std::process::Command::new("notify-send")
            .args([
                "--app-name=OpenbarTest",
                "--expire-time=1000",
                "--hint=boolean:transient:true",
            ])
            .args([
                "--icon=dialog-information",
                "Openbar test",
                "<b>heard</b> by the island",
            ])
            .status()
            .expect("notify-send");
        assert!(status.success());
        let mut notified = None;
        let mut assigned = None;
        while let Ok(seen) = rx.recv_timeout(Duration::from_secs(3)) {
            match seen {
                Observed::Notified { key, call } if call.app == "OpenbarTest" => {
                    notified = Some((key, call))
                }
                Observed::Assigned { key, number }
                    if notified.as_ref().is_some_and(|(k, _)| *k == key) =>
                {
                    assigned = Some(number);
                    break;
                }
                _ => {}
            }
        }
        let (_, call) = notified.expect("the call was not seen");
        println!("heard: {call:?}");
        println!("the service numbered it {assigned:?}");
        assert_eq!(
            (call.summary.as_str(), plain(&call.body).as_str()),
            ("Openbar test", "heard by the island")
        );
        assert!(
            passing(&call),
            "it was sent as transient, so it is not kept"
        );
        assert!(assigned.is_some(), "the service's answer was not seen");
    }
}
