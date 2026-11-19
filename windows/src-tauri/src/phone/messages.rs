// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Phone approvals through a Telegram bot (#214): what needs no network and
//! no window. The settings, when a message may go out, what it says and
//! which buttons it carries, the linking code, and how a tap is read. The
//! Mac follows the same cases in `PhoneApprovalsSupport.swift`; both are
//! checked against `phone_cases.json`.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::sessions::file_diff::{Change, FileDiff, LineKind};
use crate::sessions::models::{Permission, Provider, Question, Session, SessionPhase, Time};
use crate::sessions::plan_modes::PlanMode;

/// What the person set for phone approvals, under `/sessions/phone*`.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    /// Minutes without keyboard or mouse input before messages go out.
    pub away_minutes: u32,
    /// Messages go out even while the person sits at the computer.
    pub send_always: bool,
    /// Between these minutes of the day messages arrive without a sound.
    pub quiet_hours: bool,
    pub quiet_from: u32,
    pub quiet_to: u32,
    /// Messages carry only the agent, the project and the kind of request.
    pub hide_code: bool,
    /// A message when a session finishes or stops on an error.
    pub tell_ends: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: false,
            away_minutes: 5,
            send_always: false,
            quiet_hours: false,
            quiet_from: 22 * 60,
            quiet_to: 7 * 60,
            hide_code: false,
            tell_ends: true,
        }
    }
}

impl Settings {
    pub fn from_prefs(value: &Value) -> Settings {
        let d = Settings::default();
        let flag = |name: &str, default: bool| {
            value
                .pointer(&format!("/sessions/{name}"))
                .and_then(Value::as_bool)
                .unwrap_or(default)
        };
        let number = |name: &str, default: u32| {
            value
                .pointer(&format!("/sessions/{name}"))
                .and_then(Value::as_u64)
                .map_or(default, |n| n.min(24 * 60 - 1) as u32)
        };
        Settings {
            enabled: flag("phoneEnabled", d.enabled),
            away_minutes: number("phoneAwayMinutes", d.away_minutes).max(1),
            send_always: flag("phoneSendAlways", d.send_always),
            quiet_hours: flag("phoneQuietHours", d.quiet_hours),
            quiet_from: number("phoneQuietFrom", d.quiet_from),
            quiet_to: number("phoneQuietTo", d.quiet_to),
            hide_code: flag("phoneHideCode", d.hide_code),
            tell_ends: flag("phoneTellEnds", d.tell_ends),
        }
    }
}

/// Away: the screen is locked, or nobody touched the keyboard or mouse for
/// the minutes set; or the person asked for messages all the time.
pub fn sends(settings: &Settings, locked: bool, idle: f64) -> bool {
    settings.send_always || locked || idle >= f64::from(settings.away_minutes.max(1)) * 60.0
}

/// Quiet hours, which may run past midnight: the message still goes out,
/// without a sound.
pub fn quiet(settings: &Settings, minute: u32) -> bool {
    if !settings.quiet_hours || settings.quiet_from == settings.quiet_to {
        return false;
    }
    if settings.quiet_from < settings.quiet_to {
        minute >= settings.quiet_from && minute < settings.quiet_to
    } else {
        minute >= settings.quiet_from || minute < settings.quiet_to
    }
}

// MARK: Linking

/// Letters and digits that can't be mistaken for one another.
pub const CODE_ALPHABET: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
pub const CODE_LENGTH: usize = 8;
pub const CODE_VALID_FOR: f64 = 10.0 * 60.0;
/// Wrong codes allowed before the code stops working.
pub const CODE_TRIES: u32 = 5;

/// Random bytes from the system, for codes and button runs.
fn random_bytes(count: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; count];
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Security::Cryptography::{
            BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
        };
        let _ = BCryptGenRandom(None, &mut bytes, BCRYPT_USE_SYSTEM_PREFERRED_RNG);
    }
    #[cfg(not(windows))]
    {
        use std::io::Read;
        if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
            let _ = file.read_exact(&mut bytes);
        }
    }
    bytes
}

fn random_text(alphabet: &str, length: usize) -> String {
    let letters: Vec<char> = alphabet.chars().collect();
    // Bytes over the largest multiple of the alphabet are drawn again, so
    // every letter is as likely.
    let limit = 256 - 256 % letters.len();
    let mut out = String::new();
    while out.chars().count() < length {
        for byte in random_bytes(length * 2) {
            if (byte as usize) < limit && out.chars().count() < length {
                out.push(letters[byte as usize % letters.len()]);
            }
        }
    }
    out
}

