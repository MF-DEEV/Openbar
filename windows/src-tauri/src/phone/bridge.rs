// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Phone approvals through a Telegram bot (#214), as the Mac's
//! `TelegramBridge.swift` does them. While the person is away from the PC,
//! each request a session waits on goes to their phone with the island's
//! buttons; a tap or a typed reply comes back into the session the way the
//! island's does. Only the Telegram account linked in Settings, in its
//! private chat with the bot, is listened to.
//!
//! Three threads: one reads Telegram's updates, one sends (one call at a
//! time, a moment apart), one looks at the sessions when they change and
//! every few seconds. The network and the system are behind `Api` and
//! `Host`, so the logic is tested without either.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::messages::{self, Action, Message, Outcome, Picked, Settings};
use crate::sessions::hook_message::Decision;
use crate::sessions::models::{Permission, Session};

/// Why a Bot API call failed.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    /// The token was revoked or never was one.
    Unauthorized,
    /// Something else reads this bot's updates: a webhook, or Openbar on
    /// another computer with the same bot.
    Conflict,
    /// Too many calls; Telegram says how many seconds to wait.
    TooMany(u64),
    Refused(String),
    Network,
}

pub trait Transport: Send + Sync {
    fn call(&self, method: &str, body: &Value, timeout: Duration) -> Result<Value, Failure>;
}

/// Makes a transport for a bot's token.
pub trait Api: Send + Sync {
    fn connect(&self, token: &str) -> Arc<dyn Transport>;
}

/// What the bridge needs from the app and the system.
pub trait Host: Send + Sync {
    /// Answers a request as the island would, from the phone.
    fn decide(&self, session: &str, request: &str, decision: Decision, revision: Option<&str>) -> bool;
    /// Types a reply into a session: None when it went, else why not.
    fn reply(&self, session: &str, text: &str) -> Option<&'static str>;
    /// Codex's Always: every request of the turn under way is allowed.
    fn auto_allow(&self, session: &str);
    fn locked(&self) -> bool;
    fn idle(&self) -> f64;
    /// Minutes since midnight, local time.
    fn minute(&self) -> u32;
    fn now(&self) -> f64;
    fn computer(&self) -> String;
    /// Settings shows what changed.
    fn changed(&self, status: &Status);
    fn save(&self, stored: &Stored);
    fn save_token(&self, token: Option<&str>) -> bool;
    fn qr(&self, text: &str) -> Option<String>;
    /// Writes the switch to the prefs, as Settings shows it. Called with the
    /// bridge's state let go: the prefs come back through `set_settings`.
    fn set_enabled(&self, on: bool);
}

/// The linked chat: only its person, in a private chat with the bot.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub chat: i64,
    pub user: i64,
    /// How the person goes by in Telegram, to show in Settings.
    pub name: String,
}

/// What stays on this PC between runs, beside the token in Credential Manager.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stored {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<Link>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkingStatus {
    pub code: String,
    pub link: String,
    pub expires: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qr: Option<String>,
}

/// What Settings shows.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub supported: bool,
    pub bot: Option<String>,
    pub linked: Option<String>,
    pub linking: Option<LinkingStatus>,
    /// invalidToken, credentialFailed, offline, conflict, revoked,
    /// codeExpired or tooManyTries.
    pub problem: Option<String>,
    pub checking: bool,
}

struct Linking {
    code: String,
    expires: f64,
    tries: u32,
}

/// A request's message on the phone.
struct Sent {
    serial: u64,
    session: String,
    request: String,
    revision: Option<String>,
    message: Option<i64>,
    text: String,
    picked: Picked,
    typed: BTreeMap<usize, String>,
    numbers: Vec<u64>,
    answering: bool,
    settled: bool,
}

