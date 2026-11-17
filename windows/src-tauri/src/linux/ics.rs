// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! iCalendar (`.ics`) files: the events in them and, for a repeating event,
//! the occurrences that fall in a span of time. Linux has no one store of
//! appointments to ask, so the calendar reads the files calendar programs and
//! sync tools keep (vdirsyncer's, Evolution's local ones).
//!
//! Covered: events with a start, an end or a duration, all-day events, UTC,
//! floating and `TZID` times (the zone database is GLib's), `RRULE` with
//! `DAILY`, `WEEKLY`, `MONTHLY` and `YEARLY`, `INTERVAL`, `COUNT`, `UNTIL`,
//! `BYDAY` (with an ordinal for months and years), `BYMONTHDAY` and `BYMONTH`,
//! `EXDATE`, and a changed occurrence (`RECURRENCE-ID`). Not covered: `RDATE`,
//! `BYSETPOS`, `BYWEEKNO` and the finer `BY…` parts.

use std::collections::HashMap;

use gtk::glib::{DateTime, TimeZone};

/// UTC, which GLib always has.
pub fn utc() -> TimeZone {
    TimeZone::from_identifier(Some("UTC")).expect("GLib has UTC")
}

/// The zone of the machine's clock.
fn local_zone() -> TimeZone {
    DateTime::now_local().map_or_else(|_| utc(), |now| now.timezone())
}

/// How many steps of a rule are tried, and occurrences kept, for one event.
const MAX_STEPS: u32 = 20_000;
const MAX_OCCURRENCES: usize = 2_000;
const DAY_MS: i64 = 86_400_000;

#[derive(Clone, Debug, PartialEq)]
pub enum Zone {
    Utc,
    /// No zone: the wall clock of whoever reads it.
    Floating,
    Named(String),
}

