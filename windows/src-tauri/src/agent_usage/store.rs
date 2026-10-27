// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Everything read so far, one record per response, with the turns agents
//! work on (AgentUsageStore.swift). Confined to the usage thread.

use super::model::{Billable, Event, Limits, LiveSession, Provider, Record, Time, Tokens};
use super::parse::{Entry, OPENCODE_SETTLE};
use super::pricing;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// A turn that ended longer ago than this is history found late, not news.
pub const LATE_END: f64 = 5.0 * 60.0;

/// How long a quiet turn waits for its work to resume before it is over.
pub fn resume_window(provider: Provider) -> f64 {
    if provider == Provider::Claude { 3600.0 } else { 6.0 * 3600.0 }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedRecord {
    pub key: String,
    pub record: Record,
    pub billable: Billable,
    pub sources: Vec<String>,
}

#[derive(Default)]
pub struct Store {
    pub records: Vec<Record>,
    billables: Vec<Billable>,
    /// The logs each response was read from.
    sources: Vec<Vec<String>>,
    index: HashMap<String, usize>,
    pub limits: HashMap<Provider, Limits>,
    pub codex_plan: Option<String>,
    codex_plan_observed: Time,
    /// Open turns by log file.
    pub turns: HashMap<String, LiveSession>,
    /// Turns gone quiet: not shown as working, but resumed if work goes on.
    pub waiting: HashMap<String, LiveSession>,
    /// Claude turns whose process was seen running.
    registered: HashSet<String>,
    /// Turns whose last step ended expecting more, with when.
    settled: HashMap<String, Time>,
    /// Off while the logs are first read, so history never replays as news.
    pub reports_transitions: bool,
}

/// The Claude sessions with a process, from `sessions/<pid>.json`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Registry {
    pub running: HashSet<String>,
    pub ended: HashSet<String>,
    /// False when a record could not be read, so absence proves nothing.
    pub complete: bool,
    /// A sessions folder could be listed.
    pub listed: bool,
}

impl Store {
    pub fn live(&self) -> Vec<LiveSession> {
        self.turns.values().cloned().collect()
    }

    /// Applies one file's entries and returns the turns they finished.
    /// `parent` is the log whose turn a subagent's responses count toward.
    #[allow(clippy::too_many_arguments)]
    pub fn apply(&mut self, entries: Vec<Entry>, file: &str, provider: Provider, tracks_turns: bool, parent: Option<&str>, modified: Time, now: Time) -> Vec<Event> {
        let mut events = Vec::new();
        for entry in entries {
            match entry {
                Entry::Usage { key, record, billable } => {
                    // Copilot's activity and session totals never measure the current task.
                    let turn = if provider == Provider::Copilot { None } else if tracks_turns { Some(file) } else { parent };
                    self.add(record, billable, key, file, turn, !tracks_turns);
                }
                Entry::UsageModel { key, model } => {
                    if let Some(&at) = self.index.get(&key)
                        && self.records[at].model.is_empty() {
                            self.records[at].model = model;
                        }
                }
                Entry::Limits(reading) => self.update_limits(reading),
                Entry::Plan(plan, date) => {
                    // An archived session read again holds an older plan.
                    if self.codex_plan_observed <= date {
                        self.codex_plan = Some(plan);
                        self.codex_plan_observed = date;
                    }
                }
                Entry::TurnBegan(date) => {
                    if !tracks_turns {
                        continue;
                    }
                    self.settled.remove(file);
                    // A log read again from its start keeps the turn it already holds.
                    if self.turns.get(file).or(self.waiting.get(file)).is_some_and(|t| (t.started - date).abs() < 1.0) {
                        continue;
                    }
                    self.waiting.remove(file);
                    let last = self.turns.get(file).map_or(date, |t| t.last_activity.max(date));
                    self.turns.insert(file.into(), LiveSession { id: file.into(), provider, started: date, last_activity: last, model: String::new(), project: String::new(), tokens: Tokens::default(), cost: 0.0 });
                }
                Entry::TurnContext { model, project } => {
                    if !tracks_turns {
                        continue;
                    }
                    if let Some(turn) = self.turns.get_mut(file).or(self.waiting.get_mut(file)) {
                        if !model.is_empty() {
                            turn.model = model;
                        }
                        if !project.is_empty() {
                            turn.project = project;
                        }
                    }
                }
                Entry::TurnActive(date) => {
                    if !tracks_turns {
                        continue;
                    }
                    self.settled.remove(file);
                    let moment = date.unwrap_or(modified);
                    match self.turns.remove(file).or_else(|| self.waiting.remove(file)) {
                        Some(mut turn) => {
                            turn.last_activity = turn.last_activity.max(moment);
                            self.turns.insert(file.into(), turn);
                        }
                        None => {
                            self.turns.insert(file.into(), LiveSession { id: file.into(), provider, started: moment, last_activity: moment, model: String::new(), project: String::new(), tokens: Tokens::default(), cost: 0.0 });
                        }
                    }
                }
                Entry::TurnSettled(date) => {
                    if !tracks_turns {
                        continue;
                    }
                    if let Some(mut turn) = self.turns.remove(file).or_else(|| self.waiting.remove(file)) {
                        turn.last_activity = turn.last_activity.max(date);
                        self.turns.insert(file.into(), turn);
                        self.settled.insert(file.into(), date);
                    }
                }
                Entry::TurnEnded { date, completed, duration } => {
                    if !tracks_turns {
                        continue;
                    }
                    self.settled.remove(file);
                    let quiet = self.waiting.remove(file);
                    let Some(turn) = self.turns.remove(file).or(quiet) else { continue };
                    if !completed || !self.reports_transitions {
                        continue;
                    }
                    let end = date.unwrap_or(modified);
                    if now - end > LATE_END {
                        continue;
                    }
                    events.push(Event::Finished { provider, duration: duration.unwrap_or(end - turn.started).max(0.0), cost: turn.cost, tokens: turn.tokens.total(), project: turn.project });
                }
                Entry::Reset => {
                    let base = file.split('#').next().unwrap_or(file).to_string();
                    self.forget_file(&base);
                }
            }
        }
        events
    }

