// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The Calendar section's reader on Linux, where Windows asks its appointment
//! store. Linux has no one store, so the system's events are read from the
//! calendar files programs and sync tools keep (`ics.rs`):
//!
//! - `.ics` files in `~/.local/share/openbar/calendars` (one calendar each),
//! - Evolution's local calendars (`~/.local/share/evolution/calendar/*/calendar.ics`),
//! - vdirsyncer's and khal's folders (`~/.calendars`, and `~/.local/share/khal/calendars`),
//!   a folder of `.ics` files being one calendar, with its `displayname` and `color`.
//!
//! Events made in Openbar (`local_calendar`) are merged in as on Windows, so
//! they show even where no system calendar is found. `calendar.rs` includes
//! this file as `imp`, since it keeps its state private.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant, SystemTime};

use tauri::{Emitter, Manager};

use super::{
    Access, Choice, Event, Message, Raw, Shared, Snapshot, enabled, event as calendar_event,
    excluded, keep, merge_local, next_refresh, now_ms,
};
use crate::linux::ics::{self, Calendar};

const DAY_MS: i64 = 86_400_000;
/// How often the files are looked at for a change, besides the next event.
const FILES: Duration = Duration::from_secs(60);
const ACCESS_POLL: Duration = Duration::from_secs(30);
const QUARTER_HOUR: i64 = 15 * 60 * 1000;
const PALETTE: [&str; 5] = ["#59a6ff", "#ff5c63", "#78cf98", "#b896ef", "#ffb45b"];

/// Where calendars are looked for.
#[derive(Clone, Debug)]
pub struct Roots {
    /// Each `.ics` file in it is a calendar.
    files: Vec<PathBuf>,
    /// Each folder in it that holds `.ics` files is a calendar.
    folders: Vec<PathBuf>,
    /// Each folder in it holds a `calendar.ics`.
    evolution: Vec<PathBuf>,
}

impl Roots {
    fn of_user() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let data = crate::linux::paths::data().unwrap_or_else(|| home.join(".local/share"));
        Roots {
            files: vec![data.join("openbar/calendars")],
            folders: vec![home.join(".calendars"), data.join("khal/calendars")],
            evolution: vec![data.join("evolution/calendar")],
        }
    }
}

/// A calendar: one or several files, and what it is called.
#[derive(Clone, Debug, PartialEq)]
struct Source {
    id: String,
    name: String,
    color: Option<String>,
    files: Vec<PathBuf>,
}

fn ics_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ics")))
        .collect();
    files.sort();
    files
}

fn subfolders(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut folders: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    folders.sort();
    folders
}

fn text_file(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    Some(text.trim().to_string()).filter(|t| !t.is_empty())
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
}

/// Every calendar the roots hold.
fn discover(roots: &Roots) -> Vec<Source> {
    let mut found = Vec::new();
    for dir in &roots.files {
        for file in ics_files(dir) {
            found.push(Source {
                id: format!("ics:{}", file.display()),
                name: stem(&file),
                color: None,
                files: vec![file],
            });
        }
    }
    for dir in &roots.evolution {
        for folder in subfolders(dir) {
            let file = folder.join("calendar.ics");
            if file.is_file() {
                found.push(Source {
                    id: format!("ics:{}", file.display()),
                    name: folder
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                    color: None,
                    files: vec![file],
                });
            }
        }
    }
    for dir in &roots.folders {
        // A folder of events is a calendar; a folder of such folders is an account.
        let mut candidates = vec![dir.clone()];
        candidates.extend(subfolders(dir));
        candidates.extend(subfolders(dir).iter().flat_map(|d| subfolders(d)));
        for folder in candidates {
            let files = ics_files(&folder);
            if files.is_empty()
                || folder == *dir && roots.folders.iter().any(|f| f != dir && f.starts_with(dir))
            {
                continue;
            }
            found.push(Source {
                id: format!("ics:{}", folder.display()),
                name: text_file(&folder.join("displayname")).unwrap_or_else(|| {
                    folder
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
                }),
                color: text_file(&folder.join("color")),
                files,
            });
        }
    }
    found.sort_by(|a, b| a.id.cmp(&b.id));
    found.dedup_by(|a, b| a.id == b.id);
    found
}

