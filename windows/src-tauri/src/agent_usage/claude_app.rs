// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The Claude desktop app's plan-usage history, a fallback for Claude's
//! limits (AgentClaudeAppUsage.swift; spec 03 §2.1.3). Best effort: whether
//! the Windows app writes `%APPDATA%\Claude\plan-usage-history.json` at all
//! is unverified (§13.4). The file holds percentages only, so renewals are
//! inferred from the readings.

use super::model::{LimitSource, LimitWindow, Limits, Provider, Time, WindowKind};
use std::collections::HashMap;
use std::path::PathBuf;

/// A reading younger than this is fresh.
pub const FRESHNESS: f64 = 30.0 * 60.0;
const MAXIMUM_SIZE: usize = 4 << 20;
const WINDOWS: [(&str, WindowKind, i64, Option<&str>); 4] = [("fh", WindowKind::Session, 300, None), ("sd", WindowKind::Weekly, 10_080, None), ("so", WindowKind::Weekly, 10_080, Some("Opus")), ("sn", WindowKind::Weekly, 10_080, Some("Sonnet"))];

pub fn history_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Claude").join("plan-usage-history.json"))
}

/// Whether the Claude desktop app is installed (its data or program folder).
pub fn installed() -> bool {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    env("LOCALAPPDATA").is_some_and(|l| l.join("AnthropicClaude").is_dir()) || env("APPDATA").is_some_and(|a| a.join("Claude").is_dir())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    pub date: Time,
    pub organization: Option<String>,
    pub used: HashMap<&'static str, f64>,
}

pub fn samples(bytes: &[u8]) -> Option<Vec<Sample>> {
    if bytes.len() > MAXIMUM_SIZE {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let version = json.get("version")?.as_i64()?;
    if version != 1 && version != 2 {
        return None;
    }
    let mut out = Vec::new();
    for entry in json.get("samples")?.as_array()? {
        let Some(ms) = entry.get("t").and_then(|t| t.as_f64()).filter(|t| t.is_finite() && *t > 0.0) else { continue };
        // The first version kept its windows beside the time.
        let values = if version == 1 { Some(entry) } else { entry.get("u") };
        let mut used = HashMap::new();
        for (key, ..) in WINDOWS {
            if let Some(n) = values.and_then(|v| v.get(key)).and_then(|n| n.as_f64()).filter(|n| n.is_finite()) {
                used.insert(key, n.clamp(0.0, 100.0));
            }
        }
        out.push(Sample { date: ms / 1000.0, organization: entry.get("org").and_then(|o| o.as_str()).map(str::to_string), used });
    }
    out.sort_by(|a, b| a.date.total_cmp(&b.date));
    Some(out)
}

fn readings<'a>(samples: &'a [Sample], organization: Option<&str>) -> Vec<&'a Sample> {
    samples.iter().filter(|s| organization.is_none_or(|o| s.organization.as_deref().is_none_or(|x| x == o))).collect()
}

/// The newest reading's date, for Settings' "Read from the Claude app".
pub fn last_check(samples: &[Sample], organization: Option<&str>) -> Option<Time> {
    readings(samples, organization).last().map(|s| s.date)
}

pub fn limits(samples: &[Sample], now: Time, session_start: Option<Time>, organization: Option<&str>) -> Option<Limits> {
    let all = readings(samples, organization);
    let latest = *all.last()?;
    if latest.date > now + 300.0 || now - latest.date >= 7.0 * 86_400.0 {
        return None;
    }
    let history: Vec<&Sample> = all.into_iter().filter(|s| s.organization == latest.organization).collect();
    let mut result = Vec::new();
    for (key, kind, minutes, scope) in WINDOWS {
        let Some(&used) = latest.used.get(key) else { continue };
        let length = minutes as f64 * 60.0;
        // An old reading says nothing about a session that renewed since.
        if kind == WindowKind::Session && now - latest.date >= length {
            continue;
        }
        let session = if kind == WindowKind::Session { session_end(&history, used, session_start, length) } else { None };
        let resets = if kind == WindowKind::Session { session.map(|s| s.0) } else { renewal(&history, key, length, latest.date) };
        if resets.is_some_and(|r| r <= now) || (resets.is_none() && now - latest.date >= 86_400.0) {
            continue;
        }
        let mut window = LimitWindow::new(format!("claude.{key}"), kind, Some(minutes), scope, used, resets);
        window.estimated_resets_at = session.and_then(|s| s.1);
        result.push(window);
    }
    (!result.is_empty()).then_some(Limits { provider: Provider::Claude, windows: result, observed_at: latest.date, source: LimitSource::ClaudeApp })
}

