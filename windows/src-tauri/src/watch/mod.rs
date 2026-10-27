// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Watch: any part of any window as a live activity (spec 04 §6;
//! `NotchWatchService.swift`). The person drags an area with the capture
//! overlay; the window under its middle becomes the target, followed
//! wherever it goes (covered, on another desktop), with the area kept in
//! points from its top-left corner. Over the desktop the target is that
//! part of the display instead, with the island kept out of the capture.
//!
//! A thread per watch reads the area every second while its page is open
//! and every two otherwise ([`platform::Source`]: one Windows.Graphics.Capture
//! session, `PrintWindow`/display snapshot where that can't run). A picture
//! that didn't move skips text recognition for up to 10 s; every reading
//! feeds the [`support::Tracker`], and once the rule is met the watch ends
//! with the sound, a `watch://finished` event for the island's notice, and
//! the page's outcome. `watch://state` carries everything the page and the
//! closed island show.

mod platform;

/// `Vorssaint.exe --watch-probe`: times the capture steps for a window (see platform::probe).
#[cfg(windows)]
pub fn probe(path: &str, title: &str) {
    platform::probe(path, title);
}
pub mod support;

use std::sync::{Condvar, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use self::platform::Grab;
use self::support::{Area, Condition, Outcome, Tracker};
use crate::capture::snapshot::Frame;

/// The area's picture in the page and the closed island, at most this long a side.
const PREVIEW_SIDE: u32 = 640;

/// Where the area is read.
#[derive(Clone, Debug, PartialEq)]
enum Kind {
    /// `crop` in points from the window's top-left corner (its extended frame).
    Window { hwnd: isize, crop: Area },
    /// `crop` in pixels from the display's top-left corner; `monitor` in desktop pixels.
    Display { monitor: crate::capture::Rect, crop: Area },
}

#[derive(Clone, Debug, PartialEq)]
struct Target {
    kind: Kind,
    /// Empty for a part of the display: the page names it Watch.
    app_name: String,
    window_title: Option<String>,
    icon: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
enum Phase {
    #[default]
    Idle,
    Watching,
    /// The window is there but can't be read now (minimized, no picture).
    Hidden,
    Finished(Outcome),
}

#[derive(Default)]
pub struct Shared {
    inner: Mutex<Inner>,
    /// Wakes the reading thread: stopped, a new watch, or the page opened.
    wake: Condvar,
}

#[derive(Default)]
struct Inner {
    target: Option<Target>,
    phase: Phase,
    preview: Option<String>,
    text: String,
    headline: String,
    /// The line chosen for the island; None picks one automatically.
    headline_line: Option<usize>,
    started_at: Option<u64>,
    /// Set from the page; None follows the saved preference.
    condition: Option<Condition>,
    match_text: String,
    match_number: Option<f64>,
    /// Windows refused the capture; the area stays hidden until it reads again.
    access_denied: bool,
    page_visible: bool,
    /// Bumped by every start and end: a reading of an older watch is dropped.
    generation: u64,
    /// Bumped by every choice and stop: a selection that ends late is dropped.
    choice: u64,
    choosing: bool,
    /// The page opened: read now rather than at the next tick.
    poke: bool,
    tracker: Option<Tracker>,
    fingerprint: Option<Vec<u8>>,
    signature: Option<String>,
    read_at: Option<Instant>,
    /// How the person's region writes numbers, read at each start.
    decimal: String,
    groups_with_space: bool,
    published: String,
}

impl Inner {
    fn is_active(&self) -> bool {
        self.target.is_some() && matches!(self.phase, Phase::Watching | Phase::Hidden)
    }

    fn headline_for(&self, text: &str) -> String {
        support::headline_for_line(text, self.headline_line, self.groups_with_space)
    }
}

/// What the page and the closed island draw.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    phase: &'static str,
    outcome: Option<Outcome>,
    target: Option<TargetInfo>,
    /// A PNG data URL of the area as last read.
    preview: Option<String>,
    text: String,
    headline: String,
    headline_line: Option<usize>,
    /// The lines a person can pin, top to bottom.
    lines: Vec<String>,
    /// Epoch ms.
    started_at: Option<u64>,
    condition: Condition,
    match_text: String,
    match_number: Option<f64>,
    can_watch_again: bool,
    access_denied: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct TargetInfo {
    app_name: String,
    window_title: Option<String>,
    icon: Option<String>,
    is_window: bool,
}

/// Raised once when the rule is met or the window closes.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Finished {
    outcome: Outcome,
    app_name: String,
    window_title: Option<String>,
}

// MARK: Preferences

fn island_pref<R: Runtime>(app: &AppHandle<R>, key: &str) -> Option<Value> {
    app.try_state::<crate::prefs::Prefs>()?.get().pointer(&format!("/island/{key}")).cloned()
}

fn flag<R: Runtime>(app: &AppHandle<R>, key: &str, default: bool) -> bool {
    island_pref(app, key).and_then(|v| v.as_bool()).unwrap_or(default)
}

/// The island is on and Watch is one of its sections (NotchWatchSupport.isEnabled).
fn enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    let hidden = island_pref(app, "hiddenModules")
        .and_then(|v| v.as_array().cloned())
        .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some("watch")));
    flag(app, "enabled", true) && island_pref(app, "sections/watch").and_then(|v| v.as_bool()) != Some(false) && !hidden
}

