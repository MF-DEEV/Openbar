// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The Calendar section's reader (spec 04 §3.5, NotchCalendarService.swift).
//! Windows shares appointments through `AppointmentManager`, which answers
//! only an app with a package identity and the `appointments` capability
//! (the sparse package) that the person allowed. One thread owns the store:
//! it reads the coming week, and the month the page shows, and reads again
//! when the store changes, the clock or time zone changes, the PC wakes, and
//! at the next boundary (an event's start or end, midnight, or 15 minutes).
//! Every read that changes something goes to all windows as
//! `calendar://events`. No event text is kept anywhere but in memory.

// The reader itself is Windows only; on a Mac only the commands answer.
#![cfg_attr(not(windows), allow(dead_code))]

use std::collections::HashSet;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;

pub const EVENT: &str = "calendar://events";
/// The agenda looks again at least this often (NotchCalendarSupport.nextRefresh).
const QUARTER_HOUR: i64 = 15 * 60 * 1000;
/// While access is missing, how often the reader checks whether it was granted.
const ACCESS_POLL: Duration = Duration::from_secs(30);
/// A calendar without a colour of its own (NotchCalendarColor.fallback).
const FALLBACK_COLOR: &str = "#59a6ff";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Access {
    /// No package identity, or one without the appointments capability.
    #[default]
    Unavailable,
    /// Windows has not asked the person yet.
    NotAsked,
    Denied,
    Allowed,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    pub title: String,
    pub calendar: String,
    pub calendar_id: String,
    /// Epoch milliseconds.
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    pub location: String,
    pub color: String,
    pub local_id: String,
    /// The occurrence's original start in a series, for opening that one.
    pub instance_start: Option<i64>,
    pub countdown_key: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub id: String,
    pub title: String,
    pub source_id: String,
    pub source: String,
    pub color: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub access: Access,
    /// The range the page asked for, or the coming week.
    pub events: Vec<Event>,
    /// Always the coming week, which countdowns follow.
    pub current: Vec<Event>,
    pub calendars: Vec<Choice>,
    /// 0 = Sunday.
    pub first_weekday: u8,
    pub minimum_days_in_first_week: u8,
}

impl Default for Snapshot {
    fn default() -> Self {
        Snapshot {
            access: Access::Unavailable,
            events: Vec::new(),
            current: Vec::new(),
            calendars: Vec::new(),
            first_weekday: 1,
            minimum_days_in_first_week: 4,
        }
    }
}

/// An appointment as Windows returns it, before it is filtered.
#[derive(Clone, Debug, Default)]
pub struct Raw {
    pub local_id: String,
    pub calendar_id: String,
    pub title: String,
    pub location: String,
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    pub canceled: bool,
    pub declined: bool,
    pub original_start: Option<i64>,
}

/// Names the event a countdown was chosen for across reads and relaunches:
/// the appointment, or one occurrence of a series by the start it first had,
/// which moving that occurrence leaves unchanged (NotchCalendarSupport.countdownKey).
pub fn countdown_key(local_id: &str, original_start: Option<i64>) -> String {
    match original_start {
        Some(ms) => format!("{local_id}@{}", ms.div_euclid(1000)),
        None => local_id.to_string(),
    }
}

/// Canceled meetings and those the person declined stay out, as do hidden
/// calendars and appointments without a real span.
pub fn keep(raw: &Raw, excluded: &HashSet<String>) -> bool {
    !raw.canceled && !raw.declined && raw.end > raw.start && !excluded.contains(&raw.calendar_id)
}

pub fn event(raw: Raw, calendar: Option<&Choice>) -> Event {
    Event {
        id: format!("{}:{}", raw.local_id, raw.start),
        countdown_key: if raw.local_id.is_empty() {
            String::new()
        } else {
            countdown_key(&raw.local_id, raw.original_start)
        },
        title: raw.title,
        calendar: calendar.map(|c| c.title.clone()).unwrap_or_default(),
        calendar_id: raw.calendar_id,
        start: raw.start,
        end: raw.end,
        all_day: raw.all_day,
        location: raw.location,
        color: calendar.map_or_else(|| FALLBACK_COLOR.into(), |c| c.color.clone()),
        local_id: raw.local_id,
        instance_start: raw.original_start,
    }
}