/// (latest possible end, estimate) of the session the latest reading is in.
fn session_end(history: &[&Sample], used: f64, start: Option<Time>, length: f64) -> Option<(Time, Option<Time>)> {
    if used <= 0.0 {
        return None;
    }
    let mut first = history.len().checked_sub(1)?;
    // The run of readings above zero ending with the latest one; a drop is a renewal too.
    while first > 0 {
        let Some(&before) = history[first - 1].used.get("fh") else { break };
        let here = history[first].used.get("fh").copied().unwrap_or(0.0);
        if !(before > 0.0 && before <= here + 1.0 && history[first].date - history[first - 1].date < length) {
            break;
        }
        first -= 1;
    }
    let latest = history[first].date;
    let earliest = if first > 0 { history[first - 1].date } else { latest - length };
    if let Some(start) = start.filter(|s| *s > earliest && *s <= latest) {
        return Some((start + length, None));
    }
    let estimate = if first > 0 && latest - earliest > FRESHNESS {
        let lower = earliest.max(latest - length);
        lower + (latest - lower) / 2.0
    } else {
        latest
    };
    Some((latest + length, (estimate < latest).then_some(estimate + length)))
}

#[derive(Clone, Copy, Debug)]
struct Drop {
    earliest: Time,
    latest: Time,
}

/// Renewals land on the UTC hour.
fn hour(t: Time) -> Time {
    (t / 3600.0).floor() * 3600.0
}

impl Drop {
    fn moment(&self) -> Time {
        let next = hour(self.earliest) + 3600.0;
        if next <= self.latest { next } else { self.latest }
    }
    fn overlap(&self, other: &Drop, shift: f64) -> Option<Drop> {
        let start = self.earliest.max(other.earliest + shift);
        let end = self.latest.min(other.latest + shift);
        (start < end).then_some(Drop { earliest: start, latest: end })
    }
}

/// A weekly renewal: a drop confirmed by another drop whole weeks away sets
/// the schedule; one that clearly passed without a drop undoes it.
fn renewal(history: &[&Sample], key: &str, length: f64, reading: Time) -> Option<Time> {
    let drops: Vec<Drop> = (1..history.len())
        .filter_map(|i| {
            let before = *history[i - 1].used.get(key)?;
            let after = *history[i].used.get(key)?;
            (after + 1.0 < before).then_some(Drop { earliest: history[i - 1].date, latest: history[i].date })
        })
        .collect();
    let mut moment = drops.last()?.moment();
    for index in (0..drops.len()).rev() {
        let spans: Vec<Drop> = (0..drops.len()).filter(|j| *j != index).filter_map(|j| confirmation(&drops[index], &drops[j], length)).collect();
        let Some(newest) = spans.last() else { continue };
        let agreed = spans.iter().try_fold(drops[index], |acc, span| acc.overlap(span, 0.0));
        let confirmed = agreed.unwrap_or(*newest).moment();
        if !missed(confirmed, history, &drops, key, length, reading) {
            moment = confirmed;
        }
        break;
    }
    while moment <= reading {
        moment += length;
    }
    Some(moment)
}