pub fn make_code() -> String {
    random_text(CODE_ALPHABET, CODE_LENGTH)
}

/// The code a message carries, in capitals without spaces or dashes:
/// sent as is, or by the `t.me/<bot>?start=<code>` link as `/start <code>`.
pub fn code_in(text: &str) -> Option<String> {
    let mut body = text.trim().to_owned();
    if body.to_lowercase().starts_with("/start") {
        body = body["/start".len()..].to_owned();
        if body.starts_with('@') {
            body = body
                .split_once(char::is_whitespace)
                .map(|(_, rest)| rest.to_owned())
                .unwrap_or_default();
        }
    }
    let code: String = body
        .to_uppercase()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    (code.chars().count() == CODE_LENGTH && code.chars().all(|c| CODE_ALPHABET.contains(c)))
        .then_some(code)
}

/// The link that opens the bot in Telegram with the code filled in.
pub fn link(bot: &str, code: &str) -> String {
    format!("https://t.me/{bot}?start={code}")
}

/// A button's data (at most 64 bytes): which run of Openbar made it and its
/// number in that run, so a tap on a message an earlier run sent can never
/// answer a request of this one.
pub fn new_run() -> String {
    random_text("abcdefghijkmnopqrstuvwxyz23456789", 6)
}

pub fn callback_data(run: &str, number: u64) -> String {
    format!("{run}:{number}")
}

pub fn parse_callback(data: &str) -> Option<(String, u64)> {
    let parts: Vec<&str> = data.split(':').collect();
    if parts.len() != 2 || parts[0].is_empty() {
        return None;
    }
    let number = parts[1].parse::<u64>().ok()?;
    Some((parts[0].to_owned(), number))
}

// MARK: Messages

/// What a button does.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Allow,
    /// Claude Code's rules for next time; for Codex, every request of the turn.
    Always,
    Deny,
    /// A plan approved into this mode, or plainly when it offers none.
    ApprovePlan(Option<PlanMode>),
    KeepPlanning,
    /// Picks (or for several, toggles) one option of one question.
    Pick(usize, usize),
    /// Sends the options picked so far.
    Send,
    Skip,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Button {
    pub title: String,
    pub action: Action,
}

/// A message to the phone: HTML text Telegram renders, and rows of buttons.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub text: String,
    pub buttons: Vec<Vec<Button>>,
}

/// How a request ended, told at the foot of its message.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Allowed,
    Always,
    Denied,
    PlanApproved(Option<String>),
    KeptPlanning,
    Answered(Vec<String>),
    Skipped,
    AnsweredElsewhere,
    Gone,
    Changed,
    Failed,
}

/// Telegram takes 4096 characters; the rest is room for the outcome line.
pub const LIMIT: usize = 3600;
/// The most diff lines a message shows.
pub const DIFF_LINES: usize = 40;
/// The most of a plan, or of a session's last words, a message shows.
pub const PROSE_LIMIT: usize = 1500;
pub const PLAN_TOOL: &str = "ExitPlanMode";

/// Only &, < and > mean anything in Telegram's HTML.
pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The text cut to whole characters within a count of UTF-16 units, the
/// length Telegram measures, and how many characters were left out.
pub fn clip(text: &str, limit: usize) -> (String, usize) {
    if utf16_len(text) <= limit {
        return (text.to_owned(), 0);
    }
    let mut used = 0;
    let mut end = 0;
    for (index, c) in text.char_indices() {
        let size = c.len_utf16();
        if used + size > limit {
            break;
        }
        used += size;
        end = index + c.len_utf8();
    }
    let shown = &text[..end];
    (
        shown.to_owned(),
        text.chars().count() - shown.chars().count(),
    )
}

pub fn agent_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "Claude Code",
        Provider::Codex => "Codex",
        Provider::Gemini => "Gemini CLI",
        Provider::Copilot => "Copilot CLI",
        Provider::Opencode => "OpenCode",
        Provider::Cursor => "Cursor",
        Provider::Pi => "Pi",
        Provider::Vibe => "Mistral Vibe",
        Provider::Devin => "Devin CLI",
        Provider::Antigravity => "Google Antigravity",
        Provider::Grok => "Grok CLI",
        Provider::Custom => "Other tool",
    }
}