/// The calendars to ask for: None reads every one, including those added
/// later; an empty list means read nothing (an empty filter would read all).
pub fn calendars_to_read(all: &[String], excluded: &HashSet<String>) -> Option<Vec<String>> {
    if !all.iter().any(|id| excluded.contains(id)) {
        return None;
    }
    Some(
        all.iter()
            .filter(|id| !excluded.contains(*id))
            .cloned()
            .collect(),
    )
}

/// When to read again: the nearest start or end still ahead, midnight, or a
/// quarter of an hour from now (NotchCalendarSupport.nextRefresh).
pub fn next_refresh(events: &[Event], now: i64, midnight: i64) -> i64 {
    events
        .iter()
        .filter(|e| e.end > now)
        .flat_map(|e| [e.start, e.end])
        .chain([midnight, now + QUARTER_HOUR])
        .filter(|&t| t > now)
        .min()
        .unwrap_or(now + QUARTER_HOUR)
}

pub fn color_hex(r: u8, g: u8, b: u8, a: u8) -> String {
    if a == 0 {
        FALLBACK_COLOR.into()
    } else {
        format!("#{r:02x}{g:02x}{b:02x}")
    }
}

/// The days after LOCALE_IFIRSTWEEKOFYEAR's rule: the week with 1 January,
/// the first full week, or the first with four days.
pub fn minimum_days(first_week_of_year: u8) -> u8 {
    match first_week_of_year {
        1 => 7,
        2 => 4,
        _ => 1,
    }
}

/// LOCALE_IFIRSTDAYOFWEEK counts from Monday; the page counts from Sunday.
pub fn weekday_from_monday(day: u8) -> u8 {
    (day % 7 + 1) % 7
}

// MARK: Shared state

struct Message {
    /// A new range for the page: Some(None) for the coming week alone.
    range: Option<Option<(i64, i64)>>,
    reply: Option<Sender<Snapshot>>,
}

#[derive(Default)]
struct Inner {
    snapshot: Mutex<Snapshot>,
    tx: Mutex<Option<Sender<Message>>>,
}

#[derive(Clone, Default)]
pub struct Shared(Arc<Inner>);

impl Shared {
    fn send(&self, message: Message) -> bool {
        self.0
            .tx
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|tx| tx.send(message).is_ok())
    }

    /// Reads again, now.
    pub fn poke(&self) {
        self.send(Message {
            range: None,
            reply: None,
        });
    }

    /// Reads (with a new range, if given) and answers with the result.
    fn ask(&self, range: Option<Option<(i64, i64)>>) -> Snapshot {
        let (reply, answer) = channel();
        if self.send(Message {
            range,
            reply: Some(reply),
        }) && let Ok(snapshot) = answer.recv_timeout(Duration::from_secs(10))
        {
            return snapshot;
        }
        self.0.snapshot.lock().unwrap().clone()
    }
}

/// Reads again when the preferences it follows change: the section's
/// switches and the calendars shown.
pub fn apply<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    before: Option<&serde_json::Value>,
    value: &serde_json::Value,
) {
    use tauri::Manager;
    let followed = |v: &serde_json::Value| (enabled(v), excluded(v));
    if before.is_none_or(|b| followed(b) != followed(value))
        && let Some(shared) = app.try_state::<Shared>()
    {
        shared.poke();
    }
}

/// Whether the section is on: the island, its switch and its place in the list.
fn enabled(prefs: &serde_json::Value) -> bool {
    let flag = |pointer: &str| {
        prefs
            .pointer(pointer)
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
    };
    let hidden = prefs
        .pointer("/island/hiddenModules")
        .and_then(|v| v.as_array())
        .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some("calendar")));
    flag("/island/enabled") && flag("/island/sections/calendar") && !hidden
}