/// A date, or a date and time, as the file wrote it.
#[derive(Clone, Debug, PartialEq)]
pub struct Moment {
    pub year: i32,
    pub month: i32,
    pub day: i32,
    /// None for a date alone, which is an all-day event.
    pub time: Option<(i32, i32, i32)>,
    pub zone: Zone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub freq: Freq,
    pub interval: i64,
    pub count: Option<u32>,
    /// Epoch milliseconds, the last moment an occurrence may start.
    pub until: Option<i64>,
    /// An optional ordinal (`-1` last, `2` second) and a weekday (1 Monday ... 7 Sunday).
    pub by_day: Vec<(Option<i32>, i32)>,
    pub by_month_day: Vec<i32>,
    pub by_month: Vec<i32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Vevent {
    pub uid: String,
    pub summary: String,
    pub location: String,
    pub description: String,
    pub start: Moment,
    pub end: Option<Moment>,
    pub duration_ms: Option<i64>,
    pub rule: Option<Rule>,
    /// Starts (epoch milliseconds) of occurrences the series leaves out.
    pub excluded: Vec<i64>,
    /// For a changed occurrence of a series: the start it had in the series.
    pub recurrence_id: Option<i64>,
    pub cancelled: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Calendar {
    pub name: Option<String>,
    pub color: Option<String>,
    pub events: Vec<Vevent>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Occurrence {
    pub event: usize,
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    /// The start it has in its series, for one that repeats.
    pub original_start: Option<i64>,
}

// MARK: Reading the file

/// Lines with their folding undone (a line that continues starts with a space
/// or a tab).
fn unfolded(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for line in text.replace("\r\n", "\n").replace('\r', "\n").split('\n') {
        match (line.strip_prefix([' ', '\t']), lines.last_mut()) {
            (Some(rest), Some(last)) => last.push_str(rest),
            _ => lines.push(line.to_string()),
        }
    }
    lines
}

/// `NAME;PARAM=VALUE;...:value`, the name upper-cased, the parameters keyed by
/// their upper-cased name, and the value as written.
fn property(line: &str) -> Option<(String, HashMap<String, String>, String)> {
    // The first colon outside quotes ends the name and parameters.
    let mut quoted = false;
    let split = line.char_indices().find(|(_, c)| {
        if *c == '"' {
            quoted = !quoted;
        }
        *c == ':' && !quoted
    })?;
    let (head, value) = (&line[..split.0], &line[split.0 + 1..]);
    let mut parts = head.split(';');
    let name = parts.next()?.trim().to_ascii_uppercase();
    let parameters = parts
        .filter_map(|p| {
            let (key, value) = p.split_once('=')?;
            Some((
                key.to_ascii_uppercase(),
                value.trim_matches('"').to_string(),
            ))
        })
        .collect();
    Some((name, parameters, value.to_string()))
}

/// What a text value escapes: `\n`, `\,`, `\;` and `\\`.
fn unescaped(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

fn number(text: &str) -> Option<i32> {
    text.parse().ok()
}

/// `20260315`, `20260315T090000` or `20260315T090000Z`.
fn moment(value: &str, parameters: &HashMap<String, String>) -> Option<Moment> {
    let value = value.trim();
    let date = value.get(..8)?;
    let (year, month, day) = (
        number(&date[..4])?,
        number(&date[4..6])?,
        number(&date[6..8])?,
    );
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let all_day = parameters.get("VALUE").is_some_and(|v| v == "DATE") || value.len() == 8;
    if all_day {
        return Some(Moment {
            year,
            month,
            day,
            time: None,
            zone: Zone::Floating,
        });
    }
    let time = value.get(9..15)?;
    let (hour, minute, second) = (
        number(&time[..2])?,
        number(&time[2..4])?,
        number(&time[4..6])?,
    );
    let zone = if value.ends_with('Z') {
        Zone::Utc
    } else {
        parameters
            .get("TZID")
            .map_or(Zone::Floating, |z| Zone::Named(z.clone()))
    };
    Some(Moment {
        year,
        month,
        day,
        time: Some((hour, minute, second)),
        zone,
    })
}

/// `PT1H30M`, `P1D`, `P2W`: a length of time in milliseconds.
fn duration(value: &str) -> Option<i64> {
    let (negative, rest) = match value.trim().strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (
            false,
            value.trim().strip_prefix('+').unwrap_or(value.trim()),
        ),
    };
    let rest = rest.strip_prefix('P')?;
    let (mut total, mut digits, mut in_time) = (0i64, String::new(), false);
    for c in rest.chars() {
        match c {
            'T' => in_time = true,
            '0'..='9' => digits.push(c),
            unit => {
                let n: i64 = digits.parse().ok()?;
                digits.clear();
                total += n * match (unit, in_time) {
                    ('W', false) => 7 * DAY_MS,
                    ('D', false) => DAY_MS,
                    ('H', true) => 3_600_000,
                    ('M', true) => 60_000,
                    ('S', true) => 1_000,
                    _ => return None,
                };
            }
        }
    }
    digits
        .is_empty()
        .then_some(if negative { -total } else { total })
}

fn weekday(code: &str) -> Option<i32> {
    ["MO", "TU", "WE", "TH", "FR", "SA", "SU"]
        .iter()
        .position(|d| *d == code)
        .map(|i| i as i32 + 1)
}

fn rule(value: &str, local: &TimeZone) -> Option<Rule> {
    let parts: HashMap<String, &str> = value
        .split(';')
        .filter_map(|p| {
            let (key, value) = p.split_once('=')?;
            Some((key.to_ascii_uppercase(), value))
        })
        .collect();
    let freq = match *parts.get("FREQ")? {
        "DAILY" => Freq::Daily,
        "WEEKLY" => Freq::Weekly,
        "MONTHLY" => Freq::Monthly,
        "YEARLY" => Freq::Yearly,
        _ => return None,
    };
    let list = |key: &str| -> Vec<i32> {
        parts
            .get(key)
            .map(|v| v.split(',').filter_map(|n| number(n.trim())).collect())
            .unwrap_or_default()
    };
    let by_day = parts
        .get("BYDAY")
        .map(|v| {
            v.split(',')
                .filter_map(|d| {
                    let d = d.trim();
                    let (ordinal, code) = d.split_at(d.len().checked_sub(2)?);
                    let ordinal =
                        (!ordinal.is_empty()).then(|| number(ordinal.trim_start_matches('+')));
                    Some((ordinal.flatten(), weekday(code)?))
                })
                .collect()
        })
        .unwrap_or_default();
    let until = parts.get("UNTIL").and_then(|u| {
        let at = moment(u, &HashMap::new())?;
        // A date alone ends the day it names.
        let end_of_day = at.time.is_none();
        let ms = to_ms(&at, local)?;
        Some(if end_of_day { ms + DAY_MS - 1 } else { ms })
    });
    Some(Rule {
        freq,
        interval: parts
            .get("INTERVAL")
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(1),
        count: parts.get("COUNT").and_then(|v| v.parse().ok()),
        until,
        by_day,
        by_month_day: list("BYMONTHDAY"),
        by_month: list("BYMONTH"),
    })
}

/// Epoch milliseconds of a moment, in its own zone (a floating moment and a
/// date are in `local`).
pub fn to_ms(at: &Moment, local: &TimeZone) -> Option<i64> {
    let (hour, minute, second) = at.time.unwrap_or((0, 0, 0));
    let zone = match &at.zone {
        Zone::Utc => utc(),
        // A zone GLib does not know is read as the person's own, the better guess.
        Zone::Named(name) => TimeZone::from_identifier(Some(name)).unwrap_or_else(|| local.clone()),
        Zone::Floating => local.clone(),
    };
    let when = DateTime::new(
        &zone,
        at.year,
        at.month,
        at.day,
        hour,
        minute,
        f64::from(second),
    )
    .ok()?;
    Some(when.to_unix() * 1000)
}

/// The events of a calendar file.
pub fn parse(text: &str) -> Calendar {
    let local = local_zone();
    let mut calendar = Calendar::default();
    let mut current: Option<Vevent> = None;
    let mut depth = 0;
    for line in unfolded(text) {
        let Some((name, parameters, value)) = property(&line) else {
            continue;
        };
        // Where events begin and end, before what is inside one.
        match (name.as_str(), value.as_str()) {
            ("BEGIN", "VEVENT") if current.is_none() => {
                current = Some(Vevent {
                    uid: String::new(),
                    summary: String::new(),
                    location: String::new(),
                    description: String::new(),
                    start: Moment {
                        year: 0,
                        month: 0,
                        day: 0,
                        time: None,
                        zone: Zone::Floating,
                    },
                    end: None,
                    duration_ms: None,
                    rule: None,
                    excluded: Vec::new(),
                    recurrence_id: None,
                    cancelled: false,
                });
                depth = 0;
                continue;
            }
            ("END", "VEVENT") if depth == 0 && current.is_some() => {
                if let Some(event) = current.take().filter(|e| e.start.year != 0) {
                    calendar.events.push(event);
                }
                continue;
            }
            _ => {}
        }
        let Some(event) = current.as_mut() else {
            match name.as_str() {
                "X-WR-CALNAME" | "NAME" => calendar.name = Some(unescaped(&value)),
                "X-APPLE-CALENDAR-COLOR" | "COLOR" | "X-WR-CALCOLOR" => {
                    calendar.color = Some(value.trim().to_string());
                }
                _ => {}
            }
            continue;
        };
        // An alarm inside an event has properties of its own.
        match name.as_str() {
            "BEGIN" => depth += 1,
            "END" => depth = (depth - 1).max(0),
            _ if depth > 0 => {}
            "UID" => event.uid = value,
            "SUMMARY" => event.summary = unescaped(&value),
            "LOCATION" => event.location = unescaped(&value),
            "DESCRIPTION" => event.description = unescaped(&value),
            "DTSTART" => {
                if let Some(at) = moment(&value, &parameters) {
                    event.start = at;
                }
            }
            "DTEND" => event.end = moment(&value, &parameters),
            "DURATION" => event.duration_ms = duration(&value),
            "RRULE" => event.rule = rule(&value, &local),
            "EXDATE" => {
                for part in value.split(',') {
                    if let Some(at) = moment(part, &parameters).and_then(|m| to_ms(&m, &local)) {
                        event.excluded.push(at);
                    }
                }
            }
            "RECURRENCE-ID" => {
                event.recurrence_id = moment(&value, &parameters).and_then(|m| to_ms(&m, &local));
            }
            "STATUS" => event.cancelled = value.eq_ignore_ascii_case("CANCELLED"),
            _ => {}
        }
    }
    calendar
}

/// Epoch milliseconds of the start of the local day `days` after the one
/// `ms` is in (negative for before).
pub fn local_midnight(ms: i64, days: i64) -> Option<i64> {
    let local = DateTime::now_local().ok()?.timezone();
    let at = DateTime::from_unix_utc(ms.div_euclid(1000))
        .ok()?
        .to_timezone(&local)
        .ok()?;
    let (year, month, day) =
        civil_from_days(days_from_civil(at.year(), at.month(), at.day_of_month()) + days);
    to_ms(
        &Moment {
            year,
            month,
            day,
            time: Some((0, 0, 0)),
            zone: Zone::Floating,
        },
        &local,
    )
}

// MARK: Dates, without a calendar library

/// Days since 1970-01-01 of a civil date.
fn days_from_civil(year: i32, month: i32, day: i32) -> i64 {
    let y = i64::from(year) - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (i64::from(month) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i32, i32, i32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    ((y + i64::from(month <= 2)) as i32, month as i32, day as i32)
}

/// 1 Monday ... 7 Sunday.
fn weekday_of(days: i64) -> i32 {
    // 1970-01-01 was a Thursday.
    ((days + 3).rem_euclid(7) + 1) as i32
}

fn days_in_month(year: i32, month: i32) -> i32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        _ => 28,
    }
}

/// The days of a month a rule names: by month day (negative from the end),
/// by weekday (with an ordinal: the second Tuesday, the last Friday), or the
/// day the event started on.
fn month_days(year: i32, month: i32, rule: &Rule, started_on: i32) -> Vec<i64> {
    let length = days_in_month(year, month);
    let first = days_from_civil(year, month, 1);
    let mut days: Vec<i64> = Vec::new();
    if !rule.by_month_day.is_empty() {
        for d in &rule.by_month_day {
            let day = if *d < 0 { length + 1 + d } else { *d };
            if (1..=length).contains(&day) {
                days.push(first + i64::from(day) - 1);
            }
        }
        if !rule.by_day.is_empty() {
            // Both given: a month day that is also one of the weekdays.
            days.retain(|d| rule.by_day.iter().any(|(_, wd)| *wd == weekday_of(*d)));
        }
    } else if !rule.by_day.is_empty() {
        for (ordinal, wd) in &rule.by_day {
            let matching: Vec<i64> = (0..i64::from(length))
                .map(|i| first + i)
                .filter(|d| weekday_of(*d) == *wd)
                .collect();
            match ordinal {
                None => days.extend(matching),
                Some(n) if *n > 0 => days.extend(matching.get(*n as usize - 1)),
                Some(n) => days.extend(matching.iter().rev().nth((-*n) as usize - 1)),
            }
        }
    } else if started_on <= length {
        days.push(first + i64::from(started_on) - 1);
    }
    days
}

// MARK: The occurrences of an event

struct Span {
    /// Days an all-day event covers, or the length of a timed one.
    days: i64,
    ms: i64,
}

fn span(event: &Vevent, local: &TimeZone) -> Span {
    let start_ms = to_ms(&event.start, local).unwrap_or(0);
    if event.start.time.is_none() {
        let days = match &event.end {
            Some(end) => {
                days_from_civil(end.year, end.month, end.day)
                    - days_from_civil(event.start.year, event.start.month, event.start.day)
            }
            None => event.duration_ms.map_or(1, |d| (d / DAY_MS).max(1)),
        };
        return Span {
            days: days.max(1),
            ms: 0,
        };
    }
    let ms = match (&event.end, event.duration_ms) {
        (Some(end), _) => to_ms(end, local).map_or(0, |e| e - start_ms),
        (None, Some(duration)) => duration,
        (None, None) => 0,
    };
    Span {
        days: 0,
        ms: ms.max(0),
    }
}

/// A rule at work: its periods (a day, a week, a month, a year, `interval`
/// of them at a time) and the days each holds, from the event's own start.
struct Series<'a> {
    rule: &'a Rule,
    start_days: i64,
    year: i32,
    month: i32,
    day: i32,
}

impl<'a> Series<'a> {
    fn new(rule: &'a Rule, event: &Vevent) -> Self {
        Series {
            rule,
            start_days: days_from_civil(event.start.year, event.start.month, event.start.day),
            year: event.start.year,
            month: event.start.month,
            day: event.start.day,
        }
    }