/// `#rrggbb` out of what a file says (`#rrggbbaa` too), else None.
fn hex_color(text: &str) -> Option<String> {
    let text = text.trim();
    let digits = text.strip_prefix('#')?;
    (matches!(digits.len(), 6 | 8) && digits.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| format!("#{}", digits[..6].to_ascii_lowercase()))
}

fn palette(id: &str) -> String {
    let hash = id
        .bytes()
        .fold(5381u32, |h, b| h.wrapping_mul(33) ^ u32::from(b));
    PALETTE[hash as usize % PALETTE.len()].to_string()
}

/// What a file looked like when it was parsed, so an unchanged one is not read again.
type Stamp = (SystemTime, u64);

#[derive(Default)]
struct Cache {
    parsed: HashMap<PathBuf, (Stamp, Calendar)>,
}

impl Cache {
    fn calendar(&mut self, path: &Path) -> Option<&Calendar> {
        let meta = std::fs::metadata(path).ok()?;
        let stamp = (meta.modified().ok()?, meta.len());
        if self
            .parsed
            .get(path)
            .is_none_or(|(known, _)| *known != stamp)
        {
            let text = std::fs::read_to_string(path).ok()?;
            self.parsed
                .insert(path.to_path_buf(), (stamp, ics::parse(&text)));
        }
        self.parsed.get(path).map(|(_, calendar)| calendar)
    }
}

/// The calendars as the page lists them, and the events of each in `range`.
fn read(sources: &[Source], cache: &mut Cache, range: (i64, i64)) -> Vec<(Choice, Vec<Event>)> {
    sources
        .iter()
        .map(|source| {
            let mut merged = Calendar::default();
            for file in &source.files {
                if let Some(calendar) = cache.calendar(file) {
                    merged.name = merged.name.take().or_else(|| calendar.name.clone());
                    merged.color = merged.color.take().or_else(|| calendar.color.clone());
                    merged.events.extend(calendar.events.iter().cloned());
                }
            }
            let choice = Choice {
                id: source.id.clone(),
                title: if source.files.len() == 1 && source.name == stem(&source.files[0]) {
                    merged.name.clone().unwrap_or_else(|| source.name.clone())
                } else {
                    source.name.clone()
                },
                source_id: "ics".into(),
                source: "Calendar files".into(),
                color: source
                    .color
                    .as_deref()
                    .and_then(hex_color)
                    .or_else(|| merged.color.as_deref().and_then(hex_color))
                    .unwrap_or_else(|| palette(&source.id)),
            };
            let events = ics::occurrences(&merged, range.0, range.1)
                .into_iter()
                .map(|occurrence| {
                    let event = &merged.events[occurrence.event];
                    let raw = Raw {
                        local_id: event.uid.clone(),
                        calendar_id: source.id.clone(),
                        title: event.summary.clone(),
                        location: event.location.clone(),
                        start: occurrence.start,
                        end: occurrence.end,
                        all_day: occurrence.all_day,
                        canceled: false,
                        declined: false,
                        original_start: occurrence.original_start,
                    };
                    (raw, event.description.clone())
                })
                .map(|(raw, description)| {
                    let keep_it = keep(&raw, &Default::default());
                    let mut event = calendar_event(raw, Some(&choice));
                    event.description = description;
                    (keep_it, event)
                })
                .filter_map(|(keep_it, event)| keep_it.then_some(event))
                .collect();
            (choice, events)
        })
        .collect()
}

/// Wakes the reader when the clock jumps (the PC woke, or the time was set).
fn watch_clock(tx: Sender<Message>) {
    let _ = std::thread::Builder::new()
        .name("calendar-clock".into())
        .spawn(move || {
            let mut last = (Instant::now(), SystemTime::now());
            loop {
                std::thread::sleep(Duration::from_secs(5));
                let now = (Instant::now(), SystemTime::now());
                let ticked = now.0.duration_since(last.0).as_secs_f64();
                let wall = now
                    .1
                    .duration_since(last.1)
                    .map_or(-1.0, |d| d.as_secs_f64());
                if (wall - ticked).abs() > 10.0
                    && tx
                        .send(Message {
                            range: None,
                            reply: None,
                        })
                        .is_err()
                {
                    return;
                }
                last = now;
            }
        });
}

