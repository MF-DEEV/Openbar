// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The AI Agents section's usage service (AgentUsageService.swift; spec 03).
//! One background thread reads Claude Code, Codex, OpenCode and GitHub
//! Copilot logs incrementally, asks the vendor CLIs for plan limits, and
//! emits `agents://snapshot` on change and `agents://event` for notices.
//! Everything is local; the only network use is the daily price list.

mod archive;
mod claude;
mod claude_app;
mod cli;
mod codex;
mod files;
mod model;
// The database and network reads are Windows-only; their pure parts are tested everywhere.
#[cfg_attr(not(windows), allow(dead_code))]
mod opencode;
mod parse;
#[cfg_attr(not(windows), allow(dead_code))]
mod prices;
mod pricing;
mod store;
mod summary;

use files::{Cursor, Root};
use model::{Event, LimitWindow, Limits, Plan, Provider, Time};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, SystemTime};
use store::Store;
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// History covers 91 days (13 weeks).
const HORIZON: f64 = summary::DAY_COUNT as f64 * 86_400.0;
/// Logs written this recently, or holding a turn, are polled every 2 s:
/// NTFS reports an open file's appends late (§1.2).
const OPEN_LOG: f64 = 30.0 * 60.0;
const FAST: f64 = 2.0;
const TICK: f64 = 30.0;
const DISCOVERY: f64 = 10.0;
const SAVE: f64 = 5.0 * 60.0;

/// What the commands share with the reading thread.
#[derive(Default)]
pub struct Shared {
    snapshot: Mutex<Value>,
    control: Mutex<Control>,
    wake: Condvar,
}

#[derive(Default)]
struct Control {
    page_visible: bool,
    /// Some(force) when the Resets card asked for a check.
    resets: Option<bool>,
    redeem: Option<Sender<String>>,
}

pub type State = Arc<Shared>;
/// Reads the prefs, and hands an event to the island's windows.
type ReadPrefs = Box<dyn Fn() -> Value + Send>;
type Emit = Box<dyn Fn(&str, Value) + Send>;

fn poke(state: &Shared, change: impl FnOnce(&mut Control)) {
    change(&mut state.control.lock().unwrap());
    state.wake.notify_all();
}

#[tauri::command]
pub fn agents_snapshot(state: tauri::State<'_, State>) -> Value {
    let snapshot = state.snapshot.lock().unwrap().clone();
    if snapshot.is_null() { empty_snapshot() } else { snapshot }
}

/// The snapshot while nothing was read: a whole one, so the page never meets a missing field.
fn empty_snapshot() -> Value {
    let found = Provider::ALL.into_iter().map(|p| (p, files::found(p))).collect();
    serde_json::to_value(summary::Snapshot { found, ..Default::default() }).unwrap_or(Value::Null)
}

/// The AI page is on screen: Claude's limits are checked every minute.
#[tauri::command]
pub fn agents_page_visible(state: tauri::State<'_, State>, visible: bool) {
    poke(&state, |c| c.page_visible = visible);
}

#[tauri::command]
pub fn agents_resets_refresh(state: tauri::State<'_, State>, force: bool) {
    poke(&state, |c| c.resets = Some(force || c.resets == Some(true)));
}

/// Spends one banked Codex reset. Only the Resets card's confirmed button calls this.
#[tauri::command]
pub async fn agents_resets_redeem(state: tauri::State<'_, State>) -> Result<String, String> {
    let (tx, rx) = channel();
    poke(&state, |c| c.redeem = Some(tx));
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        rx.recv_timeout(Duration::from_secs(45)).unwrap_or_else(|_| {
            // Not started by now, it never starts: the card says it failed.
            shared.control.lock().unwrap().redeem = None;
            "failed".into()
        })
    })
    .await
    .map_err(|e| e.to_string())
}

pub fn start<R: Runtime>(app: AppHandle<R>, state: State) {
    std::thread::Builder::new()
        .name("agent-usage".into())
        .spawn(move || {
            let prefs_app = app.clone();
            let emit_app = app.clone();
            let mut service = Service::new(
                state,
                Box::new(move || prefs_app.state::<crate::prefs::Prefs>().get()),
                Box::new(move |name, payload| {
                    let _ = emit_app.emit(name, payload);
                }),
            );
            service.run();
        })
        .expect("agent usage thread");
}

/// `Vorssaint.exe --agents-probe <file> [--limits]`: reads every log once,
/// as the island does at launch, and writes the snapshot with where the logs
/// and CLIs were found. `--limits` also asks Claude Code and Codex for the
/// account limits. It never spends a reset and never writes the archive.
pub fn probe_main(path: &str, limits: bool) -> ! {
    let prefs = json!({"island": {"agentsPriceUpdates": false}});
    let state: State = Arc::default();
    let mut service = Service::new(state.clone(), Box::new(move || prefs.clone()), Box::new(|_, _| {}));
    service.persist = false;
    service.settings = Some(settings(&(service.prefs)()));
    let now = model::now();
    service.begin(files::roots(), now);
    let roots: Vec<Value> = files::roots().iter().map(|r| json!({"provider": r.provider, "path": r.path, "exists": r.path.is_dir()})).collect();
    let mut out = json!({
        "roots": roots,
        "logs": service.cursors.len(),
        "records": service.store.records.len(),
        "claudeExecutable": claude::executable(),
        "codexExecutable": codex::executable(),
        "archive": archive::folder(),
        "snapshot": state.snapshot.lock().unwrap().clone(),
    });
    if limits {
        out["claudeCheck"] = match claude::check(&model::now) {
            Ok(l) => json!(l.windows.iter().map(summary::WindowOut::from).collect::<Vec<_>>()),
            Err(f) => json!(f.key()),
        };
        out["codexCheck"] = match codex::check(&model::now) {
            Ok(s) => json!({"resets": s.available, "windows": s.limits.map(|l| l.windows.iter().map(summary::WindowOut::from).collect::<Vec<_>>())}),
            Err(f) => json!(f.key()),
        };
    }
    let written = std::fs::write(path, serde_json::to_vec_pretty(&out).unwrap_or_default());
    std::process::exit(if written.is_ok() { 0 } else { 1 })
}