pub fn header(session: &Session, name: &str) -> String {
    let mut line = format!(
        "<b>{}</b> · {}",
        escape(agent_name(session.provider)),
        escape(&session.project)
    );
    let name = name.trim();
    if !name.is_empty() && name != session.project {
        line.push_str(&format!("\n<i>{}</i>", escape(name)));
    }
    line
}

pub fn is_plan(ask: &Permission) -> bool {
    ask.tool == PLAN_TOOL
}

/// What the request is, in a line that holds no code.
pub fn kind(ask: &Permission) -> String {
    if !ask.questions.is_empty() {
        return if ask.questions.len() == 1 {
            "Asks you a question.".into()
        } else {
            format!("Asks you {} questions.", ask.questions.len())
        };
    }
    if is_plan(ask) {
        return "Has a plan for you to review.".into();
    }
    if !ask.diffs.is_empty() {
        return if ask.diffs.len() == 1 {
            "Wants to change a file.".into()
        } else {
            format!("Wants to change {} files.", ask.diffs.len())
        };
    }
    match ask.tool.as_str() {
        "Bash" | "Command" | "PowerShell" => "Wants to run a command.".into(),
        tool => format!("Wants to use {tool}."),
    }
}

/// Whether one tap on an option answers: one question, one choice.
pub fn answers_on_tap(ask: &Permission) -> bool {
    ask.questions.len() == 1 && !ask.questions[0].multi_select
}

pub type Picked = BTreeMap<usize, BTreeSet<usize>>;

/// The message for one request. `picked` holds the options toggled so far
/// for a question answered in more than one tap.
pub fn ask(
    ask: &Permission,
    session: &Session,
    name: &str,
    hide_code: bool,
    picked: &Picked,
) -> Message {
    let mut parts = vec![header(session, name), kind(ask)];
    if hide_code {
        parts.push(if ask.questions.is_empty() {
            "<i>Hidden because Don’t send code is on.</i>".into()
        } else {
            "<i>Don’t send code is on, so answer this one on the computer.</i>".into()
        });
    } else {
        let used = utf16_len(&parts.join("\n")) + 2;
        parts.push(body(ask, LIMIT.saturating_sub(used).max(200)));
    }
    Message {
        text: parts
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n"),
        buttons: buttons(ask, session, hide_code, picked),
    }
}

/// The request itself, cut to the room left.
pub fn body(ask: &Permission, room: usize) -> String {
    if !ask.questions.is_empty() {
        return questions(&ask.questions, room);
    }
    if is_plan(ask) {
        let (text, left) = clip(ask.detail.trim(), room.min(PROSE_LIMIT));
        return escape(&text)
            + &if left > 0 {
                format!("\n{}", more(left, "plan"))
            } else {
                String::new()
            };
    }
    if !ask.diffs.is_empty() {
        return diff(&ask.diffs, room);
    }
    let detail = ask.detail.trim_matches(|c| c == '\n' || c == '\r');
    if detail.is_empty() {
        return String::new();
    }
    // The island's preview already marks what it left out; the phone
    // counts both.
    let (text, cut) = clip(detail, room.saturating_sub(80));
    let left = cut
        + if cut > 0 {
            ask.content
                .as_ref()
                .and_then(|c| c.metadata(ask).remaining)
                .unwrap_or(0)
        } else {
            0
        };
    format!("<pre>{}</pre>", escape(&text))
        + &if left > 0 {
            format!("\n{}", more(left, "request"))
        } else {
            String::new()
        }
}

pub fn more(left: usize, what: &str) -> String {
    format!(
        "<i>… {left} more character{}. The full {what} is on the computer.</i>",
        if left == 1 { "" } else { "s" }
    )
}