fn excluded(prefs: &serde_json::Value) -> HashSet<String> {
    prefs
        .pointer("/island/calendarExcluded")
        .and_then(|v| v.as_array())
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

#[cfg(windows)]
mod imp {
    use std::collections::{HashMap, HashSet};
    use std::sync::mpsc::Sender;

    use windows::ApplicationModel::Appointments::{
        Appointment, AppointmentCalendar, AppointmentManager, AppointmentParticipantResponse,
        AppointmentProperties, AppointmentStore, AppointmentStoreAccessType,
        AppointmentStoreChangedEventArgs, FindAppointmentsOptions,
    };
    use windows::ApplicationModel::Package;
    use windows::Foundation::{DateTime, TimeSpan, TypedEventHandler};
    use windows::Security::Authorization::AppCapabilityAccess::{
        AppCapability, AppCapabilityAccessStatus,
    };
    use windows::Win32::Foundation::{FILETIME, HWND, LPARAM, LRESULT, SYSTEMTIME, WPARAM};
    use windows::Win32::Globalization::{
        GetLocaleInfoEx, LOCALE_IFIRSTDAYOFWEEK, LOCALE_IFIRSTWEEKOFYEAR,
    };
    use windows::Win32::System::Time::{
        FileTimeToSystemTime, SystemTimeToFileTime, SystemTimeToTzSpecificLocalTimeEx,
        TzSpecificLocalTimeToSystemTimeEx,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MSG,
        PBT_APMRESUMEAUTOMATIC, RegisterClassW, WINDOW_EX_STYLE, WM_POWERBROADCAST,
        WM_SETTINGCHANGE, WM_TIMECHANGE, WNDCLASSW, WS_POPUP,
    };
    use windows::core::{HSTRING, PCWSTR, w};

    use super::{Access, Choice, Message, Raw};

    /// 100 ns ticks between 1601 and 1970.
    const EPOCH_TICKS: i64 = 116_444_736_000_000_000;

    pub fn to_ms(t: DateTime) -> i64 {
        (t.UniversalTime - EPOCH_TICKS).div_euclid(10_000)
    }

    pub fn from_ms(ms: i64) -> DateTime {
        DateTime {
            UniversalTime: ms * 10_000 + EPOCH_TICKS,
        }
    }

    fn filetime(ticks: i64) -> FILETIME {
        FILETIME {
            dwLowDateTime: ticks as u32,
            dwHighDateTime: (ticks >> 32) as u32,
        }
    }

    fn ticks(ft: FILETIME) -> i64 {
        ((ft.dwHighDateTime as i64) << 32) | ft.dwLowDateTime as i64
    }

    /// Local midnight `days` after the day holding `ms`, as epoch ms, by the
    /// time zone's own rules for that date.
    pub fn local_midnight(ms: i64, days: i64) -> Option<i64> {
        unsafe {
            let mut utc = SYSTEMTIME::default();
            FileTimeToSystemTime(&filetime(from_ms(ms).UniversalTime), &mut utc).ok()?;
            let mut local = SYSTEMTIME::default();
            SystemTimeToTzSpecificLocalTimeEx(None, &utc, &mut local).ok()?;
            local.wHour = 0;
            local.wMinute = 0;
            local.wSecond = 0;
            local.wMilliseconds = 0;
            let mut ft = FILETIME::default();
            SystemTimeToFileTime(&local, &mut ft).ok()?;
            let shifted = filetime(ticks(ft) + days * 864_000_000_000);
            FileTimeToSystemTime(&shifted, &mut local).ok()?;
            TzSpecificLocalTimeToSystemTimeEx(None, &local, &mut utc).ok()?;
            SystemTimeToFileTime(&utc, &mut ft).ok()?;
            Some(to_ms(DateTime {
                UniversalTime: ticks(ft),
            }))
        }
    }

    fn locale_number(kind: u32) -> Option<u8> {
        let mut buffer = [0u16; 8];
        let n = unsafe { GetLocaleInfoEx(PCWSTR::null(), kind, Some(&mut buffer)) };
        if n <= 1 {
            return None;
        }
        String::from_utf16_lossy(&buffer[..n as usize - 1])
            .trim()
            .parse()
            .ok()
    }

    /// The user's first weekday (0 = Sunday) and first-week rule.
    pub fn week_rules() -> (u8, u8) {
        (
            locale_number(LOCALE_IFIRSTDAYOFWEEK).map_or(1, super::weekday_from_monday),
            locale_number(LOCALE_IFIRSTWEEKOFYEAR).map_or(4, super::minimum_days),
        )
    }

    pub fn access() -> Access {
        if Package::Current().is_err() {
            return Access::Unavailable;
        }
        match AppCapability::Create(&HSTRING::from("appointments")).and_then(|c| c.CheckAccess()) {
            Ok(AppCapabilityAccessStatus::Allowed) => Access::Allowed,
            Ok(AppCapabilityAccessStatus::UserPromptRequired) => Access::NotAsked,
            Ok(
                AppCapabilityAccessStatus::DeniedByUser | AppCapabilityAccessStatus::DeniedBySystem,
            ) => Access::Denied,
            // An identity registered before the capability was added.
            Ok(_) => Access::Unavailable,
            Err(_) => Access::NotAsked,
        }
    }

    /// Asking for the store is what shows Windows' consent prompt.
    pub fn request_access() -> Access {
        if Package::Current().is_err() {
            return Access::Unavailable;
        }
        let _ =
            AppointmentManager::RequestStoreAsync(AppointmentStoreAccessType::AllCalendarsReadOnly)
                .and_then(|op| op.join());
        access()
    }

    /// The store, with change tracking on and every change sent to the reader.
    pub fn open_store(tx: Sender<Message>) -> windows::core::Result<AppointmentStore> {
        let store = AppointmentManager::RequestStoreAsync(
            AppointmentStoreAccessType::AllCalendarsReadOnly,
        )?
        .join()?;
        // Best effort: without tracking, StoreChanged may stay quiet, and the
        // boundary timer still reads at least every quarter hour.
        if let Ok(tracker) = store.ChangeTracker() {
            let _ = tracker.Enable();
        }
        store.StoreChanged(&TypedEventHandler::<
            AppointmentStore,
            AppointmentStoreChangedEventArgs,
        >::new(move |_, args| {
            if let Some(args) = args.as_ref()
                && let Ok(deferral) = args.GetDeferral()
            {
                let _ = deferral.Complete();
            }
            let _ = tx.send(Message {
                range: None,
                reply: None,
            });
            Ok(())
        }))?;
        Ok(store)
    }

    fn choice(calendar: &AppointmentCalendar) -> Option<Choice> {
        let color = calendar.DisplayColor().ok();
        let source = calendar
            .SourceDisplayName()
            .map(|s| s.to_string())
            .unwrap_or_default();
        Some(Choice {
            id: calendar.LocalId().ok()?.to_string(),
            title: calendar
                .DisplayName()
                .map(|s| s.to_string())
                .unwrap_or_default(),
            source_id: calendar
                .UserDataAccountId()
                .map(|s| s.to_string())
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| source.clone()),
            source,
            color: color.map_or_else(
                || super::FALLBACK_COLOR.into(),
                |c| super::color_hex(c.R, c.G, c.B, c.A),
            ),
        })
    }

    pub fn calendars(store: &AppointmentStore) -> windows::core::Result<Vec<Choice>> {
        Ok(store
            .FindAppointmentCalendarsAsync()?
            .join()?
            .into_iter()
            .filter(|c| !c.IsHidden().unwrap_or(false))
            .filter_map(|c| choice(&c))
            .collect())
    }

    fn raw(a: &Appointment) -> Raw {
        let start = a.StartTime().map(to_ms).unwrap_or_default();
        let duration = a
            .Duration()
            .map_or(0, |d: TimeSpan| d.Duration.div_euclid(10_000));
        Raw {
            local_id: a.LocalId().map(|s| s.to_string()).unwrap_or_default(),
            calendar_id: a.CalendarId().map(|s| s.to_string()).unwrap_or_default(),
            title: a.Subject().map(|s| s.to_string()).unwrap_or_default(),
            location: a.Location().map(|s| s.to_string()).unwrap_or_default(),
            start,
            end: start + duration,
            all_day: a.AllDay().unwrap_or(false),
            canceled: a.IsCanceledMeeting().unwrap_or(false),
            declined: a
                .UserResponse()
                .is_ok_and(|r| r == AppointmentParticipantResponse::Declined),
            original_start: a
                .OriginalStartTime()
                .ok()
                .and_then(|r| r.Value().ok())
                .map(to_ms),
        }
    }

    /// The appointments touching `[start, end)` in the calendars asked for.
    pub fn read(
        store: &AppointmentStore,
        (start, end): (i64, i64),
        calendars: &[Choice],
        excluded: &HashSet<String>,
    ) -> windows::core::Result<Vec<super::Event>> {
        let ids: Vec<String> = calendars.iter().map(|c| c.id.clone()).collect();
        let wanted = super::calendars_to_read(&ids, excluded);
        if wanted.as_ref().is_some_and(Vec::is_empty) {
            return Ok(Vec::new());
        }
        let options = FindAppointmentsOptions::new()?;
        let fetch = options.FetchProperties()?;
        for name in [
            AppointmentProperties::Subject()?,
            AppointmentProperties::Location()?,
            AppointmentProperties::StartTime()?,
            AppointmentProperties::Duration()?,
            AppointmentProperties::AllDay()?,
            AppointmentProperties::IsCanceledMeeting()?,
            AppointmentProperties::UserResponse()?,
            AppointmentProperties::Recurrence()?,
            AppointmentProperties::OriginalStartTime()?,
        ] {
            fetch.Append(&name)?;
        }
        if let Some(wanted) = wanted {
            let list = options.CalendarIds()?;
            for id in wanted {
                list.Append(&HSTRING::from(id))?;
            }
        }
        let span = TimeSpan {
            Duration: (end - start).max(0) * 10_000,
        };
        let by_id: HashMap<&str, &Choice> = calendars.iter().map(|c| (c.id.as_str(), c)).collect();
        Ok(store
            .FindAppointmentsAsyncWithOptions(from_ms(start), span, &options)?
            .join()?
            .into_iter()
            .map(|a| raw(&a))
            .filter(|r| super::keep(r, excluded))
            .map(|r| {
                let calendar = by_id.get(r.calendar_id.as_str()).copied();
                super::event(r, calendar)
            })
            .collect())
    }

    /// Opens one appointment, the chosen occurrence of a series, in the
    /// default calendar app; without one, or if that fails, Calendar itself.
    pub fn open(local_id: Option<String>, instance_start: Option<i64>) -> bool {
        let shown = local_id.filter(|id| !id.is_empty()).is_some_and(|id| {
            let id = HSTRING::from(id);
            match instance_start {
                Some(ms) => {
                    AppointmentManager::ShowAppointmentDetailsWithDateAsync(&id, from_ms(ms))
                }
                None => AppointmentManager::ShowAppointmentDetailsAsync(&id),
            }
            .and_then(|op| op.join())
            .is_ok()
        });
        shown || tauri_plugin_opener::open_url("outlookcal:", None::<&str>).is_ok()
    }

    static CLOCK: std::sync::OnceLock<Sender<Message>> = std::sync::OnceLock::new();

    extern "system" fn clock_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let intl = msg == WM_SETTINGCHANGE
            && lparam.0 != 0
            && unsafe { PCWSTR(lparam.0 as *const u16).to_string() }.is_ok_and(|s| s == "intl");
        let resumed = msg == WM_POWERBROADCAST && wparam.0 as u32 == PBT_APMRESUMEAUTOMATIC;
        if (msg == WM_TIMECHANGE || intl || resumed)
            && let Some(tx) = CLOCK.get()
        {
            let _ = tx.send(Message {
                range: None,
                reply: None,
            });
        }
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    /// A hidden top-level window, since message-only windows miss the
    /// broadcasts: the clock or time zone changed (WM_TIMECHANGE), the
    /// region's formats changed, or the PC woke.
    pub fn watch_clock(tx: Sender<Message>) {
        if CLOCK.set(tx).is_err() {
            return;
        }
        let _ = std::thread::Builder::new()
            .name("calendar-clock".into())
            .spawn(|| unsafe {
                let class = WNDCLASSW {
                    lpfnWndProc: Some(clock_proc),
                    lpszClassName: w!("VorssaintCalendarClock"),
                    ..Default::default()
                };
                if RegisterClassW(&class) == 0 {
                    return;
                }
                let Ok(_hwnd) = CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    w!("VorssaintCalendarClock"),
                    w!(""),
                    WS_POPUP,
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    None,
                    None,
                ) else {
                    return;
                };
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    DispatchMessageW(&msg);
                }
            });
    }
}