/// The section's switches as the island reads them.
#[derive(Clone, Debug, PartialEq)]
struct Settings {
    enabled: bool,
    providers: Vec<Provider>,
    finish_minimum: Option<f64>,
    limit_threshold: Option<f64>,
    daily_budget: Option<f64>,
    price_updates: bool,
    /// Something shows or alerts on plan limits (NotchAgentSupport.limitsInUse).
    limits_without_page: bool,
    limits_card: bool,
}

fn settings(prefs: &Value) -> Settings {
    let island = |key: &str| prefs.pointer(&format!("/island/{key}"));
    let flag = |key: &str, default: bool| island(key).and_then(Value::as_bool).unwrap_or(default);
    let number = |key: &str, default: f64| island(key).and_then(Value::as_f64).filter(|n| n.is_finite()).unwrap_or(default);
    let hidden_modules = island("hiddenModules").and_then(Value::as_array).is_some_and(|v| v.iter().any(|m| m == "agents"));
    let enabled = flag("enabled", true) && prefs.pointer("/island/sections/agents").and_then(Value::as_bool).unwrap_or(true) && !hidden_modules;
    let mut providers: Vec<Provider> = Provider::ALL.into_iter().filter(|p| prefs.pointer(p.pref()).and_then(Value::as_bool).unwrap_or(true)).collect();
    if providers.is_empty() {
        providers = Provider::ALL.to_vec();
    }
    let limit_threshold = flag("agentsLimitAlert", true).then(|| number("agentsLimitThreshold", 80.0).clamp(1.0, 100.0));
    let live_limit = flag("agentsLiveActivity", true) && island("agentsReadout").and_then(Value::as_str) == Some("limit");
    let hidden_cards = island("agentsHiddenCards").and_then(Value::as_array).is_some_and(|v| v.iter().any(|c| c == "limits"));
    Settings {
        enabled,
        providers,
        finish_minimum: flag("agentsFinishAlert", true).then(|| number("agentsFinishMinimum", 60.0).clamp(0.0, 3600.0)),
        limit_threshold,
        daily_budget: Some(number("agentsDailyBudget", 0.0)).filter(|b| *b > 0.0),
        price_updates: flag("agentsPriceUpdates", true),
        limits_without_page: limit_threshold.is_some() || live_limit,
        limits_card: !hidden_cards,
    }
}

/// What finished in the background.
enum Job {
    Claude(Result<Limits, claude::Failure>),
    /// A check's answer, with the redeems that had begun when it started.
    Resets(Result<codex::Summary, codex::Failure>, u32),
    Redeem(Result<codex::Outcome, codex::Failure>, Option<codex::Summary>, Sender<String>),
    Prices(Option<pricing::PriceList>),
}

#[derive(Default)]
struct Resets {
    summary: Option<codex::Summary>,
    failure: Option<codex::Failure>,
    checking: bool,
    redeeming: bool,
    attempted: Time,
    /// An attempt that got no answer: its key and credit, kept for a retry.
    pending: Option<(String, Option<String>, Time)>,
    /// Redeems begun, so an older check's answer is set aside.
    redeems: u32,
}

struct Service {
    state: State,
    prefs: ReadPrefs,
    emit: Emit,
    jobs: (Sender<Job>, Receiver<Job>),
    settings: Option<Settings>,
    running: Option<Vec<Provider>>,
    roots: Vec<Root>,
    store: Store,
    cursors: HashMap<String, Cursor>,
    claude_plan: Option<Plan>,
    claude_org: Option<String>,
    claude_profile_modified: Option<SystemTime>,
    claude_account: Option<Limits>,
    claude_failure: Option<claude::Failure>,
    claude_streak: u32,
    /// When the last account check began; the next is due an interval later.
    claude_attempted: Time,
    claude_checking: bool,
    app_samples: Vec<claude_app::Sample>,
    app_modified: Option<SystemTime>,
    previous_limits: HashMap<Provider, Limits>,
    warned: HashMap<String, (Provider, LimitWindow)>,
    budget_day: Option<chrono::NaiveDate>,
    resets: Resets,
    price_due: Time,
    price_checking: bool,
    published: Option<Value>,
    /// Off for the probe, which must not touch the running island's archive.
    persist: bool,
    dirty: bool,
    last_tick: Time,
    last_discovery: Time,
    last_save: Time,
}