    /// A log removed while its turn ran ends that turn without a notice.
    pub fn forget_file(&mut self, file: &str) -> bool {
        let nested = format!("{file}#");
        self.waiting.retain(|k, _| k != file && !k.starts_with(&nested));
        self.settled.retain(|k, _| k != file && !k.starts_with(&nested));
        let before = self.turns.len();
        self.turns.retain(|k, _| k != file && !k.starts_with(&nested));
        self.turns.len() != before
    }

    fn add(&mut self, record: Record, billable: Billable, key: String, source: &str, turn: Option<&str>, subagent: bool) {
        let mut delta = record.tokens;
        let mut extra = record.cost.unwrap_or(0.0);
        if let Some(&at) = self.index.get(&key) {
            if !self.sources[at].iter().any(|s| s == source) {
                self.sources[at].push(source.into());
            }
            let old = self.records[at].clone();
            let merged = old.tokens.merged(&record.tokens);
            if merged == old.tokens {
                // OpenCode can name what a reply cost only after its tokens.
                if record.provider == Provider::Opencode {
                    let (cost, reported) = if record.reported_cost { (record.cost, true) } else if old.reported_cost { (old.cost, true) } else { (record.cost.or(old.cost), false) };
                    if cost != old.cost || reported != old.reported_cost {
                        let extra = cost.unwrap_or(0.0) - old.cost.unwrap_or(0.0);
                        self.records[at].cost = cost;
                        self.records[at].reported_cost = reported;
                        if let Some(file) = turn
                            && let Some(t) = self.turns.get_mut(file).filter(|t| record.date >= t.started - 1.0) {
                                t.cost += extra;
                            }
                    }
                }
                return;
            }
            let mut combined = self.billables[at];
            combined.tokens = merged;
            combined.long_cache_write = combined.long_cache_write.max(billable.long_cache_write);
            combined.web_searches = combined.web_searches.max(billable.web_searches);
            combined.fast |= billable.fast;
            combined.domestic |= billable.domestic;
            combined.aggregate |= billable.aggregate;
            let priced = pricing::cost(&combined, &old.model);
            let (cost, reported) = if record.provider == Provider::Opencode {
                if record.reported_cost { (record.cost, true) } else if old.reported_cost { (old.cost, true) } else { (priced.0.or(record.cost).or(old.cost), false) }
            } else {
                (priced.0, false)
            };
            delta = merged.growth(&old.tokens);
            extra = cost.unwrap_or(0.0) - old.cost.unwrap_or(0.0);
            self.billables[at] = combined;
            let r = &mut self.records[at];
            r.tokens = merged;
            r.cost = cost;
            r.savings = priced.1;
            r.reported_cost = reported;
        } else {
            self.index.insert(key, self.records.len());
            self.records.push(record.clone());
            self.billables.push(billable);
            self.sources.push(vec![source.into()]);
        }
        let Some(file) = turn else { return };
        // A reply dated before its turn started leaves the turn where it was.
        if !self.turns.get(file).or(self.waiting.get(file)).is_some_and(|t| record.date >= t.started - 1.0) {
            return;
        }
        let Some(mut t) = self.turns.remove(file).or_else(|| self.waiting.remove(file)) else { return };
        t.tokens.add(&delta);
        t.cost += extra;
        t.last_activity = t.last_activity.max(record.date);
        if !subagent {
            if !record.model.is_empty() {
                t.model = record.model.clone();
            }
            if !record.project.is_empty() {
                t.project = record.project.clone();
            }
        }
        self.turns.insert(file.into(), t);
    }