fn condition<R: Runtime>(app: &AppHandle<R>, inner: &Inner) -> Condition {
    inner.condition.unwrap_or_else(|| {
        island_pref(app, "watchCondition").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
    })
}

/// Settings changed: a section switched off ends the watch, and a part of
/// the display keeps the island out of what it reads, whatever Settings ›
/// Behavior › Show in screenshots says.
pub fn apply_prefs<R: Runtime>(app: &AppHandle<R>) {
    let Some(shared) = app.try_state::<Shared>() else { return };
    let watching = shared.inner.lock().unwrap().target.is_some();
    if watching && !enabled(app) {
        stop(app);
    }
    sync_island_capture(app);
}

/// Beside the camera the island would read itself opening and closing as a
/// change, so while a part of the display is read it stays out of captures.
fn sync_island_capture<R: Runtime>(app: &AppHandle<R>) {
    let Some(shared) = app.try_state::<Shared>() else { return };
    let reads_display = {
        let inner = shared.inner.lock().unwrap();
        inner.is_active() && matches!(inner.target.as_ref().map(|t| &t.kind), Some(Kind::Display { .. }))
    };
    crate::island::show_in_captures(app, flag(app, "showInCaptures", true) && !reads_display);
}

// MARK: Publishing

fn snapshot(inner: &Inner, condition: Condition) -> Snapshot {
    let (phase, outcome) = match &inner.phase {
        Phase::Idle => ("idle", None),
        Phase::Watching => ("watching", None),
        Phase::Hidden => ("hidden", None),
        Phase::Finished(outcome) => ("finished", Some(outcome.clone())),
    };
    let can_watch_again = matches!(inner.phase, Phase::Finished(_))
        && inner.target.as_ref().is_some_and(|t| match t.kind {
            Kind::Window { hwnd, .. } => platform::window(hwnd).is_some(),
            Kind::Display { .. } => true,
        });
    let mut lines = support::lines(&inner.text);
    lines.truncate(support::PICKABLE_LINES);
    Snapshot {
        phase,
        outcome,
        target: inner.target.as_ref().map(|t| TargetInfo {
            app_name: t.app_name.clone(),
            window_title: t.window_title.clone(),
            icon: t.icon.clone(),
            is_window: matches!(t.kind, Kind::Window { .. }),
        }),
        preview: inner.preview.clone(),
        text: inner.text.clone(),
        headline: inner.headline.clone(),
        headline_line: inner.headline_line,
        lines,
        started_at: inner.started_at,
        condition,
        match_text: inner.match_text.clone(),
        match_number: inner.match_number,
        can_watch_again,
        access_denied: inner.access_denied,
    }
}

/// Sends the state when it differs from what the island last got.
fn publish<R: Runtime>(app: &AppHandle<R>) {
    let Some(shared) = app.try_state::<Shared>() else { return };
    let current = {
        let mut inner = shared.inner.lock().unwrap();
        let current = snapshot(&inner, condition(app, &inner));
        let json = serde_json::to_string(&current).unwrap_or_default();
        if json == inner.published {
            return;
        }
        inner.published = json;
        current
    };
    let _ = app.emit_to(crate::island::LABEL, "watch://state", &current);
}