fn missed(moment: Time, history: &[&Sample], drops: &[Drop], key: &str, length: f64, reading: Time) -> bool {
    let mut next = moment + length;
    while next <= reading {
        let (start, end) = (next - 3600.0, next + 3600.0);
        let before = history.iter().rev().find(|s| s.date >= start && s.date < next && s.used.contains_key(key)).and_then(|s| s.used.get(key));
        let after = history.iter().find(|s| s.date > next && s.date <= end && s.used.contains_key(key)).and_then(|s| s.used.get(key));
        if let (Some(&before), Some(&after)) = (before, after)
            && before >= 10.0 && after + 1.0 >= before && !drops.iter().any(|d| d.latest >= start && d.earliest <= end) {
                return true;
            }
        next += length;
    }
    false
}

fn confirmation(drop: &Drop, other: &Drop, length: f64) -> Option<Drop> {
    if drop.latest - drop.earliest >= 86_400.0 || other.latest - other.earliest >= 86_400.0 {
        return None;
    }
    let weeks = ((drop.moment() - other.moment()) / length).round();
    if weeks == 0.0 {
        return None;
    }
    drop.overlap(other, weeks * length)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(samples: &[(f64, &str, f64, f64)]) -> Vec<Sample> {
        let entries: Vec<String> = samples.iter().map(|(t, org, fh, sd)| format!(r#"{{"t":{},"org":"{org}","u":{{"fh":{fh},"sd":{sd}}}}}"#, t * 1000.0)).collect();
        super::samples(format!(r#"{{"version":2,"samples":[{}]}}"#, entries.join(",")).as_bytes()).unwrap()
    }

    #[test]
    fn version_one_keeps_windows_beside_the_time() {
        let s = samples(br#"{"version":1,"samples":[{"t":1790000000000,"fh":12,"sd":150}]}"#).unwrap();
        assert_eq!(s[0].used["fh"], 12.0);
        assert_eq!(s[0].used["sd"], 100.0);
        assert!(samples(br#"{"version":3,"samples":[]}"#).is_none());
    }

    #[test]
    fn a_session_renews_five_hours_after_its_run_began() {
        let t0 = 1_790_000_000.0;
        let history = file(&[(t0, "o", 0.0, 40.0), (t0 + 600.0, "o", 5.0, 41.0), (t0 + 1200.0, "o", 9.0, 42.0), (t0 + 1800.0, "x", 99.0, 99.0)]);
        let limits = limits(&history, t0 + 1500.0, None, Some("o")).unwrap();
        let session = &limits.windows[0];
        assert_eq!(session.id, "claude.fh");
        assert_eq!(session.resets_at, Some(t0 + 600.0 + 5.0 * 3600.0), "the run started at the first non-zero reading");
        assert_eq!(limits.source, LimitSource::ClaudeApp);
        // Without a known renewal, a day-old week says nothing.
        assert!(limits.windows.iter().any(|w| w.id == "claude.sd"));
        assert!(super::limits(&history, t0 + 2.0 * 86_400.0, None, Some("o")).is_none());
        // Claude Code's own first request places the start exactly.
        let exact = super::limits(&history, t0 + 1500.0, Some(t0 + 300.0), Some("o")).unwrap();
        assert_eq!(exact.windows[0].resets_at, Some(t0 + 300.0 + 5.0 * 3600.0));
    }

    #[test]
    fn weekly_drops_a_week_apart_set_the_schedule() {
        let week = 7.0 * 86_400.0;
        let base = 1_789_000_000.0 - 1_789_000_000.0 % 3600.0;
        let history = file(&[(base - 1800.0, "o", 0.0, 80.0), (base + 1800.0, "o", 0.0, 2.0), (base + week - 1800.0, "o", 0.0, 70.0), (base + week + 1800.0, "o", 0.0, 1.0), (base + week + 7200.0, "o", 0.0, 5.0)]);
        let limits = limits(&history, base + week + 7300.0, None, Some("o")).unwrap();
        let weekly = limits.windows.iter().find(|w| w.id == "claude.sd").unwrap();
        assert_eq!(weekly.resets_at, Some(base + 2.0 * week));
    }
}