    fn keeps(&self, day: i64) -> bool {
        let (year, month, dom) = civil_from_days(day);
        let rule = self.rule;
        let by_period = matches!(rule.freq, Freq::Monthly | Freq::Yearly);
        (rule.by_month.is_empty() || rule.by_month.contains(&month))
            && (rule.by_day.is_empty()
                || by_period
                || rule.by_day.iter().any(|(_, wd)| *wd == weekday_of(day)))
            && (rule.by_month_day.is_empty()
                || by_period
                || rule
                    .by_month_day
                    .iter()
                    .any(|d| *d == dom || (*d < 0 && days_in_month(year, month) + 1 + d == dom)))
    }

    /// The first day the period can hold, and the days it does, in order and
    /// no earlier than the event's start.
    fn period(&self, step: u32) -> (i64, Vec<i64>) {
        let rule = self.rule;
        let k = i64::from(step) * rule.interval;
        let (first, mut days) = match rule.freq {
            Freq::Daily => {
                let day = self.start_days + k;
                (day, vec![day])
            }
            Freq::Weekly => {
                let anchor = self.start_days + 7 * k;
                let monday = anchor - i64::from(weekday_of(anchor) - 1);
                let wanted: Vec<i32> = if rule.by_day.is_empty() {
                    vec![weekday_of(self.start_days)]
                } else {
                    rule.by_day.iter().map(|(_, wd)| *wd).collect()
                };
                (
                    monday,
                    wanted.iter().map(|wd| monday + i64::from(wd - 1)).collect(),
                )
            }
            Freq::Monthly => {
                let index = i64::from(self.year) * 12 + i64::from(self.month - 1) + k;
                let (year, month) = (index.div_euclid(12) as i32, index.rem_euclid(12) as i32 + 1);
                (
                    days_from_civil(year, month, 1),
                    month_days(year, month, rule, self.day),
                )
            }
            Freq::Yearly => {
                let year = self
                    .year
                    .saturating_add(k.min(i64::from(i32::MAX / 2)) as i32);
                let months = if rule.by_month.is_empty() {
                    vec![self.month]
                } else {
                    rule.by_month.clone()
                };
                (
                    days_from_civil(year, 1, 1),
                    months
                        .iter()
                        .flat_map(|m| month_days(year, *m, rule, self.day))
                        .collect(),
                )
            }
        };
        days.retain(|d| *d >= self.start_days && self.keeps(*d));
        days.sort_unstable();
        days.dedup();
        (first, days)
    }
}

/// The occurrences of one event that overlap `[from, to)`, as (start, end,
/// the start in its series) in milliseconds.
fn occurrences_of(
    event: &Vevent,
    from: i64,
    to: i64,
    skip: &[i64],
    local: &TimeZone,
) -> Vec<(i64, i64, Option<i64>)> {
    let Some(first_ms) = to_ms(&event.start, local) else {
        return Vec::new();
    };
    let span = span(event, local);
    let at = |day: i64| -> Option<(i64, i64)> {
        let (y, m, d) = civil_from_days(day);
        let moment = Moment {
            year: y,
            month: m,
            day: d,
            ..event.start.clone()
        };
        let start = to_ms(&moment, local)?;
        let end = if event.start.time.is_none() {
            let (ey, em, ed) = civil_from_days(day + span.days);
            to_ms(
                &Moment {
                    year: ey,
                    month: em,
                    day: ed,
                    ..moment
                },
                local,
            )?
        } else {
            start + span.ms
        };
        Some((start, end))
    };
    let overlaps = |start: i64, end: i64| end > from && start < to;
    let Some(rule) = &event.rule else {
        let (start, end) = if event.start.time.is_none() {
            at(days_from_civil(
                event.start.year,
                event.start.month,
                event.start.day,
            ))
            .unwrap_or((first_ms, first_ms + DAY_MS))
        } else {
            (first_ms, first_ms + span.ms)
        };
        return if overlaps(start, end) {
            vec![(start, end, event.recurrence_id)]
        } else {
            Vec::new()
        };
    };
    let mut found = Vec::new();
    let series = Series::new(rule, event);
    let mut produced = 0u32;
    'periods: for step in 0..MAX_STEPS {
        let (first, days) = series.period(step);
        // Nothing in a period that begins after the span, or in those after it.
        if at(first.max(series.start_days)).is_some_and(|(start, _)| start >= to) {
            break;
        }
        for day in days {
            produced += 1;
            if rule.count.is_some_and(|count| produced > count) {
                break 'periods;
            }
            let Some((start, end)) = at(day) else {
                continue;
            };
            if rule.until.is_some_and(|until| start > until) || start >= to {
                break 'periods;
            }
            if overlaps(start, end) && !event.excluded.contains(&start) && !skip.contains(&start) {
                found.push((start, end, Some(start)));
                if found.len() >= MAX_OCCURRENCES {
                    break 'periods;
                }
            }
        }
    }
    found
}