/// Starts the reader thread.
pub fn start<R: tauri::Runtime>(app: tauri::AppHandle<R>, shared: Shared) {
    let (tx, rx) = channel();
    *shared.0.tx.lock().unwrap() = Some(tx.clone());
    #[cfg(windows)]
    {
        imp::watch_clock(tx.clone());
        let _ = std::thread::Builder::new()
            .name("calendar".into())
            .spawn(move || run(app, shared, tx, rx));
    }
    #[cfg(not(windows))]
    {
        // Answers every request with what it has: nothing to read here.
        let _ = (app, tx);
        let _ = std::thread::Builder::new()
            .name("calendar".into())
            .spawn(move || {
                for message in rx {
                    if let Some(reply) = message.reply {
                        let _ = reply.send(shared.0.snapshot.lock().unwrap().clone());
                    }
                }
            });
    }
}

#[cfg(windows)]
fn run<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    shared: Shared,
    tx: Sender<Message>,
    rx: std::sync::mpsc::Receiver<Message>,
) {
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::Instant;
    use tauri::{Emitter, Manager};

    let _apartment = crate::com::Apartment::mta();
    let mut store: Option<windows::ApplicationModel::Appointments::AppointmentStore> = None;
    let mut visible: Option<(i64, i64)> = None;
    let mut wake = Instant::now();
    loop {
        let mut replies = Vec::new();
        let first = match rx.recv_timeout(wake.saturating_duration_since(Instant::now())) {
            Ok(message) => Some(message),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        // Everything queued is answered by one read.
        for message in first.into_iter().chain(rx.try_iter()) {
            if let Some(range) = message.range {
                visible = range;
            }
            replies.extend(message.reply);
        }

        let prefs = app.state::<crate::prefs::Prefs>().get();
        let (first_weekday, minimum_days) = imp::week_rules();
        let mut next = Snapshot {
            access: imp::access(),
            first_weekday,
            minimum_days_in_first_week: minimum_days,
            ..Snapshot::default()
        };
        let now = now_ms();
        wake = Instant::now() + ACCESS_POLL;
        if !enabled(&prefs) || next.access != Access::Allowed {
            store = None;
        } else {
            if store.is_none() {
                store = imp::open_store(tx.clone()).ok();
            }
            let read = store.as_ref().and_then(|s| {
                let excluded = excluded(&prefs);
                let today = imp::local_midnight(now, 0).unwrap_or(now);
                let week = (
                    today,
                    imp::local_midnight(now, 7).unwrap_or(today + 7 * 86_400_000),
                );
                let calendars = imp::calendars(s).ok()?;
                let current = imp::read(s, week, &calendars, &excluded).ok()?;
                let events = match visible {
                    Some(range) if range != week => {
                        imp::read(s, range, &calendars, &excluded).ok()?
                    }
                    _ => current.clone(),
                };
                Some((current, events, calendars))
            });
            match read {
                Some((current, events, calendars)) => {
                    let midnight = imp::local_midnight(now, 1).unwrap_or(now + QUARTER_HOUR);
                    let at = next_refresh(&events, now, midnight)
                        .min(next_refresh(&current, now, midnight));
                    wake = Instant::now()
                        + Duration::from_millis((at - now).clamp(1000, i64::MAX) as u64);
                    next.current = current;
                    next.events = events;
                    next.calendars = calendars;
                }
                // A store that stopped answering is asked for again in 30 s;
                // until then the last read stays, rather than an empty week.
                None => {
                    store = None;
                    let last = shared.0.snapshot.lock().unwrap();
                    next.current = last.current.clone();
                    next.events = last.events.clone();
                    next.calendars = last.calendars.clone();
                }
            }
        }

        let changed = {
            let mut snapshot = shared.0.snapshot.lock().unwrap();
            let changed = *snapshot != next;
            *snapshot = next.clone();
            changed
        };
        if changed {
            let _ = app.emit(EVENT, &next);
        }
        for reply in replies {
            let _ = reply.send(next.clone());
        }
    }
}

#[tauri::command]
pub async fn calendar_snapshot(state: tauri::State<'_, Shared>) -> Result<Snapshot, ()> {
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || shared.ask(None))
        .await
        .map_err(|_| ())
}

