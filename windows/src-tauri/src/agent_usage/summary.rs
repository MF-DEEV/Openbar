// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The snapshot the AI page draws (AgentUsageSummary.swift; spec 03 §8),
//! rebuilt from memory on each publish. Days and hours follow the local
//! calendar; dates leave as milliseconds since 1970, as `src/agents/types.ts`
//! expects them.

use super::model::{
    LimitSource, LimitWindow, Limits, LiveSession, Plan, Provider, Record, Time, Tokens, WindowKind,
};
use super::pricing;
use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

pub const DAY_COUNT: usize = 91;
pub const BLOCK_LENGTH: f64 = 5.0 * 3600.0;
const BLOCK_HISTORY: f64 = 24.0 * 3600.0;
const BURN_WINDOW: f64 = 30.0 * 60.0;

fn ms(t: Time) -> f64 {
    (t * 1000.0).round()
}
fn ms_opt(t: Option<Time>) -> Option<f64> {
    t.map(ms)
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub tokens: Tokens,
    pub cost: f64,
    pub savings: f64,
    pub requests: i64,
    pub unpriced: i64,
}

impl Totals {
    pub fn add(&mut self, r: &Record) {
        self.tokens.add(&r.tokens);
        self.requests += r.requests.max(0);
        self.savings += r.savings;
        match r.cost {
            Some(c) => self.cost += c,
            None => self.unpriced += 1,
        }
    }
    /// Cost when every response had a price, tokens otherwise.
    fn weight(&self, by_cost: bool) -> f64 {
        if by_cost {
            self.cost
        } else {
            self.tokens.total() as f64
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Share {
    pub id: String,
    pub name: String,
    pub provider: Option<Provider>,
    pub totals: Totals,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodUsage {
    pub total: Totals,
    pub by_provider: BTreeMap<Provider, Totals>,
    pub models: Vec<Share>,
    pub projects: Vec<Share>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    pub start: f64,
    pub by_provider: BTreeMap<Provider, Totals>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowOut {
    pub id: String,
    pub kind: WindowKind,
    pub minutes: Option<i64>,
    pub scope: Option<String>,
    pub used_percent: f64,
    pub resets_at: Option<f64>,
    pub estimated_resets_at: Option<f64>,
}

impl From<&LimitWindow> for WindowOut {
    fn from(w: &LimitWindow) -> WindowOut {
        WindowOut {
            id: w.id.clone(),
            kind: w.kind,
            minutes: w.minutes,
            scope: w.scope.clone(),
            used_percent: w.used_percent,
            resets_at: ms_opt(w.resets_at),
            estimated_resets_at: ms_opt(w.estimated_resets_at),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitsOut {
    pub provider: Provider,
    pub windows: Vec<WindowOut>,
    pub observed_at: f64,
    pub source: LimitSource,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanOut {
    pub name: String,
    pub monthly_price: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveOut {
    pub id: String,
    pub provider: Provider,
    pub started: f64,
    pub last_activity: f64,
    pub model: String,
    pub project: String,
    pub tokens: Tokens,
    pub cost: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockOut {
    pub start: f64,
    pub end: f64,
    pub totals: Totals,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeStatus {
    pub account_at: Option<f64>,
    pub app_at: Option<f64>,
    pub app_installed: bool,
    pub failure: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreditOut {
    pub id: String,
    pub expires_at: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetsOut {
    pub checking: bool,
    pub count: Option<i64>,
    pub credits: Vec<CreditOut>,
    pub failure: Option<&'static str>,
    pub checked_at: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Periods {
    pub today: PeriodUsage,
    pub week: PeriodUsage,
    pub month: PeriodUsage,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub loaded: bool,
    pub now: f64,
    pub periods: Periods,
    pub days: Vec<Bucket>,
    pub hours: Vec<Bucket>,
    pub limits: BTreeMap<Provider, LimitsOut>,
    pub plans: BTreeMap<Provider, PlanOut>,
    pub live: Vec<LiveOut>,
    pub claude_block: Option<BlockOut>,
    pub burn_rate: BTreeMap<Provider, Totals>,
    pub last_activity: BTreeMap<Provider, f64>,
    pub seen: Vec<Provider>,
    pub found: BTreeMap<Provider, bool>,
    pub claude: ClaudeStatus,
    pub resets: ResetsOut,
    pub price_day: String,
}

/// Local midnight of the day holding `t`.
pub fn local_day(t: Time) -> NaiveDate {
    DateTime::from_timestamp_millis((t * 1000.0) as i64)
        .map(|d| d.with_timezone(&Local).date_naive())
        .unwrap_or_default()
}

fn local_start(day: NaiveDate) -> Time {
    // The first moment of the day that exists, past a daylight-saving gap.
    let midnight = day.and_hms_opt(0, 0, 0).unwrap_or_default();
    match Local.from_local_datetime(&midnight).earliest() {
        Some(d) => d.timestamp() as f64,
        None => Local
            .from_local_datetime(&(midnight + Duration::hours(1)))
            .earliest()
            .map_or(0.0, |d| d.timestamp() as f64),
    }
}

/// The last start at or before `t`, by bisection.
fn index_of(t: Time, starts: &[Time]) -> Option<usize> {
    let first = *starts.first()?;
    if t < first {
        return None;
    }
    Some(starts.partition_point(|s| *s <= t) - 1)
}

pub struct Inputs<'a> {
    pub records: &'a [Record],
    pub limits: &'a HashMap<Provider, Limits>,
    pub live: Vec<LiveSession>,
    pub plans: &'a HashMap<Provider, Plan>,
    pub providers: &'a [Provider],
    pub now: Time,
}

pub fn snapshot(input: Inputs) -> Snapshot {
    let Inputs {
        records,
        limits,
        live,
        plans,
        providers,
        now,
    } = input;
    let on = |p: &Provider| providers.contains(p);
    let mut s = Snapshot {
        loaded: true,
        now: ms(now),
        ..Default::default()
    };
    for (p, l) in limits.iter().filter(|(p, _)| on(p)) {
        s.limits.insert(
            *p,
            LimitsOut {
                provider: *p,
                windows: l.windows.iter().map(WindowOut::from).collect(),
                observed_at: ms(l.observed_at),
                source: l.source,
            },
        );
    }
    for (p, plan) in plans.iter().filter(|(p, _)| on(p)) {
        s.plans.insert(
            *p,
            PlanOut {
                name: plan.name.clone(),
                monthly_price: plan.monthly_price,
            },
        );
    }
    let mut live: Vec<LiveSession> = live.into_iter().filter(|t| on(&t.provider)).collect();
    live.sort_by(|a, b| a.started.total_cmp(&b.started));
    let mut seen: std::collections::BTreeSet<Provider> = s
        .limits
        .keys()
        .copied()
        .chain(live.iter().map(|t| t.provider))
        .collect();
    s.live = live
        .iter()
        .map(|t| LiveOut {
            id: t.id.clone(),
            provider: t.provider,
            started: ms(t.started),
            last_activity: ms(t.last_activity),
            model: t.model.clone(),
            project: t.project.clone(),
            tokens: t.tokens,
            cost: t.cost,
        })
        .collect();

    let today = local_day(now);
    let starts: Vec<Time> = (0..DAY_COUNT)
        .rev()
        .map(|n| local_start(today - Duration::days(n as i64)))
        .collect();
    let tomorrow = local_start(today + Duration::days(1));
    // Local hours, so a daylight-saving day still splits at its clock hours.
    let hour_starts: Vec<Time> = (0..24u32)
        .map(|h| {
            today
                .and_hms_opt(h, 0, 0)
                .and_then(|t| Local.from_local_datetime(&t).earliest())
                .map_or(starts[DAY_COUNT - 1] + h as f64 * 3600.0, |d| {
                    d.timestamp() as f64
                })
        })
        .collect();
    let mut days: Vec<Bucket> = starts
        .iter()
        .map(|t| Bucket {
            start: ms(*t),
            by_provider: BTreeMap::new(),
        })
        .collect();
    let mut hours: Vec<Bucket> = hour_starts
        .iter()
        .map(|t| Bucket {
            start: ms(*t),
            by_provider: BTreeMap::new(),
        })
        .collect();
    let mut periods: [PeriodUsage; 3] = Default::default();
    let mut models: [HashMap<String, Share>; 3] = Default::default();
    let mut projects: [HashMap<String, Share>; 3] = Default::default();
    let mut names: HashMap<&str, String> = HashMap::new();
    let mut claude: Vec<&Record> = Vec::new();
    let recent = now - BURN_WINDOW;
    let last = DAY_COUNT - 1;
    const SPANS: [usize; 3] = [1, 7, 30];

    for r in records.iter().filter(|r| on(&r.provider)) {
        seen.insert(r.provider);
        let latest = s.last_activity.entry(r.provider).or_insert(0.0);
        *latest = latest.max(ms(r.date));
        if r.provider == Provider::Claude && r.date > now - BLOCK_HISTORY && r.date <= now {
            claude.push(r);
        }
        if r.date >= recent && r.date <= now {
            s.burn_rate.entry(r.provider).or_default().add(r);
        }
        if r.date >= tomorrow {
            continue;
        }
        let Some(day) = index_of(r.date, &starts) else {
            continue;
        };
        days[day].by_provider.entry(r.provider).or_default().add(r);
        if day == last
            && let Some(hour) = index_of(r.date, &hour_starts)
        {
            hours[hour]
                .by_provider
                .entry(r.provider)
                .or_default()
                .add(r);
        }
        let name = names
            .entry(&r.model)
            .or_insert_with(|| pricing::display_name(&r.model))
            .clone();
        let model_id = format!("{}:{name}", r.provider.key());
        for (i, span) in SPANS.iter().enumerate() {
            if day + span <= last {
                continue;
            }
            periods[i].total.add(r);
            periods[i].by_provider.entry(r.provider).or_default().add(r);
            models[i]
                .entry(model_id.clone())
                .or_insert_with(|| Share {
                    id: model_id.clone(),
                    name: if name.is_empty() {
                        "?".into()
                    } else {
                        name.clone()
                    },
                    provider: Some(r.provider),
                    totals: Totals::default(),
                })
                .totals
                .add(r);
            if !r.project.is_empty() {
                // Windows folder names compare without case: one project, its first spelling.
                projects[i]
                    .entry(r.project.to_lowercase())
                    .or_insert_with(|| Share {
                        id: r.project.clone(),
                        name: r.project.clone(),
                        provider: None,
                        totals: Totals::default(),
                    })
                    .totals
                    .add(r);
            }
        }
    }
    for i in 0..3 {
        let by_cost = periods[i].total.unpriced == 0;
        periods[i].models = sorted(
            std::mem::take(&mut models[i]).into_values().collect(),
            by_cost,
        );
        periods[i].projects = sorted(
            std::mem::take(&mut projects[i]).into_values().collect(),
            by_cost,
        );
    }
    let scale = 3600.0 / BURN_WINDOW;
    for rate in s.burn_rate.values_mut() {
        rate.cost *= scale;
        rate.savings *= scale;
        rate.tokens = rate.tokens.scaled(scale);
    }
    let [today_usage, week, month] = periods;
    s.periods = Periods {
        today: today_usage,
        week,
        month,
    };
    s.days = days;
    s.hours = hours;
    s.claude_block = current_block(&claude, now).map(|(start, end, totals)| BlockOut {
        start: ms(start),
        end: ms(end),
        totals,
    });
    s.seen = seen.into_iter().collect();
    s
}

fn sorted(mut shares: Vec<Share>, by_cost: bool) -> Vec<Share> {
    shares.sort_by(|a, b| {
        b.totals
            .weight(by_cost)
            .total_cmp(&a.totals.weight(by_cost))
            .then_with(|| a.name.cmp(&b.name))
    });
    shares
}

/// Claude's allowance renews five hours after the first request that
/// follows a renewal (§2.3).
pub fn current_block(records: &[&Record], now: Time) -> Option<(Time, Time, Totals)> {
    let mut ordered: Vec<&&Record> = records.iter().collect();
    ordered.sort_by(|a, b| a.date.total_cmp(&b.date));
    let mut block: Option<(Time, Time, Totals)> = None;
    for r in ordered {
        match block.as_mut() {
            Some((_, end, totals)) if r.date < *end => totals.add(r),
            _ => {
                let mut totals = Totals::default();
                totals.add(r);
                block = Some((r.date, r.date + BLOCK_LENGTH, totals));
            }
        }
    }
    block.filter(|(_, end, _)| *end > now)
}

/// Today's API value, for the daily budget.
pub fn today_cost(records: &[Record], providers: &[Provider], now: Time) -> f64 {
    let start = local_start(local_day(now));
    records
        .iter()
        .filter(|r| providers.contains(&r.provider) && r.date >= start && r.date <= now)
        .filter_map(|r| r.cost)
        .sum()
}

// MARK: Limits (AgentLimitSupport)

/// Windows that reached `threshold` between two readings of the same account.
pub fn crossings(previous: Option<&Limits>, current: &Limits, threshold: f64) -> Vec<LimitWindow> {
    if threshold <= 0.0 {
        return vec![];
    }
    let Some(previous) = previous else {
        return vec![];
    };
    current
        .windows
        .iter()
        .filter(|w| {
            if w.used_percent < threshold {
                return false;
            }
            let Some(before) = previous.windows.iter().find(|b| b.id == w.id) else {
                return false;
            };
            before.used_percent < threshold
                || renewed(before, w, current.observed_at)
                || (current.source == LimitSource::Account && began(w, previous.observed_at))
        })
        .cloned()
        .collect()
}

fn began(w: &LimitWindow, time: Time) -> bool {
    match (w.minutes, w.resets_at) {
        (Some(m), Some(r)) => r - m as f64 * 60.0 > time,
        _ => false,
    }
}

/// Usage fell, as a renewal or a reset used early leaves it, or the renewal
/// moved on once the old one went by.
pub fn renewed(before: &LimitWindow, w: &LimitWindow, observed_at: Time) -> bool {
    if w.used_percent + 1.0 < before.used_percent {
        return true;
    }
    match (before.resets_at, w.resets_at) {
        (Some(was), Some(resets)) => resets - was > 60.0 && observed_at >= was,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(
        provider: Provider,
        date: Time,
        model: &str,
        project: &str,
        cost: Option<f64>,
        output: i64,
    ) -> Record {
        Record {
            provider,
            date,
            model: model.into(),
            project: project.into(),
            session: "s".into(),
            requests: 1,
            tokens: Tokens {
                output,
                ..Default::default()
            },
            cost,
            savings: 0.0,
            reported_cost: false,
        }
    }

    #[test]
    fn periods_days_hours_and_shares() {
        let now = local_start(local_day(1_790_000_000.0)) + 12.0 * 3600.0;
        let records = vec![
            rec(
                Provider::Claude,
                now - 60.0,
                "claude-opus-5-5",
                "vors",
                Some(2.0),
                10,
            ),
            rec(
                Provider::Codex,
                now - 2.0 * 86_400.0,
                "gpt-5.6",
                "api",
                Some(1.0),
                5,
            ),
            rec(
                Provider::Codex,
                now - 20.0 * 86_400.0,
                "mystery",
                "API",
                None,
                7,
            ),
            rec(Provider::Copilot, now - 60.0, "x", "", Some(0.0), 0),
            rec(
                Provider::Claude,
                now - 100.0 * 86_400.0,
                "claude-opus-5-5",
                "old",
                Some(9.0),
                1,
            ),
        ];
        let limits = HashMap::new();
        let plans = HashMap::new();
        let providers = [Provider::Claude, Provider::Codex, Provider::Opencode];
        let s = snapshot(Inputs {
            records: &records,
            limits: &limits,
            live: vec![],
            plans: &plans,
            providers: &providers,
            now,
        });
        assert_eq!(s.days.len(), 91);
        assert_eq!(s.hours.len(), 24);
        assert_eq!(s.periods.today.total.cost, 2.0);
        assert_eq!(s.periods.week.total.cost, 3.0);
        assert_eq!(s.periods.month.total.unpriced, 1);
        assert_eq!(
            s.periods.week.models[0].name, "Opus 5.5",
            "a fully priced week sorts by cost"
        );
        assert_eq!(
            s.periods
                .month
                .models
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>(),
            ["claude:Opus 5.5", "codex:mystery", "codex:GPT-5.6"],
            "an unpriced month sorts by tokens"
        );
        assert_eq!(
            s.periods
                .month
                .projects
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["api", "vors"]
        );
        assert_eq!(
            s.seen,
            vec![Provider::Claude, Provider::Codex],
            "Copilot is switched off"
        );
        assert_eq!(
            s.days.last().unwrap().by_provider[&Provider::Claude].cost,
            2.0
        );
        assert_eq!(s.hours[11].by_provider[&Provider::Claude].cost, 2.0);
        assert!(s.claude_block.is_some());
        assert_eq!(
            s.burn_rate[&Provider::Claude].cost,
            4.0,
            "half an hour scaled to an hour"
        );
        let json = serde_json::to_value(&s).unwrap();
        assert!(
            json.pointer("/periods/week/byProvider/codex/tokens/cacheRead")
                .is_some()
        );
        assert!(json.get("claudeBlock").is_some() && json.get("priceDay").is_some());
    }

    #[test]
    fn claude_blocks_chain_five_hours_from_the_first_request() {
        let r = |t| rec(Provider::Claude, t, "claude-opus-5-5", "", Some(1.0), 1);
        let records = [r(0.0), r(3600.0), r(5.0 * 3600.0 + 10.0), r(6.0 * 3600.0)];
        let refs: Vec<&Record> = records.iter().collect();
        let (start, end, totals) = current_block(&refs, 7.0 * 3600.0).unwrap();
        assert_eq!(
            (start, end, totals.requests),
            (5.0 * 3600.0 + 10.0, 10.0 * 3600.0 + 10.0, 2)
        );
        assert!(current_block(&refs, 11.0 * 3600.0).is_none());
    }

    #[test]
    fn crossings_warn_once_and_again_after_a_renewal() {
        let window = |used: f64, resets: f64| {
            LimitWindow::new(
                "codex.300",
                WindowKind::Session,
                Some(300),
                None,
                used,
                Some(resets),
            )
        };
        let at = |w: LimitWindow, t: Time, source| Limits {
            provider: Provider::Codex,
            windows: vec![w],
            observed_at: t,
            source,
        };
        let a = at(window(70.0, 10_000.0), 100.0, LimitSource::SessionLog);
        let b = at(window(85.0, 10_000.0), 200.0, LimitSource::SessionLog);
        assert_eq!(crossings(Some(&a), &b, 80.0).len(), 1);
        assert!(
            crossings(Some(&b), &b, 80.0).is_empty(),
            "still above, not again"
        );
        assert!(crossings(None, &b, 80.0).is_empty());
        let renewed = at(window(82.0, 30_000.0), 10_100.0, LimitSource::SessionLog);
        assert_eq!(
            crossings(Some(&b), &renewed, 80.0).len(),
            1,
            "a renewed window that is again above warns"
        );
        assert!(super::renewed(&window(50.0, 0.0), &window(10.0, 0.0), 0.0));
    }
}