enum Job {
    Send { body: Value, serial: Option<u64>, key: Option<String>, session: Option<String> },
    Call { method: &'static str, body: Value },
}

/// What a handled update leaves to do once the state is let go.
enum Effect {
    Decide { key: String, session: String, request: String, decision: Decision, revision: Option<String>, outcome: Outcome },
    AutoAllow(String),
    /// Linking turns the switch on.
    Enable,
    Reply { session: String, text: String, name: String, chat: i64 },
}

struct State {
    settings: Settings,
    token: Option<String>,
    transport: Option<Arc<dyn Transport>>,
    bot: Option<String>,
    link: Option<Link>,
    linking: Option<Linking>,
    problem: Option<String>,
    checking: bool,
    revoked: bool,
    run: String,
    started: f64,
    offset: i64,
    listening: bool,
    listen_epoch: u64,
    sent: HashMap<String, Sent>,
    actions: HashMap<u64, (String, Action)>,
    next: u64,
    serial: u64,
    owed: HashMap<u64, String>,
    message_sessions: HashMap<i64, String>,
    order: VecDeque<i64>,
    ends: HashSet<String>,
    ends_ready: bool,
    sessions: Arc<Vec<Session>>,
    resync_at: Option<f64>,
    running: bool,
}

impl State {
    fn active(&self) -> bool {
        self.settings.enabled && self.token.is_some() && self.link.is_some() && !self.revoked
    }
    fn listens(&self) -> bool {
        self.running && self.token.is_some() && !self.revoked && (self.active() || self.linking.is_some())
    }
}

pub struct Bridge {
    host: Arc<dyn Host>,
    api: Arc<dyn Api>,
    state: Mutex<State>,
    poll_wake: Condvar,
    outbox: Mutex<VecDeque<Job>>,
    outbox_wake: Condvar,
    look: Mutex<bool>,
    look_wake: Condvar,
    copies: AtomicBool,
}

fn key(session: &str, request: &str) -> String {
    format!("{session}\u{1}{request}")
}

fn revision(ask: &Permission) -> Option<String> {
    ask.content.as_ref().map(|c| c.metadata(ask).revision)
}

fn int(value: &Value) -> Option<i64> {
    value.as_i64()
}

fn person(user: &Value) -> String {
    if let Some(name) = user["username"].as_str().filter(|n| !n.is_empty()) {
        return format!("@{name}");
    }
    let name = [user["first_name"].as_str(), user["last_name"].as_str()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    if name.is_empty() { "Telegram".into() } else { name }
}

fn message_body(chat: i64, text: &str, buttons: Value) -> Value {
    json!({"chat_id": chat, "text": text, "parse_mode": "HTML",
           "link_preview_options": {"is_disabled": true},
           "reply_markup": {"inline_keyboard": buttons}})
}

fn display_name(session: &Session) -> String {
    let name = session.name.trim();
    if name.is_empty() { session.project.clone() } else { name.to_owned() }
}

impl Bridge {
    pub fn new(host: Arc<dyn Host>, api: Arc<dyn Api>, stored: Stored, token: Option<String>, settings: Settings) -> Arc<Self> {
        let started = host.now();
        let transport = token.as_deref().map(|t| api.connect(t));
        Arc::new(Bridge {
            host,
            api,
            state: Mutex::new(State {
                settings,
                token,
                transport,
                bot: stored.bot,
                link: stored.link,
                linking: None,
                problem: None,
                checking: false,
                revoked: false,
                run: messages::new_run(),
                started,
                offset: 0,
                listening: false,
                listen_epoch: 0,
                sent: HashMap::new(),
                actions: HashMap::new(),
                next: 0,
                serial: 0,
                owed: HashMap::new(),
                message_sessions: HashMap::new(),
                order: VecDeque::new(),
                ends: HashSet::new(),
                ends_ready: false,
                sessions: Arc::new(Vec::new()),
                resync_at: None,
                running: false,
            }),
            poll_wake: Condvar::new(),
            outbox: Mutex::new(VecDeque::new()),
            outbox_wake: Condvar::new(),
            look: Mutex::new(false),
            look_wake: Condvar::new(),
            copies: AtomicBool::new(false),
        })
    }

    /// Starts the three threads; they run for the app's life.
    pub fn start(self: &Arc<Self>) {
        {
            let mut state = self.state.lock().unwrap();
            state.running = true;
            self.refresh(&mut state);
        }
        let me = self.clone();
        thread::Builder::new().name("phone-updates".into()).spawn(move || me.read_updates()).ok();
        let me = self.clone();
        thread::Builder::new().name("phone-send".into()).spawn(move || me.send_jobs()).ok();
        let me = self.clone();
        thread::Builder::new().name("phone-look".into()).spawn(move || me.look_loop()).ok();
    }

    // MARK: From the app

    /// The sessions as the island now shows them.
    pub fn publish(&self, sessions: &[Session]) {
        if !self.copies.load(Ordering::Relaxed) {
            return;
        }
        self.state.lock().unwrap().sessions = Arc::new(sessions.to_vec());
        self.wake_look();
    }

    pub fn set_settings(&self, settings: Settings) {
        let mut state = self.state.lock().unwrap();
        if state.settings == settings {
            return;
        }
        let was = state.active();
        state.settings = settings;
        if was && !state.active() {
            state.ends_ready = false;
        }
        self.refresh(&mut state);
        drop(state);
        self.wake_look();
    }

    pub fn status(&self) -> Status {
        let state = self.state.lock().unwrap();
        self.status_of(&state)
    }

    fn status_of(&self, state: &State) -> Status {
        Status {
            supported: true,
            bot: state.bot.clone(),
            linked: state.link.as_ref().map(|l| l.name.clone()),
            linking: state.linking.as_ref().and_then(|l| {
                let bot = state.bot.as_deref()?;
                let link = messages::link(bot, &l.code);
                Some(LinkingStatus { code: l.code.clone(), qr: self.host.qr(&link), link, expires: l.expires })
            }),
            problem: state.problem.clone(),
            checking: state.checking,
        }
    }

    /// Whether to listen, copy snapshots, and tell Settings.
    fn refresh(&self, state: &mut State) {
        let listens = state.listens();
        if listens && !state.listening {
            // Nothing that came while no one listened is acted on later.
            state.listen_epoch += 1;
        }
        state.listening = listens;
        self.copies.store(state.running && state.active(), Ordering::Relaxed);
        self.poll_wake.notify_all();
        self.host.changed(&self.status_of(state));
    }

    /// Checks a token BotFather gave and keeps it in Credential Manager.
    pub fn use_token(&self, raw: &str) -> Status {
        let token = raw.trim().to_owned();
        if !valid_token(&token) {
            let mut state = self.state.lock().unwrap();
            state.problem = Some("invalidToken".into());
            self.refresh(&mut state);
            return self.status_of(&state);
        }
        {
            let mut state = self.state.lock().unwrap();
            state.checking = true;
            self.refresh(&mut state);
        }
        let transport = self.api.connect(&token);
        let me = transport.call("getMe", &json!({}), Duration::from_secs(20));
        let mut state = self.state.lock().unwrap();
        state.checking = false;
        match me {
            Ok(me) => match me["username"].as_str().filter(|n| !n.is_empty()) {
                Some(name) if self.host.save_token(Some(&token)) => {
                    state.token = Some(token);
                    state.transport = Some(transport);
                    state.bot = Some(name.to_owned());
                    state.link = None;
                    state.revoked = false;
                    state.problem = None;
                    self.host.save(&Stored { bot: state.bot.clone(), link: None });
                    state.linking = Some(Linking {
                        code: messages::make_code(),
                        expires: self.host.now() + messages::CODE_VALID_FOR,
                        tries: 0,
                    });
                }
                Some(_) => state.problem = Some("credentialFailed".into()),
                None => state.problem = Some("invalidToken".into()),
            },
            Err(Failure::Network) => state.problem = Some("offline".into()),
            Err(_) => state.problem = Some("invalidToken".into()),
        }
        self.refresh(&mut state);
        self.status_of(&state)
    }

    pub fn start_linking(&self) -> Status {
        let mut state = self.state.lock().unwrap();
        if state.token.is_some() && state.link.is_none() {
            state.linking = Some(Linking {
                code: messages::make_code(),
                expires: self.host.now() + messages::CODE_VALID_FOR,
                tries: 0,
            });
            state.problem = None;
        }
        self.refresh(&mut state);
        self.status_of(&state)
    }

    pub fn cancel_linking(&self) -> Status {
        let mut state = self.state.lock().unwrap();
        state.linking = None;
        self.refresh(&mut state);
        self.status_of(&state)
    }

    /// Unlinks the phone and forgets the bot: nothing more is sent or read.
    pub fn unlink(&self) -> Status {
        self.outbox.lock().unwrap().clear();
        let mut state = self.state.lock().unwrap();
        state.token = None;
        state.transport = None;
        state.link = None;
        state.bot = None;
        state.linking = None;
        state.problem = None;
        state.revoked = false;
        state.sent.clear();
        state.actions.clear();
        state.owed.clear();
        state.message_sessions.clear();
        state.order.clear();
        state.ends_ready = false;
        self.host.save_token(None);
        self.host.save(&Stored::default());
        state.settings.enabled = false;
        self.refresh(&mut state);
        let status = self.status_of(&state);
        drop(state);
        self.host.set_enabled(false);
        status
    }

    pub fn send_test(&self) {
        let state = self.state.lock().unwrap();
        if let Some(link) = &state.link {
            self.enqueue(Job::Call { method: "sendMessage", body: message_body(link.chat, messages::TEST, json!([])) });
        }
    }

    // MARK: Reading updates

    fn read_updates(self: Arc<Self>) {
        let mut drained = 0u64;
        let mut backoff = 2u64;
        loop {
            let (transport, epoch, offset) = {
                let mut state = self.state.lock().unwrap();
                while !state.listens() {
                    state = self.poll_wake.wait(state).unwrap();
                }
                let Some(transport) = state.transport.clone() else { continue };
                (transport, state.listen_epoch, state.offset)
            };
            if epoch != drained {
                match transport.call("getUpdates", &json!({"offset": -1, "timeout": 0}), Duration::from_secs(20)) {
                    Ok(last) => {
                        if let Some(id) = last.as_array().and_then(|l| l.last()).and_then(|u| int(&u["update_id"])) {
                            let mut state = self.state.lock().unwrap();
                            state.offset = state.offset.max(id + 1);
                        }
                        drained = epoch;
                    }
                    Err(failure) => self.failed(failure, &mut backoff),
                }
                continue;
            }
            match transport.call(
                "getUpdates",
                &json!({"offset": offset, "timeout": 50, "allowed_updates": ["message", "callback_query"]}),
                Duration::from_secs(65),
            ) {
                Ok(updates) => {
                    backoff = 2;
                    {
                        let mut state = self.state.lock().unwrap();
                        if matches!(state.problem.as_deref(), Some("offline" | "conflict")) {
                            state.problem = None;
                            self.refresh(&mut state);
                        }
                    }
                    for update in updates.as_array().into_iter().flatten() {
                        if let Some(id) = int(&update["update_id"]) {
                            let mut state = self.state.lock().unwrap();
                            state.offset = state.offset.max(id + 1);
                        }
                        self.handle(update);
                    }
                }
                Err(failure) => self.failed(failure, &mut backoff),
            }
        }
    }

    fn failed(&self, failure: Failure, backoff: &mut u64) {
        let wait = match &failure {
            Failure::Unauthorized => {
                let mut state = self.state.lock().unwrap();
                state.revoked = true;
                state.problem = Some("revoked".into());
                self.refresh(&mut state);
                return;
            }
            Failure::Conflict => {
                let mut state = self.state.lock().unwrap();
                state.problem = Some("conflict".into());
                self.refresh(&mut state);
                30
            }
            Failure::TooMany(wait) => *wait,
            Failure::Network | Failure::Refused(_) => {
                if failure == Failure::Network {
                    let mut state = self.state.lock().unwrap();
                    if state.problem.is_none() {
                        state.problem = Some("offline".into());
                        self.refresh(&mut state);
                    }
                }
                let wait = *backoff;
                *backoff = (*backoff * 2).min(60);
                wait
            }
        };
        thread::sleep(Duration::from_secs(wait));
    }

    /// One update: a message, or a tap on a button.
    pub fn handle(&self, update: &Value) {
        let effects = {
            let mut state = self.state.lock().unwrap();
            if update["message"].is_object() {
                self.handle_message(&mut state, &update["message"])
            } else if update["callback_query"].is_object() {
                self.handle_tap(&mut state, &update["callback_query"])
            } else {
                Vec::new()
            }
        };
        for effect in effects {
            self.perform(effect);
        }
    }

    fn handle_message(&self, state: &mut State, message: &Value) -> Vec<Effect> {
        let chat = &message["chat"];
        let from = &message["from"];
        let (Some(chat_id), Some(user_id)) = (int(&chat["id"]), int(&from["id"])) else { return Vec::new() };
        if chat["type"].as_str() != Some("private") || from["is_bot"].as_bool() == Some(true) {
            return Vec::new();
        }
        let text = message["text"].as_str().unwrap_or("");
        if state.link.is_none() && state.linking.is_some() {
            let Some(code) = messages::code_in(text) else { return Vec::new() };
            let now = self.host.now();
            let (expected, expires) = state.linking.as_ref().map(|l| (l.code.clone(), l.expires)).unwrap();
            if now > expires {
                state.linking = None;
                state.problem = Some("codeExpired".into());
            } else if code == expected {
                let link = Link { chat: chat_id, user: user_id, name: person(from) };
                state.link = Some(link);
                state.linking = None;
                state.problem = None;
                state.settings.enabled = true;
                state.ends_ready = false;
                self.host.save(&Stored { bot: state.bot.clone(), link: state.link.clone() });
                self.say(chat_id, &messages::linked(&self.host.computer()));
                self.refresh(state);
                return vec![Effect::Enable];
            } else {
                let tries = state.linking.as_mut().map_or(0, |l| {
                    l.tries += 1;
                    l.tries
                });
                if tries >= messages::CODE_TRIES {
                    state.linking = None;
                    state.problem = Some("tooManyTries".into());
                }
                self.say(chat_id, messages::WRONG_CODE);
            }
            self.refresh(state);
            return Vec::new();
        }
        // Anyone but the linked person, or a group, is not answered at all.
        let Some(link) = state.link.clone() else { return Vec::new() };
        if !state.active() || chat_id != link.chat || user_id != link.user {
            return Vec::new();
        }
        // Nothing typed while Openbar was not running reaches a session later.
        if message["date"].as_f64().unwrap_or(0.0) < state.started - 120.0 {
            return Vec::new();
        }
        let reply_to = int(&message["reply_to_message"]["message_id"]);
        self.typed(state, text, reply_to, &link)
    }

    fn typed(&self, state: &mut State, raw: &str, reply_to: Option<i64>, link: &Link) -> Vec<Effect> {
        let text: String = raw.trim().chars().take(4000).collect();
        if text.is_empty() {
            return Vec::new();
        }
        if text.starts_with('/') {
            if text.to_lowercase().starts_with("/start") {
                self.say(link.chat, &messages::linked(&self.host.computer()));
            }
            return Vec::new();
        }
        let sessions = state.sessions.clone();
        // A reply to a question's message is the person's own answer.
        if let Some(reply_to) = reply_to {
            let found = state.sent.iter().find(|(_, s)| s.message == Some(reply_to)).map(|(k, _)| k.clone());
            if let Some(k) = found {
                let entry = state.sent.get_mut(&k).unwrap();
                let ask = sessions
                    .iter()
                    .find(|s| s.id == entry.session)
                    .and_then(|s| s.permission.iter().chain(&s.later_asks).find(|p| p.request_id == entry.request))
                    .cloned();
                if let Some(ask) = ask.filter(|a| !a.questions.is_empty())
                    && !entry.settled
                    && !entry.answering
                {
                    let open = (0..ask.questions.len())
                        .find(|q| entry.picked.get(q).is_none_or(|p| p.is_empty()) && !entry.typed.contains_key(q))
                        .unwrap_or(0);
                    entry.typed.insert(open, text);
                    if let Some(answers) = messages::answers(&ask, &entry.picked, &entry.typed) {
                        let labels = messages::labels(&answers, &ask);
                        return self.answer(state, &k, Decision::Answer(answers), Outcome::Answered(labels));
                    }
                    self.say(link.chat, messages::PICK_THE_REST);
                    return Vec::new();
                }
            }
        }
        let target = match reply_to {
            Some(id) => match state.message_sessions.get(&id) {
                Some(session) => messages::reply_target(Some(session), &sessions),
                None => {
                    self.say(link.chat, messages::EARLIER_RUN);
                    return Vec::new();
                }
            },
            None => messages::reply_target(None, &sessions),
        };
        let Some(session) = target.and_then(|id| sessions.iter().find(|s| s.id == id).cloned()) else {
            let any = sessions.iter().any(messages::can_reply);
            self.say(link.chat, if any { messages::WHICH_SESSION } else { messages::NO_SESSION });
            return Vec::new();
        };
        vec![Effect::Reply { name: display_name(&session), session: session.id, text, chat: link.chat }]
    }

    fn handle_tap(&self, state: &mut State, query: &Value) -> Vec<Effect> {
        let Some(id) = query["id"].as_str().map(str::to_owned) else { return Vec::new() };
        let Some(link) = state.link.clone() else { return Vec::new() };
        if int(&query["from"]["id"]) != Some(link.user) || int(&query["message"]["chat"]["id"]) != Some(link.chat) {
            return Vec::new();
        }
        let Some((run, number)) = query["data"].as_str().and_then(messages::parse_callback) else {
            self.answer_tap(state, &id, None);
            return Vec::new();
        };
        if run != state.run {
            self.answer_tap(state, &id, Some(messages::EARLIER_RUN));
            return Vec::new();
        }
        let found = state.actions.get(&number).cloned();
        let live = found.as_ref().and_then(|(k, action)| {
            let entry = state.sent.get(k)?;
            if entry.settled || entry.answering || !state.active() {
                return None;
            }
            let session = state.sessions.iter().find(|s| s.id == entry.session)?.clone();
            let ask = session.permission.iter().chain(&session.later_asks).find(|p| p.request_id == entry.request)?.clone();
            Some((k.clone(), action.clone(), session, ask))
        });
        let Some((k, action, session, ask)) = live else {
            self.answer_tap(state, &id, Some(messages::SETTLED));
            return Vec::new();
        };
        self.answer_tap(state, &id, None);
        match action {
            Action::Allow => {
                let outcome = if messages::is_plan(&ask) { Outcome::PlanApproved(None) } else { Outcome::Allowed };
                self.answer(state, &k, Decision::Allow, outcome)
            }
            Action::Always if session.codex_server() => {
                let mut effects = vec![Effect::AutoAllow(session.id.clone())];
                effects.extend(self.answer(state, &k, Decision::Allow, Outcome::Always));
                effects
            }
            Action::Always => self.answer(state, &k, Decision::AllowAlways, Outcome::Always),
            Action::Deny => self.answer(state, &k, Decision::Deny, Outcome::Denied),
            Action::KeepPlanning => self.answer(state, &k, Decision::Deny, Outcome::KeptPlanning),
            Action::Skip => self.answer(state, &k, Decision::Deny, Outcome::Skipped),
            Action::ApprovePlan(mode) => self.answer(
                state,
                &k,
                mode.map_or(Decision::Allow, Decision::ApprovePlan),
                Outcome::PlanApproved(mode.map(|m| messages::plan_label(m).to_owned())),
            ),
            Action::Pick(q, o) => {
                if q >= ask.questions.len() {
                    return Vec::new();
                }
                if messages::answers_on_tap(&ask) {
                    let picked = Picked::from([(q, [o].into())]);
                    let Some(answers) = messages::answers(&ask, &picked, &BTreeMap::new()) else { return Vec::new() };
                    let labels = messages::labels(&answers, &ask);
                    return self.answer(state, &k, Decision::Answer(answers), Outcome::Answered(labels));
                }
                let hide = state.settings.hide_code;
                let entry = state.sent.get_mut(&k).unwrap();
                let chosen = entry.picked.entry(q).or_default();
                if ask.questions[q].multi_select {
                    if !chosen.remove(&o) {
                        chosen.insert(o);
                    }
                } else {
                    *chosen = [o].into();
                    entry.typed.remove(&q);
                }
                let picked = entry.picked.clone();
                let message = entry.message;
                let old = std::mem::take(&mut entry.numbers);
                for n in old {
                    state.actions.remove(&n);
                }
                let buttons = messages::buttons(&ask, &session, hide, &picked);
                let keyboard = self.issue(state, &buttons, &k);
                if let Some(message) = message {
                    self.enqueue(Job::Call {
                        method: "editMessageReplyMarkup",
                        body: json!({"chat_id": link.chat, "message_id": message, "reply_markup": {"inline_keyboard": keyboard}}),
                    });
                }
                Vec::new()
            }
            Action::Send => {
                let entry = &state.sent[&k];
                let Some(answers) = messages::answers(&ask, &entry.picked, &entry.typed) else { return Vec::new() };
                let labels = messages::labels(&answers, &ask);
                self.answer(state, &k, Decision::Answer(answers), Outcome::Answered(labels))
            }
        }
    }

    fn answer_tap(&self, state: &State, id: &str, text: Option<&str>) {
        let Some(transport) = state.transport.clone() else { return };
        let mut body = json!({"callback_query_id": id});
        if let Some(text) = text {
            body["text"] = json!(text);
        }
        thread::spawn(move || {
            let _ = transport.call("answerCallbackQuery", &body, Duration::from_secs(10));
        });
    }

    /// Marks the request as being answered; the answer goes out once the state is let go.
    fn answer(&self, state: &mut State, k: &str, decision: Decision, outcome: Outcome) -> Vec<Effect> {
        let Some(entry) = state.sent.get_mut(k) else { return Vec::new() };
        entry.answering = true;
        vec![Effect::Decide {
            key: k.to_owned(),
            session: entry.session.clone(),
            request: entry.request.clone(),
            decision,
            revision: entry.revision.clone(),
            outcome,
        }]
    }

    fn perform(&self, effect: Effect) {
        match effect {
            Effect::AutoAllow(session) => self.host.auto_allow(&session),
            Effect::Enable => self.host.set_enabled(true),
            Effect::Decide { key, session, request, decision, revision, outcome } => {
                let sent = self.host.decide(&session, &request, decision, revision.as_deref());
                let mut state = self.state.lock().unwrap();
                let Some(entry) = state.sent.get_mut(&key) else { return };
                entry.answering = false;
                let outcome = if sent {
                    outcome
                } else {
                    let now = state
                        .sessions
                        .iter()
                        .find(|s| s.id == session)
                        .and_then(|s| s.permission.iter().chain(&s.later_asks).find(|p| p.request_id == request))
                        .map(revision_of);
                    if now.is_some_and(|r| r != revision) { Outcome::Changed } else { Outcome::Failed }
                };
                self.settle(&mut state, &key, outcome);
            }
            Effect::Reply { session, text, name, chat } => {
                let host = self.host.clone();
                let me = self.state.lock().unwrap().transport.clone();
                thread::spawn(move || {
                    let refusal = host.reply(&session, &text);
                    let words = match refusal {
                        None => messages::sent_to(&name),
                        Some(why) => messages::reply_failed(why, &name),
                    };
                    if let Some(transport) = me {
                        let _ = transport.call("sendMessage", &message_body(chat, &words, json!([])), Duration::from_secs(20));
                    }
                });
            }
        }
    }

    // MARK: Sending

    fn look_loop(self: Arc<Self>) {
        loop {
            let wait = {
                let state = self.state.lock().unwrap();
                state
                    .resync_at
                    .map(|at| (at - self.host.now()).clamp(0.05, 15.0))
                    .unwrap_or(15.0)
            };
            {
                let pending = self.look.lock().unwrap();
                let (mut pending, _) = self
                    .look_wake
                    .wait_timeout_while(pending, Duration::from_secs_f64(wait), |p| !*p)
                    .unwrap();
                *pending = false;
            }
            self.sync();
        }
    }

    fn wake_look(&self) {
        *self.look.lock().unwrap() = true;
        self.look_wake.notify_all();
    }

    /// Looks at what waits and sends, updates or settles messages.
    pub fn sync(&self) {
        let mut state = self.state.lock().unwrap();
        state.resync_at = None;
        if !state.running || !state.active() {
            return;
        }
        let sessions = state.sessions.clone();
        let mut live: HashMap<String, (&Session, &Permission)> = HashMap::new();
        for session in sessions.iter().filter(|s| s.id != crate::sessions::sample::ID) {
            for ask in session.permission.iter().chain(&session.later_asks) {
                live.insert(key(&session.id, &ask.request_id), (session, ask));
            }
        }
        let keys: Vec<String> = state.sent.keys().cloned().collect();
        for k in keys {
            let entry = &state.sent[&k];
            if entry.answering {
                continue;
            }
            match live.get(&k) {
                Some((_, ask)) => {
                    if !entry.settled && revision(ask) != entry.revision {
                        // The same request now says something else: the old
                        // message stops, and a new one goes out below.
                        self.settle(&mut state, &k, Outcome::Changed);
                        state.sent.remove(&k);
                    }
                }
                None => {
                    if !entry.settled {
                        let here = sessions.iter().any(|s| s.id == entry.session);
                        self.settle(&mut state, &k, if here { Outcome::AnsweredElsewhere } else { Outcome::Gone });
                    }
                    state.sent.remove(&k);
                }
            }
        }
        let away = messages::sends(&state.settings, self.host.locked(), self.host.idle());
        let now = self.host.now();
        if away {
            let mut fresh = false;
            let mut waiting: Vec<(&String, &(&Session, &Permission))> = live.iter().collect();
            waiting.sort_by(|a, b| a.1.1.since.total_cmp(&b.1.1.since));
            for (k, (session, ask)) in waiting {
                if state.sent.contains_key(k) {
                    continue;
                }
                // A moment first, for what the island answers by itself.
                if now - ask.since < 1.5 {
                    fresh = true;
                    continue;
                }
                self.send_ask(&mut state, k, session, ask);
            }
            if fresh {
                state.resync_at = Some(now + 1.6);
            }
        }
        // Ends seen when the bridge starts, or while the person was here,
        // are never sent later.
        if !state.settings.tell_ends {
            state.ends_ready = false;
            return;
        }
        let mut current = HashSet::new();
        for session in sessions.iter().filter(|s| s.id != crate::sessions::sample::ID) {
            let Some((end, at)) = messages::terminal(session) else { continue };
            // One message a turn: its end can still move a moment.
            let turn = session.turn_started.unwrap_or(at);
            let id = format!("{}\u{1}{turn}", session.id);
            if state.ends_ready && away && !state.ends.contains(&id) {
                let message = messages::ended(end, session, &display_name(session), state.settings.hide_code);
                self.post(&state, message, &session.id);
            }
            current.insert(id);
        }
        state.ends.extend(current.iter().cloned());
        if state.ends.len() > 2048 {
            state.ends = current;
        }
        state.ends_ready = true;
    }

    fn send_ask(&self, state: &mut State, k: &str, session: &Session, ask: &Permission) {
        let Some(link) = state.link.clone() else { return };
        let message = messages::ask(ask, session, &display_name(session), state.settings.hide_code, &Picked::new());
        state.serial += 1;
        let serial = state.serial;
        state.sent.insert(
            k.to_owned(),
            Sent {
                serial,
                session: session.id.clone(),
                request: ask.request_id.clone(),
                revision: revision(ask),
                message: None,
                text: message.text.clone(),
                picked: Picked::new(),
                typed: BTreeMap::new(),
                numbers: Vec::new(),
                answering: false,
                settled: false,
            },
        );
        let keyboard = self.issue(state, &message.buttons, k);
        let mut body = message_body(link.chat, &message.text, keyboard);
        body["disable_notification"] = json!(messages::quiet(&state.settings, self.host.minute()));
        self.enqueue(Job::Send { body, serial: Some(serial), key: Some(k.to_owned()), session: Some(session.id.clone()) });
    }

    fn post(&self, state: &State, message: Message, session: &str) {
        let Some(link) = &state.link else { return };
        let mut body = message_body(link.chat, &message.text, json!([]));
        body["disable_notification"] = json!(messages::quiet(&state.settings, self.host.minute()));
        self.enqueue(Job::Send { body, serial: None, key: None, session: Some(session.to_owned()) });
    }

    /// Ends a request's message with how it went, and takes its buttons away.
    fn settle(&self, state: &mut State, k: &str, outcome: Outcome) {
        let minute = self.host.minute();
        let Some(chat) = state.link.as_ref().map(|l| l.chat) else { return };
        let Some(entry) = state.sent.get_mut(k) else { return };
        if entry.settled {
            return;
        }
        entry.settled = true;
        let numbers = std::mem::take(&mut entry.numbers);
        let text = format!("{}\n\n{}", entry.text, messages::outcome(&outcome, minute));
        let message = entry.message;
        let serial = entry.serial;
        for n in numbers {
            state.actions.remove(&n);
        }
        match message {
            Some(id) => {
                let mut body = message_body(chat, &text, json!([]));
                body["message_id"] = json!(id);
                self.enqueue(Job::Call { method: "editMessageText", body });
            }
            None => {
                state.owed.insert(serial, text);
            }
        }
    }

    /// Numbers the buttons of a message for this run.
    fn issue(&self, state: &mut State, rows: &[Vec<messages::Button>], k: &str) -> Value {
        let run = state.run.clone();
        let mut numbers = Vec::new();
        let keyboard: Vec<Vec<Value>> = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|button| {
                        state.next += 1;
                        let n = state.next;
                        state.actions.insert(n, (k.to_owned(), button.action.clone()));
                        numbers.push(n);
                        json!({"text": button.title, "callback_data": messages::callback_data(&run, n)})
                    })
                    .collect()
            })
            .collect();
        if let Some(entry) = state.sent.get_mut(k) {
            entry.numbers.extend(numbers);
        }
        json!(keyboard)
    }

    fn say(&self, chat: i64, text: &str) {
        self.enqueue(Job::Call { method: "sendMessage", body: message_body(chat, text, json!([])) });
    }

    fn enqueue(&self, job: Job) {
        self.outbox.lock().unwrap().push_back(job);
        self.outbox_wake.notify_all();
    }

    /// One call at a time, a moment apart, as Telegram asks of bots.
    fn send_jobs(self: Arc<Self>) {
        loop {
            let job = {
                let mut outbox = self.outbox.lock().unwrap();
                while outbox.is_empty() {
                    outbox = self.outbox_wake.wait(outbox).unwrap();
                }
                outbox.pop_front().unwrap()
            };
            self.run_job(job);
            thread::sleep(Duration::from_millis(350));
        }
    }

    fn run_job(&self, job: Job) {
        let Some(transport) = self.state.lock().unwrap().transport.clone() else { return };
        let call = |method: &str, body: &Value| match transport.call(method, body, Duration::from_secs(20)) {
            Err(Failure::TooMany(wait)) => {
                thread::sleep(Duration::from_secs(wait));
                transport.call(method, body, Duration::from_secs(20))
            }
            other => other,
        };
        match job {
            Job::Call { method, body } => {
                if call(method, &body) == Err(Failure::Unauthorized) {
                    let mut state = self.state.lock().unwrap();
                    state.revoked = true;
                    state.problem = Some("revoked".into());
                    self.refresh(&mut state);
                }
            }
            Job::Send { body, serial, key, session } => {
                let result = call("sendMessage", &body);
                let mut state = self.state.lock().unwrap();
                let Ok(sent) = result else {
                    // Not sent: the next look tries again.
                    if let (Some(serial), Some(k)) = (serial, key) {
                        state.owed.remove(&serial);
                        if state.sent.get(&k).is_some_and(|e| e.serial == serial && !e.settled) {
                            state.sent.remove(&k);
                        }
                    }
                    return;
                };
                let Some(id) = int(&sent["message_id"]) else { return };
                if let Some(session) = session {
                    state.message_sessions.insert(id, session);
                    state.order.push_back(id);
                    if state.order.len() > 500
                        && let Some(old) = state.order.pop_front()
                    {
                        state.message_sessions.remove(&old);
                    }
                }
                let Some(serial) = serial else { return };
                if let Some(entry) = key.and_then(|k| state.sent.get_mut(&k)).filter(|e| e.serial == serial) {
                    entry.message = Some(id);
                }
                if let Some(text) = state.owed.remove(&serial)
                    && let Some(chat) = state.link.as_ref().map(|l| l.chat)
                {
                    let mut body = message_body(chat, &text, json!([]));
                    body["message_id"] = json!(id);
                    drop(state);
                    let _ = call("editMessageText", &body);
                }
            }
        }
    }
}