pub fn questions(questions: &[Question], room: usize) -> String {
    let mut lines = Vec::new();
    for (index, question) in questions.iter().enumerate() {
        let number = if questions.len() > 1 {
            format!("{}. ", index + 1)
        } else {
            String::new()
        };
        lines.push(format!("<b>{number}{}</b>", escape(&question.question)));
        for option in &question.options {
            let mut line = format!("• {}", escape(&option.label));
            if let Some(description) = &option.description {
                line.push_str(&format!(" — {}", escape(description)));
            }
            lines.push(line);
        }
        if question.multi_select {
            lines.push("<i>Pick any, then Send.</i>".into());
        }
    }
    lines.push("<i>Or reply to this message with your own answer.</i>".into());
    let text = lines.join("\n");
    if utf16_len(&text) <= room {
        return text;
    }
    // Escaped text can't be cut inside an entity or a tag, so the plain
    // questions are shown instead.
    let plain = questions
        .iter()
        .map(|q| q.question.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let (cut, left) = clip(&plain, room.saturating_sub(80));
    escape(&cut)
        + &if left > 0 {
            format!("\n{}", more(left, "question"))
        } else {
            String::new()
        }
}

pub fn diff(diffs: &[FileDiff], room: usize) -> String {
    let mut lines = Vec::new();
    let mut shown = 0;
    let mut hidden = 0;
    for file in diffs {
        let verb = match file.change {
            Change::Delete => "deleted",
            Change::Add => "new",
            _ => "edit",
        };
        lines.push(format!("{verb} {}", file.path));
        for line in &file.lines {
            if shown >= DIFF_LINES {
                hidden += 1;
                continue;
            }
            shown += 1;
            lines.push(match line.kind {
                LineKind::Added => format!("+ {}", line.text),
                LineKind::Removed => format!("- {}", line.text),
                LineKind::Context => format!("  {}", line.text),
                LineKind::Gap => "  ⋯".into(),
            });
        }
    }
    let (text, left) = clip(&lines.join("\n"), room.saturating_sub(80));
    let note = if hidden > 0 || left > 0 {
        "\n<i>The rest of the change is on the computer.</i>"
    } else {
        ""
    };
    format!("<pre>{}</pre>{note}", escape(&text))
}

pub fn plan_label(mode: PlanMode) -> &'static str {
    match mode {
        PlanMode::Auto => "Auto mode",
        PlanMode::AcceptEdits => "Accept edits",
        PlanMode::Bypass => "Bypass",
        PlanMode::Manual => "Manual",
    }
}

fn button(title: impl Into<String>, action: Action) -> Button {
    Button {
        title: title.into(),
        action,
    }
}

/// The buttons the island offers for the same request.
pub fn buttons(
    ask: &Permission,
    session: &Session,
    hide_code: bool,
    picked: &Picked,
) -> Vec<Vec<Button>> {
    if !ask.questions.is_empty() {
        let skip = button("Skip", Action::Skip);
        if hide_code {
            return vec![vec![skip]];
        }
        let several = ask.questions.len() > 1;
        let mut rows = Vec::new();
        for (q, question) in ask.questions.iter().enumerate() {
            for (o, option) in question.options.iter().enumerate() {
                let mark = if picked.get(&q).is_some_and(|p| p.contains(&o)) {
                    "✓ "
                } else {
                    ""
                };
                let number = if several {
                    format!("{}. ", q + 1)
                } else {
                    String::new()
                };
                rows.push(vec![button(
                    format!("{mark}{number}{}", option.label),
                    Action::Pick(q, o),
                )]);
            }
        }
        if answers_on_tap(ask) {
            rows.push(vec![skip]);
            return rows;
        }
        let complete =
            (0..ask.questions.len()).all(|q| picked.get(&q).is_some_and(|p| !p.is_empty()));
        rows.push(if complete {
            vec![skip, button("Send", Action::Send)]
        } else {
            vec![skip]
        });
        return rows;
    }
    if is_plan(ask) {
        let mut rows = vec![vec![button("Keep planning", Action::KeepPlanning)]];
        if let Some(first) = ask.plan_modes.first() {
            rows.push(vec![button(
                format!("Approve · {}", plan_label(*first)),
                Action::ApprovePlan(Some(*first)),
            )]);
            let others: Vec<Button> = ask.plan_modes[1..]
                .iter()
                .map(|m| button(plan_label(*m), Action::ApprovePlan(Some(*m))))
                .collect();
            if !others.is_empty() {
                rows.push(others);
            }
        } else {
            rows[0].push(button("Approve", Action::ApprovePlan(None)));
        }
        return rows;
    }
    let mut row = vec![button("Deny", Action::Deny)];
    if session.codex_server() || ask.offers_always {
        row.push(button("Always", Action::Always));
    }
    row.push(button("Allow", Action::Allow));
    vec![row]
}