impl Service {
    fn new(state: State, prefs: ReadPrefs, emit: Emit) -> Service {
        Service {
            state,
            prefs,
            emit,
            jobs: channel(),
            settings: None,
            running: None,
            roots: vec![],
            store: Store::default(),
            cursors: HashMap::new(),
            claude_plan: None,
            claude_org: None,
            claude_profile_modified: None,
            claude_account: None,
            claude_failure: None,
            claude_streak: 0,
            claude_attempted: f64::NEG_INFINITY,
            claude_checking: false,
            app_samples: vec![],
            app_modified: None,
            previous_limits: HashMap::new(),
            warned: HashMap::new(),
            budget_day: None,
            resets: Resets::default(),
            price_due: 0.0,
            price_checking: false,
            published: None,
            persist: true,
            dirty: true,
            last_tick: 0.0,
            last_discovery: 0.0,
            last_save: 0.0,
        }
    }

    fn run(&mut self) {
        loop {
            self.step(model::now());
            let control = self.state.control.lock().unwrap();
            let _ = self.state.wake.wait_timeout(control, Duration::from_secs_f64(FAST)).unwrap();
        }
    }

    fn step(&mut self, now: Time) {
        let settings = settings(&(self.prefs)());
        if !settings.enabled {
            self.settings = Some(settings);
            if self.running.is_some() {
                self.stop(false);
            }
            self.answer_redeem("failed");
            return;
        }
        if self.running.as_ref() != Some(&settings.providers) {
            if self.running.is_some() {
                // A provider switched on is read from the start of its logs.
                self.stop(false);
            }
            self.settings = Some(settings.clone());
            self.begin(files::roots(), now);
        }
        self.settings = Some(settings);
        while let Ok(job) = self.jobs.1.try_recv() {
            self.finish(job, now);
        }
        self.poll(now);
        if now - self.last_tick >= TICK {
            self.tick(now);
        }
        self.schedule(now);
        if self.dirty {
            self.publish(now);
        }
        if now - self.last_save >= SAVE {
            self.save(now);
        }
    }

    fn providers(&self) -> &[Provider] {
        self.running.as_deref().unwrap_or(&[])
    }

    fn on(&self, provider: Provider) -> bool {
        self.providers().contains(&provider)
    }

    // MARK: Start and stop (§1.3)

    fn begin(&mut self, roots: Vec<Root>, now: Time) {
        let providers = self.settings.as_ref().map(|s| s.providers.clone()).unwrap_or_default();
        // The cached list's age sets the next download: once a day, not each launch.
        let cached_at = prices::cache_path().and_then(|p| std::fs::metadata(p).ok()).map(|m| files::modified(&m));
        self.price_due = self.price_due.max(cached_at.map_or(0.0, |t| t + prices::INTERVAL));
        // Prices first: the newer of the bundled and the cached list.
        if let Some(list) = pricing::PriceList::newer(pricing::PriceList::decode(pricing::BUNDLED.as_bytes()), prices::cached()) {
            pricing::install(list);
        }
        self.roots = roots.into_iter().filter(|r| providers.contains(&r.provider)).collect();
        self.running = Some(providers.clone());
        self.store = Store::default();
        self.cursors.clear();
        if let Some(saved) = archive::folder().filter(|_| self.persist).and_then(|d| archive::load(&d, &providers)) {
            self.store = Store::from_saved(saved.store);
            self.store.reprice();
            let mut lost: HashSet<String> = HashSet::new();
            for cursor in saved.cursors {
                let path = cursor.path.clone();
                match Cursor::restore(cursor) {
                    Some(c) => {
                        self.cursors.insert(path, c);
                    }
                    None => {
                        lost.insert(path);
                    }
                }
            }
            // Logs gone or rewritten give back their records; logs that
            // shared one with them are read again in full.
            let known: HashSet<String> = self.cursors.keys().cloned().collect();
            lost.extend(self.store.known_files().into_iter().filter(|f| !known.contains(f) && !f.contains('#')));
            let reread = self.store.files_sharing(&lost);
            self.store.forget_files(&reread);
            self.store.forget_files(&lost);
            for path in &reread {
                self.cursors.remove(path);
            }
        }
        self.store.drop_records_before(now - HORIZON);
        self.discover(now, true);
        self.store.close_idle_turns(now, parse::IDLE_TURN);
        let registry = files::registry(&self.roots);
        self.store.close_ended_turns(&registry, true);
        self.read_claude_plan();
        self.read_claude_app(now);
        // History never replays as notices.
        self.store.reports_transitions = true;
        self.previous_limits = self.store.limits.clone();
        if let Some(budget) = self.settings.as_ref().and_then(|s| s.daily_budget)
            && summary::today_cost(&self.store.records, self.providers(), now) >= budget {
                self.budget_day = Some(summary::local_day(now));
            }
        self.last_tick = now;
        self.last_discovery = now;
        self.dirty = true;
        self.publish(now);
        self.save(now);
    }

    /// Clears everything; switching the section off also deletes the archive.
    fn stop(&mut self, keep: bool) {
        if let Some(dir) = archive::folder() {
            if keep {
                self.save(model::now());
            } else if !self.settings.as_ref().is_some_and(|s| s.enabled) {
                archive::remove(&dir);
            }
        }
        self.running = None;
        self.store = Store::default();
        self.cursors.clear();
        self.claude_plan = None;
        self.claude_org = None;
        self.claude_profile_modified = None;
        self.claude_account = None;
        self.app_samples.clear();
        self.app_modified = None;
        self.previous_limits.clear();
        self.warned.clear();
        self.budget_day = None;
        self.published = None;
        let empty = empty_snapshot();
        *self.state.snapshot.lock().unwrap() = empty.clone();
        (self.emit)("agents://snapshot", empty);
    }