    /// Prices every response again after a newer list arrives; reported costs stay.
    pub fn reprice(&mut self) {
        for (at, record) in self.records.iter_mut().enumerate() {
            if record.reported_cost {
                continue;
            }
            let priced = pricing::cost(&self.billables[at], &record.model);
            let recorded_zero = record.provider == Provider::Opencode && record.cost == Some(0.0);
            record.cost = priced.0.or(if recorded_zero { Some(0.0) } else { None });
            record.savings = priced.1;
        }
    }

    /// Keeps the newer of two readings of an account, whichever way each arrived.
    pub fn update_limits(&mut self, reading: Limits) {
        if self.limits.get(&reading.provider).is_none_or(|l| l.observed_at <= reading.observed_at) {
            self.limits.insert(reading.provider, reading);
        }
    }

    pub fn set_limits(&mut self, reading: Limits) {
        self.limits.insert(reading.provider, reading);
    }

    /// A turn silent for `idle` waits aside; one quiet past its window is over.
    pub fn close_idle_turns(&mut self, now: Time, idle: f64) {
        let quiet: Vec<String> = self.turns.iter().filter(|(_, t)| now - t.last_activity >= idle).map(|(k, _)| k.clone()).collect();
        for file in quiet {
            if let Some(turn) = self.turns.remove(&file) {
                self.waiting.insert(file, turn);
            }
        }
        self.waiting.retain(|_, t| now - t.last_activity < resume_window(t.provider));
    }

    /// A step that ended expecting more, with nothing after it, stopped there.
    pub fn close_settled_turns(&mut self, now: Time) -> bool {
        let done: Vec<String> = self.settled.iter().filter(|(_, d)| now - **d >= OPENCODE_SETTLE).map(|(k, _)| k.clone()).collect();
        let mut removed = false;
        for file in done {
            self.settled.remove(&file);
            self.waiting.remove(&file);
            removed |= self.turns.remove(&file).is_some();
        }
        removed
    }

    /// Ends the turns of Claude sessions whose process record says they are
    /// gone (spec §2.1.4). True when a turn was showing.
    pub fn close_ended_turns(&mut self, processes: &Registry, at_launch: bool) -> bool {
        self.registered.retain(|f| self.turns.contains_key(f));
        let claude: Vec<String> = self.turns.iter().filter(|(_, t)| t.provider == Provider::Claude).map(|(k, _)| k.clone()).collect();
        let mut closed = false;
        for file in claude {
            // The log's name is the session id, on either kind of path.
            let name = file.rsplit(['/', '\\']).next().unwrap_or(&file);
            let session = name.strip_suffix(".jsonl").unwrap_or(name).to_string();
            if processes.running.contains(&session) {
                self.registered.insert(file);
            } else if processes.ended.contains(&session) || (processes.complete && (self.registered.contains(&file) || (at_launch && processes.listed))) {
                self.registered.remove(&file);
                closed |= self.forget_file(&file);
            }
        }
        closed
    }