// MARK: Watching

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn make_tracker<R: Runtime>(app: &AppHandle<R>, inner: &Inner) -> Tracker {
    Tracker::new(condition(app, inner), &inner.match_text, inner.match_number, &inner.decimal, inner.groups_with_space)
}

fn begin<R: Runtime>(app: &AppHandle<R>, target: Target) {
    let shared = app.state::<Shared>();
    let generation = {
        let mut inner = shared.inner.lock().unwrap();
        // Another area must not show the last one's picture while it is first read.
        if inner.target.as_ref() != Some(&target) {
            inner.preview = None;
        }
        (inner.decimal, inner.groups_with_space) = platform::number_marks();
        inner.target = Some(target);
        inner.phase = Phase::Watching;
        inner.started_at = Some(now_ms());
        inner.fingerprint = None;
        inner.signature = None;
        inner.read_at = None;
        inner.access_denied = false;
        inner.text.clear();
        inner.headline.clear();
        inner.tracker = Some(make_tracker(app, &inner));
        inner.generation += 1;
        inner.generation
    };
    shared.wake.notify_all();
    sync_island_capture(app);
    publish(app);
    let app = app.clone();
    let _ = std::thread::Builder::new().name("watch".into()).spawn(move || run(&app, generation));
}

/// Ends every reading and choice and forgets the area.
fn stop<R: Runtime>(app: &AppHandle<R>) {
    let shared = app.state::<Shared>();
    {
        let mut inner = shared.inner.lock().unwrap();
        let keep = (inner.condition, std::mem::take(&mut inner.match_text), inner.match_number);
        let (generation, choice, page_visible) = (inner.generation, inner.choice, inner.page_visible);
        *inner = Inner {
            condition: keep.0,
            match_text: keep.1,
            match_number: keep.2,
            generation: generation + 1,
            choice: choice + 1,
            page_visible,
            published: std::mem::take(&mut inner.published),
            ..Inner::default()
        };
    }
    shared.wake.notify_all();
    sync_island_capture(app);
    publish(app);
}

/// The rule is met or the window went: the sound, then the island's notice.
fn finish<R: Runtime>(app: &AppHandle<R>, generation: u64, outcome: Outcome) {
    let shared = app.state::<Shared>();
    let event = {
        let mut inner = shared.inner.lock().unwrap();
        if inner.generation != generation || !inner.is_active() {
            return;
        }
        inner.generation += 1;
        inner.phase = Phase::Finished(outcome.clone());
        let Some(target) = inner.target.as_ref() else { return };
        Finished { outcome, app_name: target.app_name.clone(), window_title: target.window_title.clone() }
    };
    shared.wake.notify_all();
    sync_island_capture(app);
    // One play, as the Mac's Glass (NotchWatchService.finish).
    if flag(app, "watchSound", true) {
        crate::sound::play_once();
    }
    publish(app);
    let _ = app.emit_to(crate::island::LABEL, "watch://finished", &event);
}

/// Hidden, or back from hidden; `denied` when Windows refused the capture.
fn set_hidden<R: Runtime>(app: &AppHandle<R>, generation: u64, denied: bool) {
    {
        let shared = app.state::<Shared>();
        let mut inner = shared.inner.lock().unwrap();
        if inner.generation != generation || !inner.is_active() {
            return;
        }
        inner.phase = Phase::Hidden;
        inner.access_denied = denied;
    }
    publish(app);
}

/// The reading thread of one watch: reads, then waits its interval or a wake-up.
fn run<R: Runtime>(app: &AppHandle<R>, generation: u64) {
    #[cfg(windows)]
    let _apartment = crate::com::Apartment::mta();
    let mut source: Option<platform::Source> = None;
    loop {
        read(app, generation, &mut source);
        let shared = app.state::<Shared>();
        let inner = shared.inner.lock().unwrap();
        if inner.generation != generation || !inner.is_active() {
            return;
        }
        let interval = if inner.page_visible { support::VISIBLE_INTERVAL } else { support::BACKGROUND_INTERVAL };
        let (mut inner, _) = shared
            .wake
            .wait_timeout_while(inner, interval, |i| i.generation == generation && !i.poke)
            .unwrap();
        inner.poke = false;
    }
}