    fn save(&mut self, now: Time) {
        self.last_save = now;
        let Some(dir) = archive::folder().filter(|_| self.persist) else { return };
        let Some(providers) = self.running.clone() else { return };
        let cursors = self.cursors.values_mut().filter(|c| c.provider != Provider::Opencode && !c.restarted).filter_map(Cursor::saved).collect();
        let _ = archive::save(&dir, &archive::Archive { version: archive::PARSER_VERSION, providers, store: self.store.saved(), cursors });
    }

    // MARK: Reading

    /// Walks the roots for logs inside the horizon; new and grown ones are read.
    fn discover(&mut self, now: Time, all: bool) {
        self.last_discovery = now;
        for (path, provider) in files::discover(&self.roots, now - HORIZON) {
            let known = self.cursors.get(&path);
            let changed = known.is_none_or(|c| {
                all || std::fs::metadata(&path).is_ok_and(|m| files::modified(&m) != c.modified || (provider != Provider::Opencode && m.len() != c.offset))
            });
            if changed {
                self.read(&path, provider, now);
            }
        }
    }

    /// Logs written in the last half hour or holding a turn, every 2 s.
    fn poll(&mut self, now: Time) {
        if now - self.last_discovery >= DISCOVERY {
            self.discover(now, false);
            return;
        }
        let hot: Vec<(String, Provider)> = self
            .cursors
            .values()
            .filter(|c| now - c.modified < OPEN_LOG || self.store.turns.contains_key(&c.path) || self.store.turns.keys().any(|k| k.starts_with(&format!("{}#", c.path))))
            .map(|c| (c.path.clone(), c.provider))
            .collect();
        for (path, provider) in hot {
            self.read(&path, provider, now);
        }
    }

    fn read(&mut self, path: &str, provider: Provider, now: Time) {
        let mut cursor = self.cursors.remove(path).unwrap_or_else(|| Cursor::new(path, provider));
        let mut lines: Vec<(Vec<parse::Entry>, bool, String)> = Vec::new();
        let horizon = now - HORIZON;
        if provider == Provider::Opencode {
            opencode::read_appended(&mut cursor, horizon, now, &mut |c, line| {
                let entries = parse::parse(provider, line, &mut c.state, now);
                // A subagent session's usage counts toward its parent's turn.
                let file = if c.state.parent_session.is_empty() { format!("{}#{}", c.path, c.state.session) } else { format!("{}#{}", c.path, c.state.parent_session) };
                lines.push((entries, c.state.parent_session.is_empty(), file));
            });
        } else {
            let copilot = provider == Provider::Copilot;
            let keep = move |line: &[u8]| !copilot || parse::first_type(line, 0).is_some_and(|(t, _)| parse::COPILOT_EVENTS.contains(&t));
            let file = path.to_string();
            cursor.read_appended(&keep, &mut |state, line| {
                let entries = parse::parse(provider, line, state, now);
                if !entries.is_empty() {
                    lines.push((entries, true, file.clone()));
                }
            });
        }
        // A log read again from its start gives its responses again, which
        // count once by their keys; a restarted cursor is only never saved,
        // so the next launch reads that log as rewritten (as on the Mac).
        let modified = cursor.modified;
        let tracks = cursor.tracks_turns();
        let mut parent = cursor.parent.clone();
        if provider == Provider::Codex && !tracks {
            let count = self.cursors.len();
            parent = cursor.codex_parent(self.cursors.values().map(|c| (c.path.as_str(), &c.state, c.modified)), count);
        }
        self.cursors.insert(path.to_string(), cursor);
        let mut events = Vec::new();
        for (entries, tracks_line, file) in lines {
            let opencode_parent = (provider == Provider::Opencode && !tracks_line).then(|| file.clone());
            let (tracks, parent) = if provider == Provider::Opencode { (tracks_line, opencode_parent.as_deref()) } else { (tracks, parent.as_deref()) };
            if provider == Provider::Opencode && entries.iter().any(|e| matches!(e, parse::Entry::Reset)) {
                self.store.drop_provider(Provider::Opencode);
            }
            events.extend(self.store.apply(entries, &file, provider, tracks, parent, modified, now));
            self.dirty = true;
        }
        for event in events {
            self.report(event);
        }
        if self.dirty {
            self.check_limits();
        }
    }

    // MARK: Ticks

    fn tick(&mut self, now: Time) {
        self.last_tick = now;
        self.store.close_idle_turns(now, parse::IDLE_TURN);
        self.store.close_settled_turns(now);
        let registry = files::registry(&self.roots);
        self.store.close_ended_turns(&registry, false);
        self.store.drop_records_before(now - HORIZON);
        self.read_claude_plan();
        self.read_claude_app(now);
        self.report_renewals(now);
        self.check_limits();
        // A log removed while its turn ran ends that turn without a notice.
        let gone: Vec<String> = self.cursors.keys().filter(|p| !Path::new(p).exists()).cloned().collect();
        for path in gone {
            self.cursors.remove(&path);
            self.store.forget_file(&path);
        }
        // The clock moves the snapshot: days, the last half hour, Claude's block.
        self.dirty = true;
    }