    /// Takes back what logs gone or rewritten gave: a response only they held
    /// goes, one another log also holds stays. Their turns end silently.
    pub fn forget_files(&mut self, files: &HashSet<String>) {
        for file in files {
            self.forget_file(file);
        }
        let mut emptied = false;
        for list in &mut self.sources {
            if list.iter().any(|s| files.contains(s)) {
                list.retain(|s| !files.contains(s));
                emptied |= list.is_empty();
            }
        }
        if emptied {
            let keep: Vec<bool> = self.sources.iter().map(|s| !s.is_empty()).collect();
            self.keep(&keep);
        }
    }

    /// The logs holding a response one of `files` also holds.
    pub fn files_sharing(&self, files: &HashSet<String>) -> HashSet<String> {
        self.sources.iter().filter(|l| l.iter().any(|s| files.contains(s))).flatten().cloned().collect()
    }

    pub fn known_files(&self) -> HashSet<String> {
        self.sources.iter().flatten().cloned().chain(self.turns.keys().cloned()).chain(self.waiting.keys().cloned()).collect()
    }

    /// Keeps memory bounded to the history the island shows.
    pub fn drop_records_before(&mut self, date: Time) {
        if self.records.iter().any(|r| r.date < date) {
            let keep: Vec<bool> = self.records.iter().map(|r| r.date >= date).collect();
            self.keep(&keep);
        }
    }

    /// Drops one provider's records, as when OpenCode's database restarts.
    pub fn drop_provider(&mut self, provider: Provider) {
        let keep: Vec<bool> = self.records.iter().map(|r| r.provider != provider).collect();
        self.keep(&keep);
        self.turns.retain(|_, t| t.provider != provider);
        self.waiting.retain(|_, t| t.provider != provider);
    }

    fn keep(&mut self, keep: &[bool]) {
        let mut positions = HashMap::new();
        let mut at = 0;
        let (records, billables, sources) = (std::mem::take(&mut self.records), std::mem::take(&mut self.billables), std::mem::take(&mut self.sources));
        for (i, ((r, b), s)) in records.into_iter().zip(billables).zip(sources).enumerate() {
            if keep[i] {
                positions.insert(i, at);
                self.records.push(r);
                self.billables.push(b);
                self.sources.push(s);
                at += 1;
            }
        }
        self.index = std::mem::take(&mut self.index).into_iter().filter_map(|(k, v)| positions.get(&v).map(|p| (k, *p))).collect();
    }

    /// What is kept between launches: every counter, none of the text, and
    /// nothing from OpenCode, whose database is read again at each launch.
    pub fn saved(&self) -> Saved {
        let mut keys = vec![String::new(); self.records.len()];
        for (k, &at) in &self.index {
            keys[at] = k.clone();
        }
        let records = (0..self.records.len())
            .filter(|&i| self.records[i].provider != Provider::Opencode)
            .map(|i| SavedRecord { key: keys[i].clone(), record: self.records[i].clone(), billable: self.billables[i], sources: self.sources[i].clone() })
            .collect();
        let mut limits: Vec<Limits> = self.limits.values().cloned().collect();
        limits.sort_by_key(|l| l.provider);
        let turns = |map: &HashMap<String, LiveSession>| {
            let mut v: Vec<LiveSession> = map.values().filter(|t| t.provider != Provider::Opencode).cloned().collect();
            v.sort_by(|a, b| a.id.cmp(&b.id));
            v
        };
        Saved { records, limits, codex_plan: self.codex_plan.clone(), codex_plan_observed: self.codex_plan_observed, turns: turns(&self.turns), waiting: turns(&self.waiting) }
    }