/// The page's month grid (epoch ms), or nulls for the coming week alone.
#[tauri::command]
pub async fn calendar_show_range(
    state: tauri::State<'_, Shared>,
    start: Option<f64>,
    end: Option<f64>,
) -> Result<Snapshot, ()> {
    let range = start
        .zip(end)
        .filter(|(s, e)| s.is_finite() && e.is_finite() && e > s)
        .map(|(s, e)| (s as i64, e as i64));
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || shared.ask(Some(range)))
        .await
        .map_err(|_| ())
}

/// Asks for the store, which shows Windows' consent prompt. It runs off the
/// UI thread: waiting on it there would freeze every window while the prompt
/// is open. If Windows shows no prompt from here, the page offers Settings.
#[tauri::command]
pub async fn calendar_request_access(state: tauri::State<'_, Shared>) -> Result<Access, ()> {
    // The consent prompt waits on the person; one Windows never shows must
    // not leave the page asking for good (Watch's borderless request did).
    #[cfg(windows)]
    let access = tauri::async_runtime::spawn_blocking(|| {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _apartment = crate::com::Apartment::mta();
            let _ = tx.send(imp::request_access());
        });
        rx.recv_timeout(std::time::Duration::from_secs(120)).unwrap_or(Access::NotAsked)
    })
    .await
    .unwrap_or(Access::Unavailable);
    #[cfg(not(windows))]
    let access = Access::Unavailable;
    state.poke();
    Ok(access)
}