/// The answers picked and typed, keyed by question text, as the island sends them.
pub fn answers(
    ask: &Permission,
    picked: &Picked,
    typed: &BTreeMap<usize, String>,
) -> Option<BTreeMap<String, Vec<String>>> {
    if ask.questions.is_empty() {
        return None;
    }
    let mut out = BTreeMap::new();
    for (q, question) in ask.questions.iter().enumerate() {
        let mut chosen: Vec<String> = question
            .options
            .iter()
            .enumerate()
            .filter(|(o, _)| picked.get(&q).is_some_and(|p| p.contains(o)))
            .map(|(_, option)| option.label.clone())
            .collect();
        let own = typed
            .get(&q)
            .map(|t| t.trim().to_owned())
            .unwrap_or_default();
        if !own.is_empty() {
            if question.multi_select {
                chosen.push(own);
            } else {
                chosen = vec![own];
            }
        }
        if !question.multi_select {
            chosen.truncate(1);
        }
        if chosen.is_empty() {
            return None;
        }
        out.insert(question.question.clone(), chosen);
    }
    Some(out)
}

/// The labels an answer gave, in the questions' order.
pub fn labels(answers: &BTreeMap<String, Vec<String>>, ask: &Permission) -> Vec<String> {
    ask.questions
        .iter()
        .flat_map(|q| answers.get(&q.question).cloned().unwrap_or_default())
        .collect()
}

/// The line a request's message ends with once it is settled.
pub fn outcome(outcome: &Outcome, minute: u32) -> String {
    let time = format!("{:02}:{:02}", minute / 60, minute % 60);
    let text = match outcome {
        Outcome::Allowed => "<b>Allowed from your phone</b>".to_owned(),
        Outcome::Always => "<b>Always allowed from your phone</b>".to_owned(),
        Outcome::Denied => "<b>Denied from your phone</b>".to_owned(),
        Outcome::PlanApproved(mode) => format!(
            "<b>Plan approved from your phone</b>{}",
            mode.as_ref()
                .map(|m| format!(" · {}", escape(m)))
                .unwrap_or_default()
        ),
        Outcome::KeptPlanning => "<b>Kept planning, from your phone</b>".to_owned(),
        Outcome::Answered(answers) => {
            let joined = answers.join(", ");
            if joined.is_empty() {
                "<b>Answered from your phone</b>".to_owned()
            } else {
                format!(
                    "<b>Answered from your phone</b>: {}",
                    escape(&clip(&joined, 200).0)
                )
            }
        }
        Outcome::Skipped => "<b>Skipped from your phone</b>".to_owned(),
        Outcome::AnsweredElsewhere => "<i>Answered on the computer.</i>".to_owned(),
        Outcome::Gone => "<i>No longer waiting.</i>".to_owned(),
        Outcome::Changed => "<i>The request changed. Look at it on the computer.</i>".to_owned(),
        Outcome::Failed => {
            "<i>That didn’t reach the session. Look at it on the computer.</i>".to_owned()
        }
    };
    format!("{text} · {time}")
}

/// How a turn ended, for its message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Finished,
    Error,
    Limit,
}

/// The end of a session's last turn, as the session sounds tell it
/// (`terminalSound` in sessions/waitingSound.ts): its kind and when.
pub fn terminal(session: &Session) -> Option<(End, Time)> {
    if session.phase.is_working() || session.phase.is_waiting() {
        return None;
    }
    let began = session.turn_started.unwrap_or(f64::NEG_INFINITY);
    if session.phase == SessionPhase::Limited
        && let Some(stop) = session.limit_stop.as_ref().filter(|s| s.at >= began)
    {
        return Some((End::Limit, stop.at));
    }
    if let (Some(failed), Some(ended)) = (session.failed_at, session.turn_ended_at)
        && failed >= began
        && failed <= ended
    {
        return Some((End::Error, failed));
    }
    if session.turn_completed
        && let Some(ended) = session.turn_ended_at.filter(|e| *e >= began)
    {
        return Some((End::Finished, ended));
    }
    None
}

/// A session finished its turn, stopped on an error or hit a usage limit.
pub fn ended(end: End, session: &Session, name: &str, hide_code: bool) -> Message {
    let what = match end {
        End::Error => "Stopped on an error.",
        End::Limit => "Stopped at its usage limit.",
        End::Finished => "Finished.",
    };
    let mut parts = vec![header(session, name), what.to_owned()];
    if !hide_code
        && end == End::Finished
        && let Some(last) = session
            .last_text
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
    {
        let (text, left) = clip(last, PROSE_LIMIT);
        parts.push(format!(
            "<blockquote>{}{}</blockquote>",
            escape(&text),
            if left > 0 { "…" } else { "" }
        ));
    }
    parts.push("<i>Reply to this message to send it your next prompt.</i>".into());
    Message {
        text: parts.join("\n\n"),
        buttons: Vec::new(),
    }
}