    pub fn from_saved(saved: Saved) -> Store {
        let mut store = Store::default();
        for (i, r) in saved.records.into_iter().enumerate() {
            store.index.insert(r.key, i);
            store.records.push(r.record);
            store.billables.push(r.billable);
            store.sources.push(r.sources);
        }
        store.limits = saved.limits.into_iter().map(|l| (l.provider, l)).collect();
        store.codex_plan = saved.codex_plan;
        store.codex_plan_observed = saved.codex_plan_observed;
        store.turns = saved.turns.into_iter().map(|t| (t.id.clone(), t)).collect();
        store.waiting = saved.waiting.into_iter().map(|t| (t.id.clone(), t)).collect();
        store
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub records: Vec<SavedRecord>,
    pub limits: Vec<Limits>,
    pub codex_plan: Option<String>,
    pub codex_plan_observed: Time,
    pub turns: Vec<LiveSession>,
    pub waiting: Vec<LiveSession>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(key: &str, date: Time, output: i64) -> Entry {
        let tokens = Tokens { output, ..Default::default() };
        Entry::Usage { key: key.into(), record: Record { provider: Provider::Claude, date, model: "claude-opus-5-5".into(), project: "p".into(), session: "s".into(), requests: 1, tokens, cost: Some(output as f64), savings: 0.0, reported_cost: false }, billable: Billable { tokens, ..Default::default() } }
    }

    #[test]
    fn streamed_duplicates_merge_and_a_finished_turn_is_reported_once() {
        let mut store = Store { reports_transitions: true, ..Default::default() };
        store.apply(vec![Entry::TurnBegan(100.0), usage("k", 110.0, 10), usage("k", 111.0, 40)], "a.jsonl", Provider::Claude, true, None, 120.0, 130.0);
        assert_eq!(store.records.len(), 1);
        assert_eq!(store.records[0].tokens.output, 40);
        assert_eq!(store.turns["a.jsonl"].tokens.output, 40);
        let events = store.apply(vec![Entry::TurnEnded { date: Some(160.0), completed: true, duration: None }], "a.jsonl", Provider::Claude, true, None, 160.0, 170.0);
        assert!(matches!(&events[..], [Event::Finished { duration, tokens: 40, .. }] if *duration == 60.0));
        assert!(store.turns.is_empty());
        // A turn that ended long ago is history, not news.
        store.apply(vec![Entry::TurnBegan(200.0)], "b.jsonl", Provider::Claude, true, None, 0.0, 0.0);
        let late = store.apply(vec![Entry::TurnEnded { date: Some(210.0), completed: true, duration: None }], "b.jsonl", Provider::Claude, true, None, 0.0, 210.0 + LATE_END + 1.0);
        assert!(late.is_empty());
    }

    #[test]
    fn quiet_turns_wait_and_resume_and_subagents_feed_their_parent() {
        let mut store = Store::default();
        store.apply(vec![Entry::TurnBegan(0.0)], "p.jsonl", Provider::Claude, true, None, 0.0, 0.0);
        store.close_idle_turns(700.0, 600.0);
        assert!(store.turns.is_empty() && store.waiting.contains_key("p.jsonl"));
        store.apply(vec![usage("sub", 800.0, 7)], "p/subagents/x.jsonl", Provider::Claude, false, Some("p.jsonl"), 800.0, 800.0);
        assert_eq!(store.turns["p.jsonl"].tokens.output, 7, "work resumed through the subagent");
        store.close_idle_turns(800.0 + 3601.0, 600.0);
        assert!(store.turns.is_empty() && store.waiting.is_empty());
    }

    #[test]
    fn forgetting_a_log_keeps_responses_another_log_also_holds() {
        let mut store = Store::default();
        store.apply(vec![usage("shared", 1.0, 1), usage("own", 2.0, 2)], "a", Provider::Claude, true, None, 0.0, 0.0);
        store.apply(vec![usage("shared", 1.0, 1)], "b", Provider::Claude, true, None, 0.0, 0.0);
        assert_eq!(store.files_sharing(&HashSet::from(["a".to_string()])), HashSet::from(["a".to_string(), "b".to_string()]));
        store.forget_files(&HashSet::from(["a".to_string()]));
        assert_eq!(store.records.len(), 1);
        assert_eq!(store.index["shared"], 0);
        let restored = Store::from_saved(store.saved());
        assert_eq!(restored.records, store.records);
    }

    #[test]
    fn the_registry_ends_a_killed_sessions_turn() {
        let mut store = Store::default();
        store.apply(vec![Entry::TurnBegan(0.0)], r"C:\Users\me\.claude\projects\x\abc.jsonl", Provider::Claude, true, None, 0.0, 0.0);
        let running = Registry { running: HashSet::from(["abc".into()]), complete: true, listed: true, ..Default::default() };
        assert!(!store.close_ended_turns(&running, false));
        let gone = Registry { complete: true, listed: true, ..Default::default() };
        assert!(store.close_ended_turns(&gone, false), "a record seen running is gone");
        assert!(store.turns.is_empty());
    }
}