fn revision_of(ask: &Permission) -> Option<String> {
    revision(ask)
}

/// A token as BotFather hands it out: the bot's number, a colon, a secret.
pub fn valid_token(token: &str) -> bool {
    let Some((id, secret)) = token.split_once(':') else { return false };
    (3..=20).contains(&id.len())
        && id.chars().all(|c| c.is_ascii_digit())
        && (30..=100).contains(&secret.len())
        && secret.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    const TOKEN: &str = "123456789:AAtesttokentesttokentesttokentest00";

    #[derive(Default)]
    struct FakeTransport {
        calls: StdMutex<Vec<(String, Value)>>,
    }

    impl Transport for FakeTransport {
        fn call(&self, method: &str, body: &Value, _: Duration) -> Result<Value, Failure> {
            self.calls.lock().unwrap().push((method.to_owned(), body.clone()));
            match method {
                "getMe" => Ok(json!({"username": "openbar_test_bot"})),
                "sendMessage" => Ok(json!({"message_id": 100 + self.calls.lock().unwrap().len() as i64})),
                _ => Ok(json!(true)),
            }
        }
    }

    struct FakeApi(Arc<FakeTransport>);
    impl Api for FakeApi {
        fn connect(&self, _: &str) -> Arc<dyn Transport> {
            self.0.clone()
        }
    }

    #[derive(Default)]
    struct FakeHost {
        decided: StdMutex<Vec<(String, String, Decision, Option<String>)>>,
        auto: StdMutex<Vec<String>>,
        idle: StdMutex<f64>,
        enabled: StdMutex<Option<bool>>,
    }

    impl Host for FakeHost {
        fn decide(&self, session: &str, request: &str, decision: Decision, revision: Option<&str>) -> bool {
            self.decided.lock().unwrap().push((session.into(), request.into(), decision, revision.map(Into::into)));
            true
        }
        fn reply(&self, _: &str, _: &str) -> Option<&'static str> {
            None
        }
        fn auto_allow(&self, session: &str) {
            self.auto.lock().unwrap().push(session.into());
        }
        fn locked(&self) -> bool {
            false
        }
        fn idle(&self) -> f64 {
            *self.idle.lock().unwrap()
        }
        fn minute(&self) -> u32 {
            600
        }
        fn now(&self) -> f64 {
            1000.0
        }
        fn computer(&self) -> String {
            "PC".into()
        }
        fn changed(&self, _: &Status) {}
        fn save(&self, _: &Stored) {}
        fn save_token(&self, _: Option<&str>) -> bool {
            true
        }
        fn qr(&self, _: &str) -> Option<String> {
            None
        }
        fn set_enabled(&self, on: bool) {
            *self.enabled.lock().unwrap() = Some(on);
        }
    }

    fn bridge(idle: f64) -> (Arc<Bridge>, Arc<FakeTransport>, Arc<FakeHost>) {
        let transport = Arc::new(FakeTransport::default());
        let host = Arc::new(FakeHost::default());
        *host.idle.lock().unwrap() = idle;
        let link = Link { chat: 555, user: 555, name: "@me".into() };
        let settings = Settings { enabled: true, ..Default::default() };
        let bridge = Bridge::new(
            host.clone(),
            Arc::new(FakeApi(transport.clone())),
            Stored { bot: Some("openbar_test_bot".into()), link: Some(link) },
            Some(TOKEN.into()),
            settings,
        );
        bridge.state.lock().unwrap().running = true;
        bridge.copies.store(true, Ordering::Relaxed);
        (bridge, transport, host)
    }

    fn waiting(codex: bool) -> Session {
        let mut s = crate::sessions::sample::waiting(1, "/w", 0.0);
        s.id = "s1".into();
        s.project = "web".into();
        s.name = "web".into();
        s.provider = if codex { crate::sessions::models::Provider::Codex } else { crate::sessions::models::Provider::Claude };
        s.via = None;
        let mut p = Permission::new("r1", "Bash", Some("ls"), true, 0.0);
        p.set_content("ls", true, Some(2));
        p.detail = "ls".into();
        s.permission = Some(p);
        s.later_asks = Vec::new();
        s
    }

    /// Runs what the sender thread would.
    fn flush(bridge: &Bridge) {
        loop {
            let job = bridge.outbox.lock().unwrap().pop_front();
            match job {
                Some(job) => bridge.run_job(job),
                None => break,
            }
        }
    }

    fn sent_messages(transport: &FakeTransport) -> Vec<Value> {
        transport.calls.lock().unwrap().iter().filter(|(m, _)| m == "sendMessage").map(|(_, b)| b.clone()).collect()
    }

    fn tap(bridge: &Bridge, from: i64, title: &str, transport: &FakeTransport) {
        let message = sent_messages(transport).last().cloned().unwrap();
        let data = message["reply_markup"]["inline_keyboard"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| r.as_array().unwrap().clone())
            .find(|b| b["text"] == title)
            .unwrap()["callback_data"]
            .clone();
        bridge.handle(&json!({"update_id": 1, "callback_query": {"id": "q", "from": {"id": from},
            "message": {"message_id": 101, "chat": {"id": 555, "type": "private"}}, "data": data}}));
    }

    #[test]
    fn nothing_goes_out_while_the_person_is_here() {
        let (bridge, transport, _) = bridge(10.0);
        bridge.publish(&[waiting(false)]);
        bridge.sync();
        flush(&bridge);
        assert!(sent_messages(&transport).is_empty());
    }

    #[test]
    fn a_request_goes_out_when_away_and_a_tap_answers_it() {
        let (bridge, transport, host) = bridge(400.0);
        bridge.publish(&[waiting(false)]);
        bridge.sync();
        flush(&bridge);
        let sent = sent_messages(&transport);
        assert_eq!(sent.len(), 1);
        assert!(sent[0]["text"].as_str().unwrap().contains("<pre>ls</pre>"));
        // A stranger's tap does nothing.
        tap(&bridge, 666, "Allow", &transport);
        assert!(host.decided.lock().unwrap().is_empty());
        tap(&bridge, 555, "Allow", &transport);
        let decided = host.decided.lock().unwrap().clone();
        assert_eq!(decided.len(), 1);
        assert_eq!(decided[0].2, Decision::Allow);
        assert!(decided[0].3.is_some(), "the message's revision goes along");
        flush(&bridge);
        let edits: Vec<Value> = transport.calls.lock().unwrap().iter().filter(|(m, _)| m == "editMessageText").map(|(_, b)| b.clone()).collect();
        assert!(edits.last().unwrap()["text"].as_str().unwrap().ends_with("<b>Allowed from your phone</b> · 10:00"));
        // A second tap is too late.
        tap(&bridge, 555, "Deny", &transport);
        assert_eq!(host.decided.lock().unwrap().len(), 1);
    }

    #[test]
    fn codex_always_allows_the_turn() {
        let (bridge, transport, host) = bridge(400.0);
        bridge.publish(&[waiting(true)]);
        bridge.sync();
        flush(&bridge);
        tap(&bridge, 555, "Always", &transport);
        assert_eq!(host.auto.lock().unwrap().as_slice(), ["s1"]);
        assert_eq!(host.decided.lock().unwrap()[0].2, Decision::Allow);
    }

    #[test]
    fn an_answer_on_the_computer_updates_the_message() {
        let (bridge, transport, _) = bridge(400.0);
        bridge.publish(&[waiting(false)]);
        bridge.sync();
        flush(&bridge);
        let mut answered = waiting(false);
        answered.permission = None;
        bridge.publish(&[answered]);
        bridge.sync();
        flush(&bridge);
        let edits: Vec<Value> = transport.calls.lock().unwrap().iter().filter(|(m, _)| m == "editMessageText").map(|(_, b)| b.clone()).collect();
        assert!(edits[0]["text"].as_str().unwrap().contains("Answered on the computer."));
        assert_eq!(edits[0]["reply_markup"]["inline_keyboard"], json!([]));
    }

    #[test]
    fn a_tap_from_an_earlier_run_does_nothing() {
        let (bridge, _, host) = bridge(400.0);
        bridge.publish(&[waiting(false)]);
        bridge.sync();
        bridge.handle(&json!({"update_id": 1, "callback_query": {"id": "q", "from": {"id": 555},
            "message": {"message_id": 101, "chat": {"id": 555}}, "data": "zzzzzz:1"}}));
        assert!(host.decided.lock().unwrap().is_empty());
    }

    #[test]
    fn linking_takes_only_the_right_code() {
        let transport = Arc::new(FakeTransport::default());
        let host = Arc::new(FakeHost::default());
        let bridge = Bridge::new(host.clone(), Arc::new(FakeApi(transport.clone())), Stored::default(), None, Settings::default());
        bridge.state.lock().unwrap().running = true;
        assert_eq!(bridge.use_token("nope").problem.as_deref(), Some("invalidToken"));
        let status = bridge.use_token(TOKEN);
        assert_eq!(status.bot.as_deref(), Some("openbar_test_bot"));
        let code = status.linking.unwrap().code;
        let message = |text: &str, chat_type: &str| json!({"update_id": 2, "message": {"message_id": 1, "date": 1000,
            "chat": {"id": 777, "type": chat_type}, "from": {"id": 777, "username": "me"}, "text": text}});
        bridge.handle(&message(&format!("/start {code}"), "group"));
        assert!(bridge.status().linked.is_none(), "never from a group");
        bridge.handle(&message("ABCDEFGH", "private"));
        assert!(bridge.status().linked.is_none());
        bridge.handle(&message(&format!("/start {code}"), "private"));
        assert_eq!(bridge.status().linked.as_deref(), Some("@me"));
        assert!(bridge.state.lock().unwrap().active());
        assert_eq!(*host.enabled.lock().unwrap(), Some(true), "Settings shows the switch on");
        bridge.unlink();
        assert_eq!(*host.enabled.lock().unwrap(), Some(false));
        assert!(bridge.status().linked.is_none() && bridge.status().bot.is_none());
    }

    #[test]
    fn tokens_look_like_botfathers() {
        assert!(valid_token(TOKEN));
        assert!(!valid_token("123:short"));
        assert!(!valid_token("abc:AAtesttokentesttokentesttokentest00"));
        assert!(!valid_token("123456789:AAtest token with spaces and more text"));
    }
}
