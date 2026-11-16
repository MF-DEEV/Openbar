// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Other apps' windows on KDE Plasma's Wayland session (#252). KWin has no
//! socket to ask, but it runs scripts: a small one is loaded over D-Bus
//! (`org.kde.KWin /Scripting`) that sends the window list, which one is
//! active, which are fullscreen and the active display to an object of ours
//! on the session bus whenever any of that changes. Raising a window is a
//! one-line script of its own. Both Plasma 5's and Plasma 6's script API are
//! spoken.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;
use zbus::blocking::{Connection, Proxy, connection::Builder};

use super::wm::{Output, Toplevel};

const PLUGIN: &str = "openbar-windows";
const RAISE_PLUGIN: &str = "openbar-raise";
const INTERFACE: &str = "com.openbar.island.KWin";
const PATH: &str = "/com/openbar/island/KWin";

/// What the script said last.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub windows: Vec<Toplevel>,
    pub screen: Option<Output>,
    /// KWin's id of each window, by the id the rest of the code knows it by.
    pub uuids: HashMap<u64, String>,
}

static LAST: Mutex<Option<Report>> = Mutex::new(None);

/// A window's id as a number: the same for the same KWin id while the app runs.
fn number(uuid: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    uuid.hash(&mut hasher);
    // Kept to 48 bits, so it fits the isize a paste target is passed around in.
    hasher.finish() & 0xffff_ffff_ffff
}

/// The script's report, least recently active windows first as KWin's
/// stacking order runs from the bottom, the active one last.
pub fn parse(json: &str) -> Option<Report> {
    let value: Value = serde_json::from_str(json).ok()?;
    let mut report = Report::default();
    for w in value.get("windows")?.as_array()? {
        let Some(uuid) = w.get("id").and_then(Value::as_str) else {
            continue;
        };
        let n = |key: &str| w.get(key).and_then(Value::as_f64).unwrap_or(0.0).round() as i32;
        let id = number(uuid);
        report.uuids.insert(id, uuid.to_string());
        let (x, y) = (n("x"), n("y"));
        report.windows.push(Toplevel {
            id,
            pid: w
                .get("pid")
                .and_then(Value::as_u64)
                .and_then(|p| u32::try_from(p).ok())
                .unwrap_or(0),
            title: w.get("title").and_then(Value::as_str).unwrap_or("").into(),
            app_id: w.get("app").and_then(Value::as_str).unwrap_or("").into(),
            focused: w.get("focused").and_then(Value::as_bool) == Some(true),
            fullscreen: w.get("fullscreen").and_then(Value::as_bool) == Some(true),
            rect: [x, y, x + n("w"), y + n("h")],
        });
    }
    report.windows.sort_by_key(|t| t.focused);
    report.screen = value.get("screen").map(|s| {
        let n = |key: &str| s.get(key).and_then(Value::as_f64).unwrap_or(0.0).round() as i32;
        Output {
            index: 0,
            focused: true,
            x: n("x"),
            y: n("y"),
            width: n("w"),
            height: n("h"),
        }
    });
    Some(report)
}

/// The script that keeps us told, sending to `service`.
fn script(service: &str) -> String {
    format!(
        r#"// Openbar: the window list, sent whenever it changes.
const ws = workspace;
const plasma6 = typeof ws.windowList === "function";
function windows() {{ return plasma6 ? ws.windowList() : ws.clientList(); }}
function active() {{ return plasma6 ? ws.activeWindow : ws.activeClient; }}
function area(w) {{ return w.frameGeometry || w.geometry || {{x: w.x, y: w.y, width: w.width, height: w.height}}; }}
function screen() {{
    if (plasma6) {{ const g = ws.activeScreen.geometry; return {{x: g.x, y: g.y, w: g.width, h: g.height}}; }}
    const g = ws.clientArea(KWin.ScreenArea, ws.activeScreen, ws.currentDesktop);
    return {{x: g.x, y: g.y, w: g.width, h: g.height}};
}}
function report() {{
    const now = active();
    const list = [];
    for (const w of windows()) {{
        if (!(w.normalWindow || w.dialog)) continue;
        const g = area(w);
        list.push({{id: String(w.internalId), pid: w.pid, title: String(w.caption),
            app: String(w.resourceClass), focused: w === now, fullscreen: !!w.fullScreen,
            x: g.x, y: g.y, w: g.width, h: g.height}});
    }}
    callDBus("{service}", "{PATH}", "{INTERFACE}", "Report", JSON.stringify({{windows: list, screen: screen()}}));
}}
function watch(w) {{
    if (!w) return;
    if (w.fullScreenChanged) w.fullScreenChanged.connect(report);
    if (w.captionChanged) w.captionChanged.connect(report);
}}
for (const w of windows()) watch(w);
if (plasma6) {{
    ws.windowActivated.connect(report);
    ws.windowAdded.connect(function (w) {{ watch(w); report(); }});
    ws.windowRemoved.connect(report);
    ws.screensChanged.connect(report);
}} else {{
    ws.clientActivated.connect(report);
    ws.clientAdded.connect(function (w) {{ watch(w); report(); }});
    ws.clientRemoved.connect(report);
    ws.numberScreensChanged.connect(report);
}}
report();
"#
    )
}