/// Every occurrence in `[from, to)` of every event that is not cancelled,
/// ordered by start; a changed occurrence of a series takes the place of the
/// one the series would have had.
pub fn occurrences(calendar: &Calendar, from: i64, to: i64) -> Vec<Occurrence> {
    let local = local_zone();
    let mut replaced: HashMap<&str, Vec<i64>> = HashMap::new();
    for event in &calendar.events {
        if let Some(original) = event.recurrence_id {
            replaced
                .entry(event.uid.as_str())
                .or_default()
                .push(original);
        }
    }
    let mut found: Vec<Occurrence> = Vec::new();
    for (index, event) in calendar.events.iter().enumerate() {
        if event.cancelled {
            continue;
        }
        let skip = if event.recurrence_id.is_none() {
            replaced
                .get(event.uid.as_str())
                .cloned()
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        for (start, end, original_start) in occurrences_of(event, from, to, &skip, &local) {
            found.push(Occurrence {
                event: index,
                start,
                end,
                all_day: event.start.time.is_none(),
                original_start,
            });
        }
    }
    found.sort_by_key(|o| (o.start, o.end, o.event));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(y: i32, m: i32, d: i32, h: i32, mi: i32) -> i64 {
        to_ms(
            &Moment {
                year: y,
                month: m,
                day: d,
                time: Some((h, mi, 0)),
                zone: Zone::Utc,
            },
            &utc(),
        )
        .unwrap()
    }

    fn calendar(body: &str) -> Calendar {
        parse(&format!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\n{body}END:VCALENDAR\r\n"
        ))
    }

    fn starts(calendar: &Calendar, from: i64, to: i64) -> Vec<i64> {
        occurrences(calendar, from, to)
            .iter()
            .map(|o| o.start)
            .collect()
    }

    #[test]
    fn civil_dates_round_trip_and_name_their_weekday() {
        for (y, m, d) in [
            (1970, 1, 1),
            (2000, 2, 29),
            (2026, 11, 6),
            (1999, 12, 31),
            (2100, 3, 1),
        ] {
            assert_eq!(civil_from_days(days_from_civil(y, m, d)), (y, m, d));
        }
        // 6 November 2026 is a Friday; 1 January 1970 a Thursday.
        assert_eq!(weekday_of(days_from_civil(2026, 11, 6)), 5);
        assert_eq!(weekday_of(0), 4);
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2100, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
    }

    #[test]
    fn folded_lines_escapes_and_durations_are_read() {
        let lines = unfolded("SUMMARY:One long\r\n  line\r\n\tends\nOTHER:x\r\n");
        assert_eq!(lines, ["SUMMARY:One long lineends", "OTHER:x", ""]);
        assert_eq!(unescaped(r"a\, b\; c\\ d\ne"), "a, b; c\\ d\ne");
        assert_eq!(duration("PT1H30M"), Some(5_400_000));
        assert_eq!(duration("P1D"), Some(DAY_MS));
        assert_eq!(duration("P2W"), Some(14 * DAY_MS));
        assert_eq!(duration("-PT15M"), Some(-900_000));
        assert_eq!(duration("1H"), None);
        assert_eq!(duration("PT5X"), None);
        let (name, parameters, value) =
            property("DTSTART;TZID=\"Europe/War:saw\";VALUE=DATE-TIME:20260315T090000").unwrap();
        assert_eq!(name, "DTSTART");
        assert_eq!(parameters["TZID"], "Europe/War:saw");
        assert_eq!(value, "20260315T090000");
        assert!(property("no colon here").is_none());
    }

    #[test]
    fn an_event_has_its_text_times_and_a_cancelled_one_is_left_out() {
        let calendar = calendar(
            "X-WR-CALNAME:Work\r\nX-APPLE-CALENDAR-COLOR:#FF0000\r\nBEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Stand\\, up\r\nLOCATION:Room 1\r\nDESCRIPTION:Line one\\nline two\r\nDTSTART:20260106T090000Z\r\nDTEND:20260106T093000Z\r\nBEGIN:VALARM\r\nSUMMARY:not the event's\r\nEND:VALARM\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:b\r\nSUMMARY:Called off\r\nDTSTART:20260106T100000Z\r\nDTEND:20260106T110000Z\r\nSTATUS:CANCELLED\r\nEND:VEVENT\r\n",
        );
        assert_eq!(calendar.name.as_deref(), Some("Work"));
        assert_eq!(calendar.color.as_deref(), Some("#FF0000"));
        assert_eq!(calendar.events.len(), 2);
        let event = &calendar.events[0];
        assert_eq!(
            (
                event.summary.as_str(),
                event.location.as_str(),
                event.description.as_str()
            ),
            ("Stand, up", "Room 1", "Line one\nline two")
        );
        let found = occurrences(&calendar, ms(2026, 1, 6, 0, 0), ms(2026, 1, 7, 0, 0));
        assert_eq!(found.len(), 1, "the cancelled one is out");
        assert_eq!(
            (found[0].start, found[0].end),
            (ms(2026, 1, 6, 9, 0), ms(2026, 1, 6, 9, 30))
        );
        assert!(!found[0].all_day);
        // Outside the span: nothing.
        assert!(occurrences(&calendar, ms(2026, 1, 7, 0, 0), ms(2026, 1, 8, 0, 0)).is_empty());
    }

    #[test]
    fn an_all_day_event_spans_its_days_in_the_local_zone() {
        let calendar = calendar(
            "BEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Trip\r\nDTSTART;VALUE=DATE:20260310\r\nDTEND;VALUE=DATE:20260313\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:b\r\nSUMMARY:Holiday\r\nDTSTART;VALUE=DATE:20260320\r\nEND:VEVENT\r\n",
        );
        let found = occurrences(&calendar, ms(2026, 3, 1, 0, 0), ms(2026, 4, 1, 0, 0));
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|o| o.all_day));
        // Three days, then one (a date alone is a day).
        assert_eq!((found[0].end - found[0].start) / 3_600_000 / 24, 3);
        assert_eq!((found[1].end - found[1].start) / 3_600_000 / 24, 1);
    }

    #[test]
    fn a_zone_names_the_hour_and_an_unknown_one_falls_back_to_local() {
        let warsaw = Moment {
            year: 2026,
            month: 7,
            day: 1,
            time: Some((12, 0, 0)),
            zone: Zone::Named("Europe/Warsaw".into()),
        };
        // Summer: two hours ahead of UTC.
        assert_eq!(to_ms(&warsaw, &utc()), Some(ms(2026, 7, 1, 10, 0)));
        let winter = Moment {
            month: 1,
            ..warsaw.clone()
        };
        assert_eq!(to_ms(&winter, &utc()), Some(ms(2026, 1, 1, 11, 0)));
        let nowhere = Moment {
            zone: Zone::Named("Not/AZone".into()),
            ..warsaw
        };
        assert_eq!(to_ms(&nowhere, &utc()), Some(ms(2026, 7, 1, 12, 0)));
    }

    #[test]
    fn a_daily_series_with_an_interval_a_count_and_an_exception() {
        let calendar = calendar(
            "BEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Daily\r\nDTSTART:20260101T080000Z\r\nDTEND:20260101T083000Z\r\nRRULE:FREQ=DAILY;INTERVAL=2;COUNT=5\r\nEXDATE:20260105T080000Z\r\nEND:VEVENT\r\n",
        );
        // 1, 3, 5, 7, 9 January: the 5th is skipped, and still counted.
        let days: Vec<i64> = starts(&calendar, ms(2025, 12, 1, 0, 0), ms(2026, 2, 1, 0, 0));
        assert_eq!(
            days,
            [
                ms(2026, 1, 1, 8, 0),
                ms(2026, 1, 3, 8, 0),
                ms(2026, 1, 7, 8, 0),
                ms(2026, 1, 9, 8, 0)
            ]
        );
        // A span inside the series finds its occurrences without starting from it.
        assert_eq!(
            starts(&calendar, ms(2026, 1, 6, 0, 0), ms(2026, 1, 8, 0, 0)),
            [ms(2026, 1, 7, 8, 0)]
        );
    }

    #[test]
    fn a_weekly_series_names_its_weekdays_and_stops_at_until() {
        let calendar = calendar(
            "BEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Gym\r\nDTSTART:20260105T180000Z\r\nDTEND:20260105T190000Z\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR;UNTIL=20260116T235959Z\r\nEND:VEVENT\r\n",
        );
        let days = starts(&calendar, ms(2026, 1, 1, 0, 0), ms(2026, 3, 1, 0, 0));
        // Mon 5, Wed 7, Fri 9, Mon 12, Wed 14, Fri 16.
        let wanted: Vec<i64> = [5, 7, 9, 12, 14, 16]
            .iter()
            .map(|d| ms(2026, 1, *d, 18, 0))
            .collect();
        assert_eq!(days, wanted);
        // Every other week, on the start's own weekday.
        let biweekly = calendar_of("RRULE:FREQ=WEEKLY;INTERVAL=2;COUNT=3");
        assert_eq!(
            starts(&biweekly, ms(2026, 1, 1, 0, 0), ms(2026, 3, 1, 0, 0)),
            [
                ms(2026, 1, 5, 18, 0),
                ms(2026, 1, 19, 18, 0),
                ms(2026, 2, 2, 18, 0)
            ]
        );
    }

    fn calendar_of(rule: &str) -> Calendar {
        calendar(&format!(
            "BEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Series\r\nDTSTART:20260105T180000Z\r\nDTEND:20260105T190000Z\r\n{rule}\r\nEND:VEVENT\r\n"
        ))
    }

    #[test]
    fn a_monthly_series_by_day_of_month_by_ordinal_weekday_and_from_the_end() {
        let jan_to_jun = (ms(2026, 1, 1, 0, 0), ms(2026, 7, 1, 0, 0));
        // The 31st: only the months that have one.
        let on_31 = calendar(
            "BEGIN:VEVENT\r\nUID:a\r\nSUMMARY:End\r\nDTSTART:20260131T100000Z\r\nDTEND:20260131T110000Z\r\nRRULE:FREQ=MONTHLY\r\nEND:VEVENT\r\n",
        );
        let found = starts(&on_31, jan_to_jun.0, jan_to_jun.1);
        assert_eq!(
            found,
            [
                ms(2026, 1, 31, 10, 0),
                ms(2026, 3, 31, 10, 0),
                ms(2026, 5, 31, 10, 0)
            ]
        );
        // The second Tuesday.
        let second_tuesday = calendar_of("RRULE:FREQ=MONTHLY;BYDAY=2TU;COUNT=3");
        assert_eq!(
            starts(&second_tuesday, jan_to_jun.0, jan_to_jun.1),
            [
                ms(2026, 1, 13, 18, 0),
                ms(2026, 2, 10, 18, 0),
                ms(2026, 3, 10, 18, 0)
            ]
        );
        // The last Friday, and the last day of the month.
        let last_friday = calendar_of("RRULE:FREQ=MONTHLY;BYDAY=-1FR;COUNT=2");
        assert_eq!(
            starts(&last_friday, jan_to_jun.0, jan_to_jun.1),
            [ms(2026, 1, 30, 18, 0), ms(2026, 2, 27, 18, 0)]
        );
        let last_day = calendar_of("RRULE:FREQ=MONTHLY;BYMONTHDAY=-1;COUNT=3");
        assert_eq!(
            starts(&last_day, jan_to_jun.0, jan_to_jun.1),
            [
                ms(2026, 1, 31, 18, 0),
                ms(2026, 2, 28, 18, 0),
                ms(2026, 3, 31, 18, 0)
            ]
        );
    }

    #[test]
    fn a_yearly_series_keeps_its_date_and_a_leap_day_waits_for_leap_years() {
        let birthday = calendar_of("RRULE:FREQ=YEARLY;COUNT=3");
        assert_eq!(
            starts(&birthday, ms(2026, 1, 1, 0, 0), ms(2030, 1, 1, 0, 0)),
            [
                ms(2026, 1, 5, 18, 0),
                ms(2027, 1, 5, 18, 0),
                ms(2028, 1, 5, 18, 0)
            ]
        );
        let leap = calendar(
            "BEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Leap\r\nDTSTART:20240229T120000Z\r\nDTEND:20240229T130000Z\r\nRRULE:FREQ=YEARLY\r\nEND:VEVENT\r\n",
        );
        assert_eq!(
            starts(&leap, ms(2024, 1, 1, 0, 0), ms(2033, 1, 1, 0, 0)),
            [
                ms(2024, 2, 29, 12, 0),
                ms(2028, 2, 29, 12, 0),
                ms(2032, 2, 29, 12, 0)
            ]
        );
        // Named months.
        let spring = calendar_of("RRULE:FREQ=YEARLY;BYMONTH=3,9;COUNT=3");
        assert_eq!(
            starts(&spring, ms(2026, 1, 1, 0, 0), ms(2029, 1, 1, 0, 0)),
            [
                ms(2026, 3, 5, 18, 0),
                ms(2026, 9, 5, 18, 0),
                ms(2027, 3, 5, 18, 0)
            ]
        );
    }

    #[test]
    fn a_changed_occurrence_takes_the_place_of_the_one_it_changes() {
        let calendar = calendar(
            "BEGIN:VEVENT\r\nUID:series\r\nSUMMARY:Sync\r\nDTSTART:20260105T100000Z\r\nDTEND:20260105T103000Z\r\nRRULE:FREQ=WEEKLY;COUNT=3\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:series\r\nSUMMARY:Sync (moved)\r\nRECURRENCE-ID:20260112T100000Z\r\nDTSTART:20260112T150000Z\r\nDTEND:20260112T153000Z\r\nEND:VEVENT\r\n",
        );
        let found = occurrences(&calendar, ms(2026, 1, 1, 0, 0), ms(2026, 2, 1, 0, 0));
        let starts: Vec<i64> = found.iter().map(|o| o.start).collect();
        // The 5th and 19th as the series has them, the 12th moved to the afternoon.
        assert_eq!(
            starts,
            [
                ms(2026, 1, 5, 10, 0),
                ms(2026, 1, 12, 15, 0),
                ms(2026, 1, 19, 10, 0)
            ]
        );
        let moved = found
            .iter()
            .find(|o| o.start == ms(2026, 1, 12, 15, 0))
            .unwrap();
        assert_eq!(moved.original_start, Some(ms(2026, 1, 12, 10, 0)));
        assert_eq!(calendar.events[moved.event].summary, "Sync (moved)");
    }

    #[test]
    fn what_cannot_be_read_is_skipped_and_a_runaway_rule_stops() {
        // No start, a bad date, an unknown frequency: no events and no panic.
        let broken = calendar(
            "BEGIN:VEVENT\r\nUID:x\r\nSUMMARY:No start\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:y\r\nDTSTART:20261301T000000Z\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:z\r\nDTSTART:20260101T000000Z\r\nDTEND:20260101T010000Z\r\nRRULE:FREQ=SECONDLY\r\nEND:VEVENT\r\n",
        );
        assert_eq!(broken.events.len(), 1);
        assert_eq!(
            starts(&broken, ms(2026, 1, 1, 0, 0), ms(2026, 2, 1, 0, 0)),
            [ms(2026, 1, 1, 0, 0)]
        );
        assert!(parse("garbage").events.is_empty());
        assert!(parse("").events.is_empty());
        // A series with no end, asked about a distant span, answers and returns.
        let endless = calendar_of("RRULE:FREQ=DAILY");
        let far = starts(&endless, ms(2036, 1, 1, 0, 0), ms(2036, 1, 4, 0, 0));
        assert_eq!(far.len(), 3);
    }
}