    /// Starts background checks that are due.
    fn schedule(&mut self, now: Time) {
        let (page, resets_request, redeem) = {
            let mut c = self.state.control.lock().unwrap();
            (c.page_visible, c.resets.take(), c.redeem.take())
        };
        let Some(settings) = self.settings.clone() else { return };
        // Claude's account check (§2.2).
        let in_use = settings.limits_without_page || (page && settings.limits_card);
        // Measured from the last attempt with the page as it is now, so opening
        // the page brings a check due after a minute forward at once.
        let due = self.claude_attempted + claude::interval(self.claude_failure, self.claude_streak, page);
        if self.on(Provider::Claude) && in_use && !self.claude_checking && now >= due {
            self.claude_attempted = now;
            self.claude_checking = true;
            let tx = self.jobs.0.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Job::Claude(claude::check(&model::now)));
            });
        }
        // Codex's banked resets (§3.4).
        if let Some(force) = resets_request {
            let settled = matches!(self.resets.failure, None | Some(codex::Failure::NeedsSignIn | codex::Failure::Outdated));
            let fresh = now - self.resets.attempted < if settled { codex::FRESHNESS } else { codex::RETRY_AFTER };
            if self.on(Provider::Codex) && !self.resets.checking && !self.resets.redeeming && (force || !fresh) {
                self.resets.checking = true;
                self.resets.attempted = now;
                self.dirty = true;
                let tx = self.jobs.0.clone();
                let redeems = self.resets.redeems;
                std::thread::spawn(move || {
                    let _ = tx.send(Job::Resets(codex::check(&model::now), redeems));
                });
            }
        }
        if let Some(reply) = redeem {
            self.redeem(reply, now);
        }
        if settings.price_updates && !self.price_checking && now >= self.price_due {
            self.price_checking = true;
            let tx = self.jobs.0.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Job::Prices(prices::download()));
            });
        }
    }

    /// Spends the reset that expires first; a retry within ten minutes of an
    /// unanswered attempt reuses its key, so it never spends twice.
    fn redeem(&mut self, reply: Sender<String>, now: Time) {
        let available = self.resets.summary.as_ref().map_or(0, |s| s.available);
        if self.resets.redeeming || available <= 0 || !self.on(Provider::Codex) {
            let _ = reply.send("failed".into());
            return;
        }
        let first = self.resets.summary.as_ref().and_then(|s| s.credits.first().map(|c| c.id.clone()));
        let attempt = match self.resets.pending.take() {
            Some(pending) if now - pending.2 < codex::RETRY_WINDOW => pending,
            _ => (codex::uuid(), first, now),
        };
        self.resets.pending = Some(attempt.clone());
        self.resets.redeeming = true;
        self.resets.redeems += 1;
        self.dirty = true;
        let tx = self.jobs.0.clone();
        std::thread::spawn(move || {
            let (outcome, summary) = codex::redeem(attempt.1.as_deref(), &attempt.0, &model::now);
            let _ = tx.send(Job::Redeem(outcome, summary, reply));
        });
    }

    fn answer_redeem(&mut self, outcome: &str) {
        if let Some(reply) = self.state.control.lock().unwrap().redeem.take() {
            let _ = reply.send(outcome.into());
        }
    }

    fn finish(&mut self, job: Job, now: Time) {
        match job {
            Job::Claude(result) => {
                self.claude_checking = false;
                match result {
                    Ok(limits) => {
                        self.claude_failure = None;
                        self.claude_streak = 0;
                        self.claude_account = Some(limits);
                        self.read_claude_app(now);
                        self.check_limits();
                    }
                    Err(failure) => {
                        self.claude_streak = if failure == claude::Failure::Unreachable { self.claude_streak + 1 } else { 0 };
                        self.claude_failure = Some(failure);
                    }
                }
            }
            Job::Resets(result, redeems) => {
                self.resets.checking = false;
                // A check that began before a redeem would bring back the count it spent.
                match result {
                    _ if redeems != self.resets.redeems => {}
                    Ok(summary) => self.accept_resets(summary),
                    Err(failure) => self.fail_resets(failure),
                }
            }
            Job::Redeem(outcome, summary, reply) => {
                self.resets.redeeming = false;
                let answer = match outcome {
                    Ok(outcome) => {
                        self.resets.pending = None;
                        outcome.key()
                    }
                    // No outcome came back: the use may have happened on the
                    // way, so the attempt keeps its key.
                    Err(codex::Failure::Unreachable | codex::Failure::Refused) => "failed",
                    Err(failure) => {
                        self.resets.pending = None;
                        self.fail_resets(failure);
                        "failed"
                    }
                };
                let _ = reply.send(answer.into());
                match summary {
                    Some(summary) => self.accept_resets(summary),
                    None => poke(&self.state, |c| c.resets = Some(true)),
                }
            }
            Job::Prices(list) => {
                self.price_checking = false;
                match list {
                    Some(list) => {
                        self.price_due = now + prices::INTERVAL;
                        let current = pricing::list();
                        if let Some(newer) = pricing::PriceList::newer(Some((*current).clone()), Some(list))
                            && pricing::install(newer) {
                                self.store.reprice();
                            }
                    }
                    None => self.price_due = now + prices::RETRY,
                }
            }
        }
        self.dirty = true;
    }

    fn accept_resets(&mut self, summary: codex::Summary) {
        self.resets.failure = None;
        self.resets.attempted = summary.checked_at;
        if let Some(limits) = summary.limits.clone() {
            self.store.update_limits(limits);
            self.check_limits();
        }
        self.resets.summary = Some(summary);
    }

    fn fail_resets(&mut self, failure: codex::Failure) {
        self.resets.failure = Some(failure);
        // A lost or refused reply leaves the last count standing.
        if !matches!(failure, codex::Failure::Unreachable | codex::Failure::Refused) {
            self.resets.summary = None;
        }
    }

    // MARK: Claude's plan and limits

    /// Re-read only when `.claude.json` changes; only `oauthAccount` is kept.
    fn read_claude_plan(&mut self) {
        if !self.on(Provider::Claude) {
            return;
        }
        let path = claude::profile_path();
        let modified = path.as_ref().and_then(|p| std::fs::metadata(p).ok()).and_then(|m| m.modified().ok());
        if modified == self.claude_profile_modified && modified.is_some() {
            return;
        }
        self.claude_profile_modified = modified;
        let profile = claude::read_profile();
        if !profile.signed_in {
            self.claude_plan = None;
            self.claude_org = None;
            self.claude_account = None;
            self.store.limits.remove(&Provider::Claude);
            return;
        }
        if profile.organization != self.claude_org {
            self.claude_account = None;
            self.store.limits.remove(&Provider::Claude);
        }
        self.claude_org = profile.organization.clone();
        self.claude_plan = pricing::claude_plan(profile.organization_type.as_deref(), profile.rate_limit_tier.as_deref(), &pricing::list());
        self.dirty = true;
    }

    /// The Claude app's readings, placed at `now`; the account reading wins
    /// while it is under half an hour old.
    fn read_claude_app(&mut self, now: Time) {
        if !self.on(Provider::Claude) {
            return;
        }
        let path = claude_app::history_path();
        let modified = path.as_ref().and_then(|p| std::fs::metadata(p).ok()).and_then(|m| m.modified().ok());
        if modified != self.app_modified {
            self.app_modified = modified;
            self.app_samples = path.filter(|_| modified.is_some()).and_then(|p| std::fs::read(p).ok()).and_then(|b| claude_app::samples(&b)).unwrap_or_default();
        }
        let start = claude_app::last_check(&self.app_samples, self.claude_org.as_deref()).and_then(|reading| {
            let recent: Vec<&model::Record> = self.store.records.iter().filter(|r| r.provider == Provider::Claude && r.date <= reading && reading - r.date < 86_400.0).collect();
            summary::current_block(&recent, reading).map(|b| b.0)
        });
        let desktop = claude_app::limits(&self.app_samples, now, start, self.claude_org.as_deref());
        let account = claude::current(self.claude_account.as_ref(), now);
        let recent = account.clone().filter(|a| now - a.observed_at < claude_app::FRESHNESS);
        let latest = recent.or_else(|| [desktop, account].into_iter().flatten().max_by(|a, b| a.observed_at.total_cmp(&b.observed_at)));
        match latest {
            Some(limits) => self.store.set_limits(limits),
            None => {
                self.store.limits.remove(&Provider::Claude);
            }
        }
        self.dirty = true;
    }

    // MARK: Alerts (§9)

    fn check_limits(&mut self) {
        if !self.store.reports_transitions {
            return;
        }
        let threshold = self.settings.as_ref().and_then(|s| s.limit_threshold).unwrap_or(80.0);
        let mut events = Vec::new();
        for (provider, limits) in &self.store.limits {
            for window in summary::crossings(self.previous_limits.get(provider), limits, threshold) {
                self.warned.insert(window.id.clone(), (*provider, window.clone()));
                events.push(Event::LimitWarning { provider: *provider, window });
            }
        }
        // A banked reset renews a warned window before its time.
        let renewed: Vec<String> = self
            .warned
            .iter()
            .filter(|(id, (provider, before))| self.store.limits.get(provider).is_some_and(|reading| reading.windows.iter().any(|w| &w.id == *id && w.used_percent < threshold && summary::renewed(before, w, reading.observed_at))))
            .map(|(id, _)| id.clone())
            .collect();
        for id in renewed {
            if let Some((provider, _)) = self.warned.remove(&id)
                && let Some(window) = self.store.limits.get(&provider).and_then(|l| l.windows.iter().find(|w| w.id == id)).cloned() {
                    events.push(Event::LimitReset { provider, window });
                }
        }
        self.previous_limits = self.store.limits.clone();
        for event in events {
            self.report(event);
        }
    }

    fn report_renewals(&mut self, now: Time) {
        let due: Vec<String> = self.warned.iter().filter(|(_, (_, w))| w.resets_at.is_some_and(|r| r <= now)).map(|(id, _)| id.clone()).collect();
        for id in due {
            if let Some((provider, window)) = self.warned.remove(&id) {
                self.report(Event::LimitReset { provider, window });
            }
        }
    }

    /// Filters by the person's choices, then hands the event to the island.
    fn report(&self, event: Event) {
        let Some(settings) = &self.settings else { return };
        let payload = match event {
            Event::Finished { provider, duration, cost, tokens, project } => {
                let Some(minimum) = settings.finish_minimum.filter(|m| duration >= *m && self.on(provider)) else { return };
                let _ = minimum;
                json!({"kind": "finished", "provider": provider, "duration": duration, "cost": cost, "tokens": tokens, "project": project})
            }
            Event::LimitWarning { provider, window } | Event::LimitReset { provider, window } if settings.limit_threshold.is_none() || !self.on(provider) => {
                let _ = (provider, window);
                return;
            }
            Event::LimitWarning { provider, window } => json!({"kind": "limitWarning", "provider": provider, "window": summary::WindowOut::from(&window)}),
            Event::LimitReset { provider, window } => json!({"kind": "limitReset", "provider": provider, "window": summary::WindowOut::from(&window)}),
            Event::BudgetReached { spent, budget } => {
                if settings.daily_budget.is_none() {
                    return;
                }
                json!({"kind": "budgetReached", "spent": spent, "budget": budget})
            }
        };
        (self.emit)("agents://event", payload);
    }

    // MARK: Publishing

    fn snapshot(&self, now: Time) -> summary::Snapshot {
        let mut plans = HashMap::new();
        if let Some(plan) = &self.claude_plan {
            plans.insert(Provider::Claude, plan.clone());
        }
        if let Some(plan) = pricing::codex_plan(self.store.codex_plan.as_deref(), &pricing::list()) {
            plans.insert(Provider::Codex, plan);
        }
        let mut s = summary::snapshot(summary::Inputs { records: &self.store.records, limits: &self.store.limits, live: self.store.live(), plans: &plans, providers: self.providers(), now });
        s.found = Provider::ALL.into_iter().map(|p| (p, files::found(p))).collect();
        s.claude = summary::ClaudeStatus {
            account_at: self.claude_account.as_ref().map(|l| (l.observed_at * 1000.0).round()),
            app_at: claude_app::last_check(&self.app_samples, self.claude_org.as_deref()).map(|t| (t * 1000.0).round()),
            app_installed: claude_app::installed(),
            failure: self.claude_failure.map(claude::Failure::key),
        };
        let r = &self.resets;
        s.resets = summary::ResetsOut {
            checking: r.checking || r.redeeming,
            count: r.summary.as_ref().map(|s| s.available),
            credits: r.summary.as_ref().map(|s| s.credits.iter().map(|c| summary::CreditOut { id: c.id.clone(), expires_at: c.expires_at.map(|t| (t * 1000.0).round()) }).collect()).unwrap_or_default(),
            failure: r.failure.map(codex::Failure::key),
            checked_at: r.summary.as_ref().map(|s| (s.checked_at * 1000.0).round()),
        };
        s.price_day = pricing::list().updated.clone();
        s
    }

    fn publish(&mut self, now: Time) {
        self.dirty = false;
        let snapshot = self.snapshot(now);
        self.check_budget(snapshot.periods.today.total.cost, now);
        let mut value = serde_json::to_value(&snapshot).unwrap_or(Value::Null);
        // The time it was built alone is no change.
        let stamp = value.as_object_mut().and_then(|o| o.remove("now"));
        if self.published.as_ref() == Some(&value) {
            return;
        }
        self.published = Some(value.clone());
        if let (Some(object), Some(stamp)) = (value.as_object_mut(), stamp) {
            object.insert("now".into(), stamp);
        }
        *self.state.snapshot.lock().unwrap() = value.clone();
        (self.emit)("agents://snapshot", value);
    }

    fn check_budget(&mut self, spent: f64, now: Time) {
        let Some(budget) = self.settings.as_ref().and_then(|s| s.daily_budget) else { return };
        let today = summary::local_day(now);
        if !self.store.reports_transitions || spent < budget || self.budget_day == Some(today) {
            return;
        }
        self.budget_day = Some(today);
        self.report(Event::BudgetReached { spent, budget });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Harness {
        service: Service,
        events: Arc<Mutex<Vec<(String, Value)>>>,
    }

    fn harness(prefs: Value) -> Harness {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let mut service = Service::new(Arc::new(Shared::default()), Box::new(move || prefs.clone()), Box::new(move |name, payload| sink.lock().unwrap().push((name.into(), payload))));
        // Tests never touch the person's real archive in %LOCALAPPDATA%.
        service.persist = false;
        Harness { service, events }
    }

    #[test]
    fn a_rewritten_log_keeps_its_history_and_its_turn() {
        let dir = std::env::temp_dir().join(format!("vorssaint-restart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let projects = dir.join("projects").join("p");
        std::fs::create_dir_all(&projects).unwrap();
        let log = projects.join("s.jsonl");
        let now = model::now();
        let iso = |t: Time| chrono::DateTime::from_timestamp_millis((t * 1000.0) as i64).unwrap().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let reply = |id: &str, t: Time, stop: &str| format!("{{\"type\":\"assistant\",\"timestamp\":\"{}\",\"requestId\":\"{id}\",\"message\":{{\"id\":\"{id}\",\"model\":\"claude-opus-5-5\",\"stop_reason\":\"{stop}\",\"usage\":{{\"output_tokens\":10}}}}}}\n", iso(t));
        let prompt = format!("{{\"type\":\"user\",\"timestamp\":\"{}\",\"message\":{{\"content\":\"go\"}}}}\n", iso(now - 300.0));
        std::fs::write(&log, format!("{prompt}{}{}", reply("a", now - 200.0, "tool_use"), reply("b", now - 100.0, "tool_use"))).unwrap();
        let mut h = harness(json!({"island": {"agentsPriceUpdates": false, "agentsLimitAlert": false}}));
        h.service.settings = Some(settings(&(h.service.prefs)()));
        h.service.begin(vec![Root { provider: Provider::Claude, path: dir.join("projects") }], now);
        assert_eq!(h.service.store.records.len(), 2);
        // Rewritten shorter in place, then written on: read again from the start.
        std::fs::write(&log, format!("{prompt}{}", reply("a", now - 200.0, "tool_use"))).unwrap();
        h.service.poll(now);
        use std::io::Write;
        std::fs::OpenOptions::new().append(true).open(&log).unwrap().write_all(reply("c", now - 1.0, "end_turn").as_bytes()).unwrap();
        h.service.poll(now);
        h.service.poll(now);
        assert_eq!(h.service.store.records.len(), 3, "what was read stays counted");
        assert!(h.events.lock().unwrap().iter().any(|(n, e)| n == "agents://event" && e["kind"] == "finished"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_check_begun_before_a_redeem_does_not_bring_back_its_count() {
        let mut h = harness(json!({}));
        let summary = |n| codex::Summary { available: n, credits: vec![], limits: None, checked_at: 0.0 };
        h.service.resets.redeems = 1;
        h.service.finish(Job::Resets(Ok(summary(2)), 0), 0.0);
        assert!(h.service.resets.summary.is_none());
        h.service.finish(Job::Resets(Ok(summary(1)), 1), 0.0);
        assert_eq!(h.service.resets.summary.as_ref().map(|s| s.available), Some(1));
    }

    #[test]
    fn settings_follow_the_prefs_and_keep_one_provider() {
        let s = settings(&json!({"island": {"agentsClaude": false, "agentsCodex": false, "agentsOpenCode": false, "agentsCopilot": false, "agentsLimitAlert": false, "agentsReadout": "limit", "agentsDailyBudget": 25, "agentsFinishMinimum": 99999}}));
        assert!(s.enabled);
        assert_eq!(s.providers.len(), 4, "the last provider can't be switched off");
        assert_eq!(s.limit_threshold, None);
        assert!(s.limits_without_page, "the strip shows a limit");
        assert_eq!(s.daily_budget, Some(25.0));
        assert_eq!(s.finish_minimum, Some(3600.0));
        assert!(!settings(&json!({"island": {"sections": {"agents": false}}})).enabled);
        assert!(!settings(&json!({"island": {"hiddenModules": ["agents"]}})).enabled);
    }

    #[test]
    fn logs_become_a_snapshot_and_a_finished_turn_a_notice() {
        let dir = std::env::temp_dir().join(format!("vorssaint-service-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let projects = dir.join("projects").join("C--work-site");
        std::fs::create_dir_all(&projects).unwrap();
        let log = projects.join("sess-1.jsonl");
        let now = model::now();
        let iso = |t: Time| chrono::DateTime::from_timestamp_millis((t * 1000.0) as i64).unwrap().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        std::fs::write(&log, format!("{{\"type\":\"user\",\"sessionId\":\"sess-1\",\"cwd\":\"C:\\\\work\\\\site\",\"timestamp\":\"{}\",\"message\":{{\"content\":\"go\"}}}}\n", iso(now - 120.0))).unwrap();
        let mut h = harness(json!({"island": {"agentsPriceUpdates": false, "agentsLimitAlert": false}}));
        h.service.settings = Some(settings(&(h.service.prefs)()));
        h.service.begin(vec![Root { provider: Provider::Claude, path: dir.join("projects") }], now);
        let snapshot = h.service.state.snapshot.lock().unwrap().clone();
        assert_eq!(snapshot["loaded"], true);
        assert_eq!(snapshot["live"].as_array().unwrap().len(), 1, "the open turn shows as working");
        use std::io::Write;
        let reply = format!("{{\"type\":\"assistant\",\"sessionId\":\"sess-1\",\"timestamp\":\"{}\",\"requestId\":\"r\",\"message\":{{\"id\":\"m\",\"model\":\"claude-opus-5-5\",\"stop_reason\":\"end_turn\",\"usage\":{{\"input_tokens\":100,\"output_tokens\":2000}}}}}}\n", iso(now - 1.0));
        std::fs::OpenOptions::new().append(true).open(&log).unwrap().write_all(reply.as_bytes()).unwrap();
        h.service.poll(now);
        h.service.publish(now);
        let events = h.events.lock().unwrap().clone();
        let finished = events.iter().find(|(n, _)| n == "agents://event").expect("a notice");
        assert_eq!(finished.1["kind"], "finished");
        assert_eq!(finished.1["project"], "site");
        assert!(finished.1["duration"].as_f64().unwrap() >= 118.0);
        let last = &events.iter().rev().find(|(n, _)| n == "agents://snapshot").unwrap().1;
        assert!(last["live"].as_array().unwrap().is_empty());
        assert_eq!(last["periods"]["today"]["projects"][0]["name"], "site");
        assert!(last["periods"]["today"]["total"]["cost"].as_f64().unwrap() > 0.0);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