fn read<R: Runtime>(app: &AppHandle<R>, generation: u64, source: &mut Option<platform::Source>) {
    let shared = app.state::<Shared>();
    let kind = {
        let inner = shared.inner.lock().unwrap();
        match inner.target.as_ref() {
            Some(target) if inner.generation == generation && inner.is_active() => target.kind.clone(),
            _ => return,
        }
    };
    // The crop and the size of what it's measured on, in the same units.
    let (crop, space) = match &kind {
        Kind::Window { hwnd, crop } => {
            let Some(window) = platform::window(*hwnd) else {
                finish(app, generation, Outcome::Closed);
                return;
            };
            if platform::minimized(*hwnd) {
                set_hidden(app, generation, false);
                return;
            }
            (*crop, (window.bounds.width / window.scale, window.bounds.height / window.scale))
        }
        Kind::Display { monitor, crop } => (*crop, (monitor.width as f64, monitor.height as f64)),
    };
    let frame = match source.get_or_insert_with(|| platform::Source::new(&kind)).grab(crop, space) {
        Grab::Frame(frame) => frame,
        Grab::Waiting => return,
        Grab::Missing => return set_hidden(app, generation, false),
        Grab::Denied => return set_hidden(app, generation, true),
    };
    let picture = support::fingerprint(&frame.rgba, frame.width, frame.height);
    {
        let mut inner = shared.inner.lock().unwrap();
        if inner.generation != generation || !inner.is_active() {
            return;
        }
        inner.phase = Phase::Watching;
        inner.access_denied = false;
        let still = inner.signature.is_some()
            && support::same_fingerprint(picture.as_deref(), inner.fingerprint.as_deref())
            && inner.read_at.is_some_and(|at| at.elapsed() < support::REREAD_INTERVAL);
        if still {
            // The area looks as it did: no need to read it again, but it
            // confirms a change and lets time settle it.
            let Inner { tracker, signature, text, .. } = &mut *inner;
            let outcome = tracker.as_mut().and_then(|t| t.observe(signature.as_deref(), text, Instant::now()));
            drop(inner);
            match outcome {
                Some(outcome) => finish(app, generation, outcome),
                None => publish(app),
            }
            return;
        }
    }
    publish(app);
    // A failed recognition says nothing about the area: read it again next time.
    let Some(recognized) = recognize(&frame) else { return };
    let preview = preview(&frame);
    let outcome = {
        let mut inner = shared.inner.lock().unwrap();
        // Kept only once read: a reading dropped halfway must not leave the
        // area marked as read, or a still area would never be read again.
        if inner.generation != generation || !inner.is_active() {
            return;
        }
        inner.fingerprint = picture;
        inner.read_at = Some(Instant::now());
        inner.preview = preview.or(inner.preview.take());
        inner.headline = inner.headline_for(&recognized);
        inner.signature = support::signature(&recognized, inner.fingerprint.as_deref());
        inner.text = recognized;
        let Inner { tracker, signature, text, .. } = &mut *inner;
        tracker.as_mut().and_then(|t| t.observe(signature.as_deref(), text, Instant::now()))
    };
    match outcome {
        Some(outcome) => finish(app, generation, outcome),
        None => publish(app),
    }
}

/// The area's text, read at most 900 px a side (the OCR helper then scales
/// small text up as it does for Copy text from screen). Empty without an
/// OCR language, as an area without text; None when recognition failed.
fn recognize(frame: &Frame) -> Option<String> {
    let scale = support::recognition_scale(frame.width, frame.height);
    let result = if scale < 1.0 {
        let image = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba.clone())?;
        let (w, h) = crate::ocr::scaled_size(frame.width, frame.height, scale);
        let small = image::imageops::resize(&image, w, h, image::imageops::FilterType::Triangle);
        crate::ocr::recognize(small.as_raw(), w, h)
    } else {
        crate::ocr::recognize(&frame.rgba, frame.width, frame.height)
    };
    match result {
        Ok(result) => Some(result.text()),
        Err(crate::ocr::OcrError::NoLanguage) => Some(String::new()),
        Err(crate::ocr::OcrError::Failed(_)) => None,
    }
}

/// A small PNG of the area for the page and the closed island.
fn preview(frame: &Frame) -> Option<String> {
    let image = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba.clone())?;
    let scale = (PREVIEW_SIDE as f64 / frame.width.max(frame.height) as f64).min(1.0);
    let (w, h) = crate::ocr::scaled_size(frame.width, frame.height, scale);
    let small = if scale < 1.0 { image::imageops::thumbnail(&image, w, h) } else { image };
    crate::shell::encode_png(small.as_raw(), w, h).map(|png| crate::shell::png_data_url(&png))
}