/// The script that activates the window KWin knows as `uuid`.
fn raise_script(uuid: &str) -> String {
    let uuid = serde_json::to_string(uuid).unwrap_or_default();
    format!(
        r#"// Openbar: bring one window forward.
const plasma6 = typeof workspace.windowList === "function";
for (const w of (plasma6 ? workspace.windowList() : workspace.clientList())) {{
    if (String(w.internalId) === {uuid}) {{
        if (plasma6) workspace.activeWindow = w; else workspace.activeClient = w;
    }}
}}
"#
    )
}

struct Receiver;

#[zbus::interface(name = "com.openbar.island.KWin")]
impl Receiver {
    fn report(&self, json: String) {
        if let Some(report) = parse(&json) {
            *LAST.lock().unwrap() = Some(report);
        }
    }
}

struct Session {
    /// Kept so our object stays on the bus.
    _bus: Connection,
}

fn scripting(bus: &Connection) -> Option<Proxy<'static>> {
    Proxy::new(bus, "org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting").ok()
}

/// Loads `source` as KWin script `plugin`, in place of one by that name, and starts it.
fn run_script(bus: &Connection, plugin: &str, source: &str) -> bool {
    let Some(scripting) = scripting(bus) else {
        return false;
    };
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let file = dir.join(format!("{plugin}-{}.js", std::process::id()));
    if std::fs::write(&file, source).is_err() {
        return false;
    }
    let _: Result<bool, _> = scripting.call("unloadScript", &(plugin,));
    let loaded: Result<i32, _> =
        scripting.call("loadScript", &(file.to_string_lossy().as_ref(), plugin));
    let started: Result<(), _> = scripting.call("start", &());
    loaded.is_ok_and(|id| id >= 0) && started.is_ok()
}

fn start() -> Option<Session> {
    let service = format!("com.openbar.island.kwin.p{}", std::process::id());
    let bus = Builder::session()
        .ok()?
        .name(service.as_str())
        .ok()?
        .serve_at(PATH, Receiver)
        .ok()?
        .build()
        .ok()?;
    if !run_script(&bus, PLUGIN, &script(&service)) {
        return None;
    }
    Some(Session { _bus: bus })
}

fn session() -> Option<&'static Session> {
    static SESSION: OnceLock<Option<Session>> = OnceLock::new();
    SESSION.get_or_init(start).as_ref()
}

/// Whether this is Plasma on Wayland with a KWin that runs our script.
pub fn available() -> bool {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    desktop.split(':').any(|d| d.eq_ignore_ascii_case("KDE")) && session().is_some()
}

/// What the script said last, waiting a moment for its first report.
pub fn report() -> Option<Report> {
    session()?;
    let deadline = Instant::now() + Duration::from_millis(500);
    loop {
        if let Some(report) = LAST.lock().unwrap().clone() {
            return Some(report);
        }
        if Instant::now() > deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Asks KWin to activate the window known here as `id`.
pub fn activate(id: u64) -> bool {
    let Some(uuid) = report().and_then(|r| r.uuids.get(&id).cloned()) else {
        return false;
    };
    let Ok(bus) = Connection::session() else {
        return false;
    };
    run_script(&bus, RAISE_PLUGIN, &raise_script(&uuid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_report_gives_windows_active_last_and_the_active_display() {
        let json = r#"{"windows":[
            {"id":"{aaa}","pid":40,"title":"~/work","app":"org.kde.konsole","focused":true,"fullscreen":false,"x":0,"y":0,"w":800,"h":600.4},
            {"id":"{bbb}","pid":50,"title":"video","app":"mpv","focused":false,"fullscreen":true,"x":10,"y":20,"w":1920,"h":1080},
            {"pid":60}
        ],"screen":{"x":1920,"y":0,"w":2560,"h":1440}}"#;
        let report = parse(json).unwrap();
        assert_eq!(
            report.windows.len(),
            2,
            "a window without an id is left out"
        );
        assert_eq!(report.windows[1].pid, 40, "the active window last");
        assert!(report.windows[1].focused && !report.windows[1].fullscreen);
        assert!(report.windows[0].fullscreen);
        assert_eq!(report.windows[0].rect, [10, 20, 1930, 1100]);
        assert_eq!(report.windows[1].rect, [0, 0, 800, 600]);
        assert_eq!(report.uuids[&report.windows[0].id], "{bbb}");
        assert_eq!(number("{aaa}"), number("{aaa}"));
        assert!(number("{aaa}") <= 0xffff_ffff_ffff);
        let screen = report.screen.unwrap();
        assert_eq!((screen.x, screen.width, screen.height), (1920, 2560, 1440));
        assert!(parse("{}").is_none());
        assert!(parse("not json").is_none());
    }

    #[test]
    fn the_scripts_name_our_service_and_quote_the_window() {
        let watching = script("com.openbar.island.kwin.p7");
        assert!(watching.contains(r#"callDBus("com.openbar.island.kwin.p7", "/com/openbar/island/KWin", "com.openbar.island.KWin", "Report""#));
        let raising = raise_script(r#"{x"y}"#);
        assert!(
            raising.contains(r#"=== "{x\"y}""#),
            "the id is a JS string, quotes and all"
        );
    }
}