pub fn linked(computer: &str) -> String {
    format!(
        "<b>Linked to Openbar on {}.</b>\n\nWhile you’re away from the computer, sessions that need you show up here. Tap a button to answer, or reply to a message to write to its session.",
        escape(computer)
    )
}

pub const TEST: &str = "<b>Test message from Openbar.</b>\n\nPhone approvals are working, and requests will show up here while you’re away from the computer.";
pub const WRONG_CODE: &str = "That code doesn’t match. Check the code Openbar shows in Settings.";
pub const EARLIER_RUN: &str =
    "This button is from before Openbar restarted. Look at the request on the computer.";
pub const SETTLED: &str = "Already settled.";
pub const WHICH_SESSION: &str = "Reply to a session’s message so Openbar knows where this goes.";
pub const NO_SESSION: &str = "No session can take a reply right now.";
pub const PICK_THE_REST: &str = "Saved. Pick answers for the other questions, then tap Send.";

pub fn sent_to(name: &str) -> String {
    format!("Sent to {}.", escape(name))
}

pub fn reply_failed(refusal: &str, name: &str) -> String {
    if refusal == "waiting" {
        format!(
            "{} is waiting on a request. Answer that first.",
            escape(name)
        )
    } else {
        format!(
            "Couldn’t reach {}. Look at it on the computer.",
            escape(name)
        )
    }
}

/// Whether a reply typed into the session could reach it now (`canReply`
/// in sessions/voice.ts).
pub fn can_reply(session: &Session) -> bool {
    session.accepts_replies
        && !session.phase.is_waiting()
        && session.waiting_for.is_none()
        && !session.background.as_ref().is_some_and(|b| b.ended)
}