/// The selection as a target: the window under its middle with the area in
/// points from that window's corner, or else that part of the display.
fn resolve(selection: &crate::capture::Selection) -> Target {
    let r = selection.rect;
    let area = Area { x: r.x as f64, y: r.y as f64, width: r.width as f64, height: r.height as f64 };
    if selection.window != 0
        && let Some(window) = platform::window(selection.window)
        && let Some(crop) = support::window_crop(area, window.bounds)
    {
        let exe = crate::shell::process_path(window.pid);
        // A Store app's window belongs to its frame host, whose name says nothing.
        let hosted = exe.as_deref().is_some_and(|e| e.to_ascii_lowercase().ends_with("\\applicationframehost.exe"));
        let app_name = match exe.as_deref() {
            Some(exe) if !hosted => crate::shell::app_name(exe),
            _ => window.title.clone().unwrap_or_default(),
        };
        let icon = exe.as_deref().filter(|_| !hosted).and_then(|e| crate::shell::icon(e, 32));
        return Target {
            kind: Kind::Window { hwnd: selection.window, crop: crop.in_points(window.scale) },
            app_name,
            window_title: window.title,
            icon,
        };
    }
    let m = selection.monitor;
    Target {
        kind: Kind::Display {
            monitor: m,
            crop: Area { x: (r.x - m.x) as f64, y: (r.y - m.y) as f64, width: r.width as f64, height: r.height as f64 },
        },
        app_name: String::new(),
        window_title: None,
        icon: None,
    }
}

// MARK: Commands

#[tauri::command]
pub fn watch_snapshot<R: Runtime>(app: AppHandle<R>, shared: tauri::State<'_, Shared>) -> Snapshot {
    let inner = shared.inner.lock().unwrap();
    snapshot(&inner, condition(&app, &inner))
}

/// Shows the selection overlay (collapse the island first) and starts
/// watching what the person chose. True when a watch started.
#[tauri::command]
pub async fn watch_choose<R: Runtime>(app: AppHandle<R>) -> bool {
    let choice = {
        let shared = app.state::<Shared>();
        let mut inner = shared.inner.lock().unwrap();
        if !enabled(&app) || inner.choosing {
            return false;
        }
        inner.choosing = true;
        inner.choice += 1;
        inner.choice
    };
    let selection = crate::capture::select_area(&app, crate::capture::Purpose::Watch).await;
    let target = match selection {
        Some(selection) => tauri::async_runtime::spawn_blocking(move || resolve(&selection)).await.ok(),
        None => None,
    };
    {
        let shared = app.state::<Shared>();
        let mut inner = shared.inner.lock().unwrap();
        inner.choosing = false;
        // Stopped, or switched off, while the person was choosing.
        if inner.choice != choice || !enabled(&app) || target.is_none() {
            return false;
        }
        // Another area has other lines: its reading starts automatic again.
        inner.headline_line = None;
    }
    begin(&app, target.unwrap());
    true
}

#[tauri::command]
pub fn watch_stop<R: Runtime>(app: AppHandle<R>) {
    stop(&app);
}

/// The same area again after a watch ended, while its window is still there.
#[tauri::command]
pub fn watch_again<R: Runtime>(app: AppHandle<R>) -> bool {
    let target = app.state::<Shared>().inner.lock().unwrap().target.clone();
    let Some(target) = target.filter(|_| enabled(&app)) else { return false };
    if let Kind::Window { hwnd, .. } = target.kind
        && platform::window(hwnd).is_none()
    {
        return false;
    }
    begin(&app, target);
    true
}