#[tauri::command]
pub async fn calendar_open(local_id: Option<String>, instance_start: Option<f64>) -> bool {
    let instance = instance_start.filter(|t| t.is_finite()).map(|t| t as i64);
    #[cfg(windows)]
    return tauri::async_runtime::spawn_blocking(move || imp::open(local_id, instance))
        .await
        .unwrap_or(false);
    #[cfg(not(windows))]
    {
        let _ = (local_id, instance);
        false
    }
}

/// Settings › Privacy & security › Calendar.
#[tauri::command]
pub fn calendar_open_privacy_settings() -> bool {
    tauri_plugin_opener::open_url("ms-settings:privacy-calendar", None::<&str>).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60_000;

    fn ev(start: i64, end: i64) -> Event {
        event(
            Raw {
                local_id: "a".into(),
                start,
                end,
                ..Raw::default()
            },
            None,
        )
    }

    #[test]
    fn next_refresh_takes_the_nearest_boundary() {
        let now = 1_000 * MIN;
        let midnight = now + 600 * MIN;
        // The start of an event ahead.
        assert_eq!(
            next_refresh(&[ev(now + 5 * MIN, now + 30 * MIN)], now, midnight),
            now + 5 * MIN
        );
        // The end of one under way.
        assert_eq!(
            next_refresh(&[ev(now - 5 * MIN, now + 7 * MIN)], now, midnight),
            now + 7 * MIN
        );
        // Ended events and far ones leave the quarter hour.
        let quiet = [
            ev(now - 60 * MIN, now - 30 * MIN),
            ev(now + 120 * MIN, now + 180 * MIN),
        ];
        assert_eq!(next_refresh(&quiet, now, midnight), now + 15 * MIN);
        // Midnight when it comes first.
        assert_eq!(next_refresh(&[], now, now + 3 * MIN), now + 3 * MIN);
        // A boundary at this very moment has passed.
        assert_eq!(
            next_refresh(&[ev(now, now + 20 * MIN)], now, midnight),
            now + 15 * MIN
        );
    }

    #[test]
    fn canceled_declined_and_hidden_calendars_stay_out() {
        let excluded: HashSet<String> = ["hidden".to_string()].into();
        let base = Raw {
            calendar_id: "work".into(),
            start: 0,
            end: MIN,
            ..Raw::default()
        };
        assert!(keep(&base, &excluded));
        assert!(!keep(
            &Raw {
                canceled: true,
                ..base.clone()
            },
            &excluded
        ));
        assert!(!keep(
            &Raw {
                declined: true,
                ..base.clone()
            },
            &excluded
        ));
        assert!(!keep(
            &Raw {
                calendar_id: "hidden".into(),
                ..base.clone()
            },
            &excluded
        ));
        assert!(!keep(&Raw { end: 0, ..base }, &excluded));
    }

    #[test]
    fn countdown_keys_name_one_occurrence_of_a_series() {
        assert_eq!(countdown_key("abc", None), "abc");
        assert_eq!(
            countdown_key("abc", Some(1_793_318_400_123)),
            "abc@1793318400"
        );
        let single = event(
            Raw {
                local_id: "x".into(),
                start: 5,
                end: 9,
                ..Raw::default()
            },
            None,
        );
        assert_eq!(
            (single.id.as_str(), single.countdown_key.as_str()),
            ("x:5", "x")
        );
        assert_eq!(single.color, FALLBACK_COLOR);
        let unnamed = event(Raw::default(), None);
        assert_eq!(unnamed.countdown_key, "");
    }

    #[test]
    fn calendars_to_read_follows_the_exclusions() {
        let all = vec!["a".to_string(), "b".to_string()];
        assert_eq!(calendars_to_read(&all, &HashSet::new()), None);
        assert_eq!(
            calendars_to_read(&all, &["b".to_string()].into()),
            Some(vec!["a".to_string()])
        );
        assert_eq!(
            calendars_to_read(&all, &["a".to_string(), "b".to_string()].into()),
            Some(vec![])
        );
        // An exclusion for a calendar that no longer exists reads them all.
        assert_eq!(calendars_to_read(&all, &["gone".to_string()].into()), None);
    }

    #[test]
    fn locale_rules_map_to_the_page() {
        // LOCALE_IFIRSTDAYOFWEEK: 0 = Monday … 6 = Sunday.
        assert_eq!(weekday_from_monday(0), 1);
        assert_eq!(weekday_from_monday(6), 0);
        assert_eq!(weekday_from_monday(5), 6);
        assert_eq!(
            (minimum_days(0), minimum_days(1), minimum_days(2)),
            (1, 7, 4)
        );
        assert_eq!(color_hex(255, 92, 99, 255), "#ff5c63");
        assert_eq!(color_hex(1, 2, 3, 0), FALLBACK_COLOR);
    }

    #[test]
    fn the_section_switches_the_reader() {
        assert!(enabled(&serde_json::json!({})));
        assert!(!enabled(
            &serde_json::json!({ "island": { "sections": { "calendar": false } } })
        ));
        assert!(!enabled(
            &serde_json::json!({ "island": { "hiddenModules": ["calendar"] } })
        ));
        assert!(!enabled(
            &serde_json::json!({ "island": { "enabled": false } })
        ));
        let prefs = serde_json::json!({ "island": { "calendarExcluded": ["a", 3, "b"] } });
        assert_eq!(excluded(&prefs), ["a".to_string(), "b".to_string()].into());
    }

    #[test]
    fn requests_are_answered_without_a_reader() {
        let shared = Shared::default();
        assert_eq!(shared.ask(None), Snapshot::default());
    }
}