/// Where a typed message goes: the session whose message it replies to,
/// else the only session that can take a reply.
pub fn reply_target(replying_to: Option<&str>, sessions: &[Session]) -> Option<String> {
    if let Some(id) = replying_to {
        return sessions.iter().any(|s| s.id == id).then(|| id.to_owned());
    }
    let open: Vec<&Session> = sessions
        .iter()
        .filter(|s| s.id != crate::sessions::sample::ID && can_reply(s))
        .collect();
    (open.len() == 1).then(|| open[0].id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::file_diff::Line;
    use crate::sessions::models::QuestionOption;
    use serde_json::json;

    fn cases() -> Value {
        serde_json::from_str(include_str!("phone_cases.json")).unwrap()
    }

    fn session(json: &Value, asks: Vec<Permission>, last_text: Option<String>) -> Session {
        let mut s = crate::sessions::sample::waiting(1, "/w", 0.0);
        s.id = format!("s-{}", json["project"].as_str().unwrap_or("p"));
        s.provider = match json["provider"].as_str() {
            Some("codex") => Provider::Codex,
            _ => Provider::Claude,
        };
        s.project = json["project"].as_str().unwrap_or("p").to_owned();
        s.name = json["name"].as_str().unwrap_or("").to_owned();
        s.via = None;
        s.permission = asks.first().cloned();
        s.later_asks = asks.into_iter().skip(1).collect();
        s.last_text = last_text;
        s
    }

    fn permission(json: &Value) -> Permission {
        let text = json["detail"]
            .as_str()
            .unwrap_or("")
            .repeat(json["repeat"].as_u64().unwrap_or(1) as usize);
        let mut p = Permission::new(
            "r1",
            json["tool"].as_str().unwrap_or("Bash"),
            None,
            json["offersAlways"].as_bool().unwrap_or(false),
            0.0,
        );
        p.set_content(&text, true, Some(text.chars().count()));
        p.questions = json["questions"]
            .as_array()
            .map(|qs| {
                qs.iter()
                    .map(|q| Question {
                        question: q["question"].as_str().unwrap_or("").into(),
                        header: None,
                        options: q["options"]
                            .as_array()
                            .map(|os| {
                                os.iter()
                                    .map(|o| QuestionOption {
                                        label: o["label"].as_str().unwrap_or("").into(),
                                        description: o["description"].as_str().map(Into::into),
                                        preview: None,
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                        multi_select: q["multiSelect"].as_bool().unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default();
        p.plan_modes = json["planModes"]
            .as_array()
            .map(|ms| {
                ms.iter()
                    .filter_map(|m| serde_json::from_value(m.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        p.diffs = json["diffs"]
            .as_array()
            .map(|ds| {
                ds.iter()
                    .map(|d| FileDiff {
                        path: d["path"].as_str().unwrap_or("").into(),
                        change: match d["change"].as_str() {
                            Some("add") => Change::Add,
                            Some("delete") => Change::Delete,
                            Some("replace") => Change::Replace,
                            _ => Change::Update,
                        },
                        lines: d["lines"]
                            .as_array()
                            .map(|ls| {
                                ls.iter()
                                    .map(|l| Line {
                                        kind: match l[0].as_str() {
                                            Some("added") => LineKind::Added,
                                            Some("removed") => LineKind::Removed,
                                            Some("gap") => LineKind::Gap,
                                            _ => LineKind::Context,
                                        },
                                        text: l[1].as_str().unwrap_or("").into(),
                                        number: None,
                                        gap: None,
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                        ..Default::default()
                    })
                    .collect()
            })
            .unwrap_or_default();
        p
    }

    #[test]
    fn away_quiet_codes_and_callbacks_follow_the_shared_cases() {
        let cases = cases();
        for item in cases["away"].as_array().unwrap() {
            let settings = Settings {
                away_minutes: item["awayMinutes"].as_u64().unwrap() as u32,
                send_always: item["sendAlways"].as_bool().unwrap(),
                ..Default::default()
            };
            assert_eq!(
                sends(
                    &settings,
                    item["locked"].as_bool().unwrap(),
                    item["idle"].as_f64().unwrap()
                ),
                item["expect"].as_bool().unwrap(),
                "{item}"
            );
        }
        for item in cases["quiet"].as_array().unwrap() {
            let settings = Settings {
                quiet_hours: item["on"].as_bool().unwrap(),
                quiet_from: item["from"].as_u64().unwrap() as u32,
                quiet_to: item["to"].as_u64().unwrap() as u32,
                ..Default::default()
            };
            assert_eq!(
                quiet(&settings, item["minute"].as_u64().unwrap() as u32),
                item["expect"].as_bool().unwrap(),
                "{item}"
            );
        }
        for item in cases["codes"].as_array().unwrap() {
            assert_eq!(
                code_in(item["text"].as_str().unwrap()).as_deref(),
                item["expect"].as_str(),
                "{item}"
            );
        }
        for item in cases["callbacks"].as_array().unwrap() {
            let got = parse_callback(item["data"].as_str().unwrap());
            assert_eq!(
                got.as_ref().map(|g| g.0.as_str()),
                item["run"].as_str(),
                "{item}"
            );
            if let Some((_, number)) = got {
                assert_eq!(Some(number), item["number"].as_u64());
            }
        }
        let code = make_code();
        assert_eq!(code_in(&code), Some(code.clone()));
        assert_ne!(make_code(), make_code());
        let run = new_run();
        assert_eq!(
            parse_callback(&callback_data(&run, 42)),
            Some((run.clone(), 42))
        );
        assert!(callback_data(&run, 999_999).len() <= 64);
    }

    #[test]
    fn escaping_and_clipping_follow_the_shared_cases() {
        let cases = cases();
        for item in cases["escape"].as_array().unwrap() {
            assert_eq!(
                escape(item["text"].as_str().unwrap()),
                item["expect"].as_str().unwrap()
            );
        }
        for item in cases["clip"].as_array().unwrap() {
            let (shown, left) = clip(
                item["text"].as_str().unwrap(),
                item["limit"].as_u64().unwrap() as usize,
            );
            assert_eq!(shown, item["shown"].as_str().unwrap(), "{item}");
            assert_eq!(left as u64, item["left"].as_u64().unwrap(), "{item}");
        }
    }

    #[test]
    fn messages_and_buttons_follow_the_shared_cases() {
        for item in cases()["messages"].as_array().unwrap() {
            let name = item["name"].as_str().unwrap();
            let p = permission(&item["ask"]);
            let s = session(&item["session"], vec![p.clone()], None);
            let mut picked = Picked::new();
            if let Some(map) = item["picked"].as_object() {
                for (key, value) in map {
                    picked.insert(
                        key.parse().unwrap(),
                        value
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_u64().unwrap() as usize)
                            .collect(),
                    );
                }
            }
            let message = ask(
                &p,
                &s,
                item["session"]["name"].as_str().unwrap_or(""),
                item["hideCode"].as_bool().unwrap_or(false),
                &picked,
            );
            if let Some(text) = item["text"].as_str() {
                assert_eq!(message.text, text, "{name}");
            }
            for part in item["contains"].as_array().into_iter().flatten() {
                assert!(
                    message.text.contains(part.as_str().unwrap()),
                    "{name}: {}",
                    message.text
                );
            }
            if let Some(most) = item["maxLength"].as_u64() {
                assert!(utf16_len(&message.text) as u64 <= most, "{name}");
            }
            if let Some(expected) = item["buttons"].as_array() {
                let titles: Vec<Vec<String>> = message
                    .buttons
                    .iter()
                    .map(|r| r.iter().map(|b| b.title.clone()).collect())
                    .collect();
                let expected: Vec<Vec<String>> = expected
                    .iter()
                    .map(|r| {
                        r.as_array()
                            .unwrap()
                            .iter()
                            .map(|t| t.as_str().unwrap().to_owned())
                            .collect()
                    })
                    .collect();
                assert_eq!(titles, expected, "{name}");
            }
            if let Some(expected) = item["answers"].as_object() {
                let got = answers(&p, &picked, &BTreeMap::new()).unwrap();
                let want: BTreeMap<String, Vec<String>> = expected
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::from_value(v.clone()).unwrap()))
                    .collect();
                assert_eq!(got, want, "{name}");
            }
        }
    }

    #[test]
    fn outcomes_and_ends_follow_the_shared_cases() {
        let cases = cases();
        for item in cases["outcomes"].as_array().unwrap() {
            let o = match item["outcome"].as_str().unwrap() {
                "allowed" => Outcome::Allowed,
                "always" => Outcome::Always,
                "denied" => Outcome::Denied,
                "planApproved" => Outcome::PlanApproved(item["mode"].as_str().map(Into::into)),
                "keptPlanning" => Outcome::KeptPlanning,
                "answered" => {
                    Outcome::Answered(serde_json::from_value(item["answers"].clone()).unwrap())
                }
                "skipped" => Outcome::Skipped,
                "answeredElsewhere" => Outcome::AnsweredElsewhere,
                "gone" => Outcome::Gone,
                "changed" => Outcome::Changed,
                _ => Outcome::Failed,
            };
            assert_eq!(
                outcome(&o, item["minute"].as_u64().unwrap() as u32),
                item["expect"].as_str().unwrap()
            );
        }
        for item in cases["ended"].as_array().unwrap() {
            let end = match item["kind"].as_str().unwrap() {
                "error" => End::Error,
                "limit" => End::Limit,
                _ => End::Finished,
            };
            let s = session(
                &item["session"],
                vec![],
                item["lastText"].as_str().map(Into::into),
            );
            let got = ended(
                end,
                &s,
                item["session"]["name"].as_str().unwrap_or(""),
                item["hideCode"].as_bool().unwrap(),
            );
            assert_eq!(got.text, item["expect"].as_str().unwrap());
            assert!(got.buttons.is_empty());
        }
    }

    #[test]
    fn settings_come_from_the_prefs() {
        let s = Settings::from_prefs(
            &json!({"sessions": {"phoneEnabled": true, "phoneAwayMinutes": 0,
            "phoneQuietFrom": 99999, "phoneHideCode": true}}),
        );
        assert!(s.enabled && s.hide_code && s.tell_ends);
        assert_eq!(s.away_minutes, 1, "at least a minute");
        assert_eq!(s.quiet_from, 24 * 60 - 1);
        assert_eq!(Settings::from_prefs(&json!({})), Settings::default());
    }

    #[test]
    fn replies_go_only_where_they_point() {
        let a = session(&json!({"project": "a"}), vec![], None);
        let b = session(&json!({"project": "b"}), vec![], None);
        let sessions = vec![a, b];
        assert_eq!(reply_target(Some("s-b"), &sessions).as_deref(), Some("s-b"));
        assert_eq!(reply_target(Some("gone"), &sessions), None);
    }
}