/// The rule from the page. Typed words and numbers arrive once confirmed
/// (Enter or leaving the field), so a half-typed "1" on the way to 100
/// can't end the watch early; a new rule starts from what the area shows now.
#[tauri::command]
pub fn watch_set_rule<R: Runtime>(
    app: AppHandle<R>,
    condition: Option<Condition>,
    text: Option<String>,
    number: Option<String>,
) {
    let shared = app.state::<Shared>();
    let (generation, outcome) = {
        let mut inner = shared.inner.lock().unwrap();
        let mut reset = false;
        if let Some(condition) = condition {
            // Against the running tracker's rule: the page saves the new one
            // to the preferences first, so they already agree with it.
            reset |= inner.tracker.as_ref().is_some_and(|t| t.condition() != condition);
            inner.condition = Some(condition);
        }
        let current = self::condition(&app, &inner);
        if let Some(text) = text {
            let trimmed = text.trim().to_string();
            if trimmed != inner.match_text {
                inner.match_text = trimmed;
                reset |= current == Condition::Contains;
            }
        }
        if let Some(number) = number {
            if inner.decimal.is_empty() {
                (inner.decimal, inner.groups_with_space) = platform::number_marks();
            }
            let value = support::typed_number(&number, &inner.decimal);
            if value != inner.match_number {
                inner.match_number = value;
                reset |= current == Condition::Reaches;
            }
        }
        let mut outcome = None;
        if reset {
            let mut tracker = make_tracker(&app, &inner);
            if inner.is_active() && inner.phase == Phase::Watching && inner.signature.is_some() {
                outcome = tracker.observe(inner.signature.as_deref(), &inner.text, Instant::now());
            }
            inner.tracker = Some(tracker);
        }
        (inner.generation, outcome)
    };
    match outcome {
        Some(outcome) => finish(&app, generation, outcome),
        None => publish(&app),
    }
}

/// The line the closed island shows; None goes back to automatic.
#[tauri::command]
pub fn watch_pin_line<R: Runtime>(app: AppHandle<R>, line: Option<usize>) {
    {
        let shared = app.state::<Shared>();
        let mut inner = shared.inner.lock().unwrap();
        if inner.headline_line == line {
            return;
        }
        inner.headline_line = line;
        inner.headline = inner.headline_for(&inner.text);
    }
    publish(&app);
}

/// The page is on screen in the open island: read every second, starting now.
#[tauri::command]
pub fn watch_page_visible<R: Runtime>(app: AppHandle<R>, visible: bool) {
    let shared = app.state::<Shared>();
    let mut inner = shared.inner.lock().unwrap();
    if visible && !inner.page_visible && inner.is_active() {
        inner.poke = true;
        shared.wake.notify_all();
    }
    inner.page_visible = visible;
}

/// A Windows notification for a watch that ended while the island couldn't
/// show its notice (hidden in a full-screen app). Needs the app's package
/// identity; false when it was not posted.
#[tauri::command]
pub async fn watch_notify(title: String, body: String) -> bool {
    tauri::async_runtime::spawn_blocking(move || platform::toast(&title, &body)).await.unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_selection_over_the_desktop_reads_that_part_of_the_display() {
        // The stub knows no windows: every selection is a display region.
        let selection = crate::capture::Selection {
            purpose: crate::capture::Purpose::Watch,
            rect: crate::capture::Rect { x: 1940, y: 30, width: 200, height: 40 },
            monitor: crate::capture::Rect { x: 1920, y: 0, width: 2560, height: 1440 },
            window: 0,
        };
        let target = resolve(&selection);
        assert_eq!(
            target.kind,
            Kind::Display { monitor: selection.monitor, crop: Area { x: 20.0, y: 30.0, width: 200.0, height: 40.0 } }
        );
        assert_eq!(target.app_name, "");
    }

    #[test]
    fn previews_shrink_to_their_longest_side() {
        let frame = Frame { width: 1280, height: 40, rgba: vec![200; 1280 * 40 * 4] };
        let url = preview(&frame).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        let small = Frame { width: 4, height: 4, rgba: vec![0; 64] };
        assert!(preview(&small).is_some());
    }

    #[test]
    fn snapshots_serialize_for_the_page() {
        let inner = Inner {
            phase: Phase::Finished(Outcome::Reached("100%".into())),
            text: "Build\n100%".into(),
            ..Inner::default()
        };
        let json = serde_json::to_value(snapshot(&inner, Condition::Changes)).unwrap();
        assert_eq!(json["phase"], "finished");
        assert_eq!(json["outcome"], serde_json::json!({ "kind": "reached", "text": "100%" }));
        assert_eq!(json["lines"], serde_json::json!(["Build", "100%"]));
        assert_eq!(json["condition"], "changes");
        assert_eq!(json["canWatchAgain"], false);
    }
}