pub fn run<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    shared: Shared,
    tx: Sender<Message>,
    rx: Receiver<Message>,
) {
    watch_clock(tx);
    let roots = Roots::of_user();
    let mut cache = Cache::default();
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
        let (first_weekday, minimum_days) = crate::linux::locale::week_rules();
        let mut next = Snapshot {
            first_weekday,
            minimum_days_in_first_week: minimum_days,
            ..Snapshot::default()
        };
        let now = now_ms();
        let today = ics::local_midnight(now, 0).unwrap_or(now);
        let week = (
            today,
            ics::local_midnight(now, 7).unwrap_or(today + 7 * DAY_MS),
        );
        let hidden = excluded(&prefs);
        wake = Instant::now() + ACCESS_POLL;
        if enabled(&prefs) {
            let sources = discover(&roots);
            next.system_access = if sources.is_empty() {
                Access::Unavailable
            } else {
                Access::Allowed
            };
            let current = read(&sources, &mut cache, week);
            let shown = match visible {
                Some(range) if range != week => read(&sources, &mut cache, range),
                _ => current.clone(),
            };
            for ((choice, events), (_, shown_events)) in current.into_iter().zip(shown) {
                if !hidden.contains(&choice.id) {
                    next.current.extend(events);
                    next.events.extend(shown_events);
                }
                next.calendars.push(choice);
            }
            if !sources.is_empty() {
                wake = Instant::now() + FILES;
            }
        }
        merge_local(
            &mut next,
            app.state::<crate::local_calendar::Store>().load(),
            visible.unwrap_or(week),
            week,
            &hidden,
        );
        let at = next_refresh(
            &next.current,
            now,
            ics::local_midnight(now, 1).unwrap_or(now + QUARTER_HOUR),
        );
        wake = wake.min(Instant::now() + Duration::from_millis((at - now).max(1000) as u64));

        let changed = {
            let mut snapshot = shared.0.snapshot.lock().unwrap();
            let changed = *snapshot != next;
            *snapshot = next.clone();
            changed
        };
        if changed {
            let _ = app.emit(super::EVENT, &next);
        }
        for reply in replies {
            let _ = reply.send(next.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("openbar-cal-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn event(uid: &str, summary: &str, start: &str, end: &str) -> String {
        format!(
            "BEGIN:VEVENT\r\nUID:{uid}\r\nSUMMARY:{summary}\r\nDTSTART:{start}\r\nDTEND:{end}\r\nDESCRIPTION:About {summary}\r\nEND:VEVENT\r\n"
        )
    }

    fn calendar(events: &[String]) -> String {
        format!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\n{}END:VCALENDAR\r\n",
            events.concat()
        )
    }

    #[test]
    fn calendars_are_found_in_each_place_programs_keep_them() {
        let home = temp("discover");
        let one = calendar(&[event("1", "A", "20260105T090000Z", "20260105T100000Z")]);
        write(&home.join("data/openbar/calendars/Work.ics"), &one);
        write(
            &home.join("data/evolution/calendar/system/calendar.ics"),
            &one,
        );
        write(&home.join("data/evolution/calendar/empty/notes.txt"), "x");
        // vdirsyncer: an account folder holding calendar folders, one with a name and a color.
        write(&home.join("cal/account/family/e1.ics"), &one);
        write(&home.join("cal/account/family/e2.ics"), &one);
        write(&home.join("cal/account/family/displayname"), "Family\n");
        write(&home.join("cal/account/family/color"), "#FFAA0033\n");
        write(&home.join("cal/plain/e.ics"), &one);
        write(&home.join("cal/readme.txt"), "x");
        let roots = Roots {
            files: vec![home.join("data/openbar/calendars")],
            folders: vec![home.join("cal"), home.join("missing")],
            evolution: vec![home.join("data/evolution/calendar")],
        };
        let found = discover(&roots);
        let names: Vec<_> = found.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names.len(), 4, "{names:?}");
        for wanted in ["Work", "system", "Family", "plain"] {
            assert!(names.contains(&wanted), "{wanted} in {names:?}");
        }
        let family = found.iter().find(|s| s.name == "Family").unwrap();
        assert_eq!(family.files.len(), 2);
        assert_eq!(family.color.as_deref(), Some("#FFAA0033"));
        assert!(
            discover(&Roots {
                files: vec![],
                folders: vec![],
                evolution: vec![]
            })
            .is_empty()
        );
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn a_calendars_events_come_with_its_name_color_and_text() {
        let home = temp("read");
        let file = home.join("Work.ics");
        write(
            &file,
            &calendar(&[
                event("1", "Standup", "20260105T090000Z", "20260105T093000Z"),
                event("2", "Later", "20260301T090000Z", "20260301T093000Z"),
            ]),
        );
        let sources = [Source {
            id: format!("ics:{}", file.display()),
            name: "Work".into(),
            color: Some("#112233".into()),
            files: vec![file.clone()],
        }];
        let mut cache = Cache::default();
        let span = (
            ics::to_ms(
                &ics::Moment {
                    year: 2026,
                    month: 1,
                    day: 1,
                    time: Some((0, 0, 0)),
                    zone: ics::Zone::Utc,
                },
                &ics::utc(),
            )
            .unwrap(),
            ics::to_ms(
                &ics::Moment {
                    year: 2026,
                    month: 2,
                    day: 1,
                    time: Some((0, 0, 0)),
                    zone: ics::Zone::Utc,
                },
                &ics::utc(),
            )
            .unwrap(),
        );
        let read = read(&sources, &mut cache, span);
        assert_eq!(read.len(), 1);
        let (choice, events) = &read[0];
        assert_eq!(
            (choice.title.as_str(), choice.color.as_str()),
            ("Work", "#112233")
        );
        assert_eq!(events.len(), 1, "March is out of the span");
        let standup = &events[0];
        assert_eq!(
            (standup.title.as_str(), standup.description.as_str()),
            ("Standup", "About Standup")
        );
        assert_eq!(
            (standup.calendar.as_str(), standup.color.as_str()),
            ("Work", "#112233")
        );
        assert_eq!(standup.local_id, "1");
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn a_changed_file_is_read_again_and_an_unchanged_one_is_not() {
        let home = temp("cache");
        let file = home.join("a.ics");
        write(
            &file,
            &calendar(&[event("1", "One", "20260105T090000Z", "20260105T100000Z")]),
        );
        let mut cache = Cache::default();
        assert_eq!(cache.calendar(&file).unwrap().events.len(), 1);
        let first = cache.parsed[&file].0;
        assert_eq!(cache.calendar(&file).unwrap().events.len(), 1);
        assert_eq!(cache.parsed[&file].0, first);
        write(
            &file,
            &calendar(&[
                event("1", "One", "20260105T090000Z", "20260105T100000Z"),
                event("2", "Two", "20260106T090000Z", "20260106T100000Z"),
            ]),
        );
        assert_eq!(cache.calendar(&file).unwrap().events.len(), 2);
        assert!(cache.calendar(&home.join("gone.ics")).is_none());
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn colors_are_cut_to_six_digits_and_a_calendar_without_one_gets_a_steady_pick() {
        assert_eq!(hex_color("#FFAA0033").as_deref(), Some("#ffaa00"));
        assert_eq!(hex_color(" #12abEF ").as_deref(), Some("#12abef"));
        for bad in ["red", "#12", "123456", "#12345g", ""] {
            assert_eq!(hex_color(bad), None, "{bad}");
        }
        assert_eq!(palette("ics:/a"), palette("ics:/a"));
        assert!(PALETTE.contains(&palette("ics:/b").as_str()));
    }

    #[test]
    fn local_midnights_are_days_apart_and_day_starts() {
        let now = 1_800_000_000_000i64;
        let today = ics::local_midnight(now, 0).unwrap();
        let tomorrow = ics::local_midnight(now, 1).unwrap();
        assert!(today <= now && now < tomorrow);
        // A day is 23 to 25 hours, with the clocks.
        assert!((23 * 3_600_000..=25 * 3_600_000).contains(&(tomorrow - today)));
        assert_eq!(ics::local_midnight(now, -1).map(|y| y < today), Some(true));
    }
}
