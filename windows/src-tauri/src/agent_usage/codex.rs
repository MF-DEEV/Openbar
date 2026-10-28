// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Codex's account limits and banked resets through `codex app-server`
//! over stdio (AgentCodexServer.swift, AgentCodexResetService.swift; spec 03
//! §3.3, §3.4). The island's sessions client talks to the shared app-server
//! daemon over its socket; this one starts its own short-lived app-server,
//! as the Mac does, so a check never depends on that daemon running.
//!
//! `account/rateLimitResetCredit/consume` spends a real reset. It runs only
//! from the Resets card's confirmed button, keeps its idempotency key for
//! ten minutes so a retry never spends twice, and is never run by tests.

use super::cli::{self, Conversation};
use super::model::{LimitSource, Limits, Provider, Time};
use super::parse::{ACCOUNT_KEYS, codex_windows, int, is_main_bucket, seconds};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;

const CHECK_TIMEOUT: Duration = Duration::from_secs(20);
const REDEEM_TIMEOUT: Duration = Duration::from_secs(30);
/// A reading, or a failure only the person can fix, holds this long.
pub const FRESHNESS: f64 = 5.0 * 60.0;
pub const RETRY_AFTER: f64 = 60.0;
/// An attempt that got no answer keeps its key this long.
pub const RETRY_WINDOW: f64 = 10.0 * 60.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Missing,
    NeedsSignIn,
    Outdated,
    Refused,
    Unreachable,
}

impl Failure {
    pub fn key(self) -> &'static str {
        match self {
            Failure::Missing => "missing",
            Failure::NeedsSignIn => "needsSignIn",
            Failure::Outdated => "outdated",
            Failure::Refused => "refused",
            Failure::Unreachable => "unreachable",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Reset,
    NothingToReset,
    NoCredit,
    AlreadyRedeemed,
}

impl Outcome {
    pub fn key(self) -> &'static str {
        match self {
            Outcome::Reset => "reset",
            Outcome::NothingToReset => "nothingToReset",
            Outcome::NoCredit => "noCredit",
            Outcome::AlreadyRedeemed => "alreadyRedeemed",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Credit {
    pub id: String,
    pub expires_at: Option<Time>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    pub available: i64,
    pub credits: Vec<Credit>,
    pub limits: Option<Limits>,
    pub checked_at: Time,
}

/// `codex` on Windows: the npm shim, a native install, then PATH (§3.2).
pub fn executable() -> Option<PathBuf> {
    let exts = cli::extensions();
    let mut candidates = Vec::new();
    if let Some(home) = super::files::home() {
        candidates.push(home.join(".local").join("bin").join("codex.exe"));
    }
    candidates.extend(cli::in_dirs(&cli::search_path(), "codex", &exts));
    candidates.extend(cli::in_dirs(&cli::tool_dirs(), "codex", &exts));
    cli::first_file(candidates, &[])
}

pub fn failure(error: Option<&Value>) -> Failure {
    let message = error
        .and_then(|e| e.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();
    if error.and_then(|e| e.get("code")).and_then(Value::as_i64) == Some(-32601)
        || message.contains("unknown variant")
        || message.contains("method not found")
    {
        Failure::Outdated
    } else if message.contains("authentication") || message.contains("not logged in") {
        Failure::NeedsSignIn
    } else {
        Failure::Refused
    }
}

/// API-key and cloud accounts have no plan to limit.
pub fn sign_in_failure(account: &Value) -> Option<Failure> {
    (account.pointer("/account/type").and_then(Value::as_str) != Some("chatgpt"))
        .then_some(Failure::NeedsSignIn)
}

pub fn limits(result: &Value, observed: Time) -> Option<Limits> {
    let snapshot = result
        .pointer("/rateLimitsByLimitId/codex")
        .filter(|s| s.is_object())
        .or_else(|| {
            result
                .get("rateLimits")
                .filter(|s| s.is_object() && is_main_bucket(s))
        })?;
    let windows = codex_windows(snapshot, observed, &ACCOUNT_KEYS);
    (!windows.is_empty()).then_some(Limits {
        provider: Provider::Codex,
        windows,
        observed_at: observed,
        source: LimitSource::Account,
    })
}

/// None for a Codex too old for resets.
pub fn summary(result: &Value, now: Time) -> Option<Summary> {
    let credits = result.get("rateLimitResetCredits")?;
    let mut listed: Vec<Credit> = credits
        .get("credits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let id = c.get("id")?.as_str().filter(|s| !s.is_empty())?;
            if c.get("status")
                .and_then(Value::as_str)
                .unwrap_or("available")
                != "available"
            {
                return None;
            }
            let expires = seconds(c.get("expiresAt"));
            if expires.is_some_and(|e| e <= now) {
                return None;
            }
            Some(Credit {
                id: id.into(),
                expires_at: expires,
            })
        })
        .collect();
    listed.sort_by(|a, b| {
        a.expires_at
            .unwrap_or(f64::MAX)
            .total_cmp(&b.expires_at.unwrap_or(f64::MAX))
    });
    let available = (listed.len() as i64).max(int(credits.get("availableCount")));
    Some(Summary {
        available,
        credits: listed,
        limits: limits(result, now),
        checked_at: now,
    })
}

pub fn outcome(result: &Value) -> Option<Outcome> {
    match result.get("outcome")?.as_str()? {
        "reset" => Some(Outcome::Reset),
        "nothingToReset" => Some(Outcome::NothingToReset),
        "noCredit" => Some(Outcome::NoCredit),
        "alreadyRedeemed" => Some(Outcome::AlreadyRedeemed),
        _ => None,
    }
}

struct Server {
    channel: Conversation,
    last_id: i64,
}

impl Server {
    fn start(timeout: Duration) -> Result<Server, Failure> {
        let executable = executable().ok_or(Failure::Missing)?;
        let env = vec![("PATH".to_string(), cli::child_path(&executable))];
        // Account metadata needs no plugins or marketplace updates.
        let channel = Conversation::start(
            &executable,
            &["-c", "features.plugins=false", "app-server"],
            &env,
            &[],
            timeout,
        )
        .ok_or(Failure::Unreachable)?;
        let mut server = Server {
            channel,
            last_id: 0,
        };
        server.ask("initialize", json!({"clientInfo": {"name": "vorssaint", "title": "Vorssaint", "version": env!("CARGO_PKG_VERSION")}, "capabilities": null}))?;
        if !server.channel.send(&json!({"method": "initialized"})) {
            return Err(Failure::Unreachable);
        }
        Ok(server)
    }

    fn ask(&mut self, method: &str, params: Value) -> Result<Value, Failure> {
        self.last_id += 1;
        let id = self.last_id;
        if !self
            .channel
            .send(&json!({"id": id, "method": method, "params": params}))
        {
            return Err(Failure::Unreachable);
        }
        response(&mut || self.channel.next(), id)
    }

    fn summary(&mut self, now: Time) -> Result<Summary, Failure> {
        let result = self.ask("account/rateLimits/read", json!({}))?;
        summary(&result, now).ok_or(Failure::Outdated)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.channel.end(Duration::from_secs(2));
    }
}

/// The answer to `id`; notifications and the server's own requests are skipped.
pub fn response(next: &mut dyn FnMut() -> Option<Value>, id: i64) -> Result<Value, Failure> {
    while let Some(message) = next() {
        if message.get("method").is_some() || message.get("id").and_then(Value::as_i64) != Some(id)
        {
            continue;
        }
        if let Some(result) = message.get("result").filter(|r| r.is_object()) {
            return Ok(result.clone());
        }
        return Err(failure(message.get("error")));
    }
    Err(Failure::Unreachable)
}

/// `account/read`, then `account/rateLimits/read`.
pub fn check(now: &dyn Fn() -> Time) -> Result<Summary, Failure> {
    let mut server = Server::start(CHECK_TIMEOUT)?;
    let account = server.ask("account/read", json!({}))?;
    if let Some(failure) = sign_in_failure(&account) {
        return Err(failure);
    }
    server.summary(now())
}

/// Spends the reset that expires first, with `key` for idempotency.
pub fn redeem(
    credit: Option<&str>,
    key: &str,
    now: &dyn Fn() -> Time,
) -> (Result<Outcome, Failure>, Option<Summary>) {
    let mut server = match Server::start(REDEEM_TIMEOUT) {
        Ok(server) => server,
        Err(failure) => return (Err(failure), None),
    };
    let mut params = json!({"idempotencyKey": key});
    if let Some(credit) = credit {
        params["creditId"] = Value::from(credit);
    }
    let outcome = server
        .ask("account/rateLimitResetCredit/consume", params)
        .and_then(|r| outcome(&r).ok_or(Failure::Unreachable));
    let summary = server.summary(now()).ok();
    (outcome, summary)
}

/// A random UUID (v4) for the idempotency key.
pub fn uuid() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).unwrap_or_default();
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limits_and_banked_resets_are_read_from_the_answer() {
        let result = json!({
            "rateLimitsByLimitId": {"codex": {"primary": {"usedPercent": 9, "windowDurationMins": 300, "resetsAt": 1_789_778_369}, "secondary": {"usedPercent": 6, "windowDurationMins": 10080, "resetsAt": 1_789_845_426}}},
            "rateLimitResetCredits": {"availableCount": 3, "credits": [
                {"id": "late", "status": "available", "expiresAt": 1_790_500_000},
                {"id": "soon", "status": "available", "expiresAt": 1_790_100_000},
                {"id": "used", "status": "redeemed", "expiresAt": 1_790_100_000},
                {"id": "gone", "status": "available", "expiresAt": 1_700_000_000}]}
        });
        let s = summary(&result, 1_790_000_000.0).unwrap();
        assert_eq!(s.available, 3);
        assert_eq!(
            s.credits.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["soon", "late"]
        );
        let limits = s.limits.unwrap();
        assert_eq!(
            limits
                .windows
                .iter()
                .map(|w| w.id.as_str())
                .collect::<Vec<_>>(),
            ["codex.300", "codex.10080"]
        );
        assert_eq!(limits.source, LimitSource::Account);
        assert!(
            summary(&json!({"rateLimits": {}}), 0.0).is_none(),
            "too old for resets"
        );
        let single = json!({"rateLimits": {"limitId": "codex", "primary": {"usedPercent": 50, "windowDurationMins": 300}}});
        assert_eq!(limits_of(&single), 1);
        let other = json!({"rateLimits": {"limitId": "gpt-x", "primary": {"usedPercent": 50, "windowDurationMins": 300}}});
        assert_eq!(limits_of(&other), 0);
    }

    fn limits_of(v: &Value) -> usize {
        limits(v, 0.0).map_or(0, |l| l.windows.len())
    }

    #[test]
    fn responses_skip_notifications_and_errors_are_mapped() {
        let lines = [
            json!({"method": "account/updated", "params": {}}),
            json!({"id": 1, "result": {}}),
            json!({"id": 2, "result": {"outcome": "reset"}}),
        ];
        let mut it = lines.into_iter();
        assert_eq!(
            outcome(&response(&mut || it.next(), 2).unwrap()),
            Some(Outcome::Reset)
        );
        assert_eq!(
            failure(Some(&json!({"code": -32601, "message": "x"}))),
            Failure::Outdated
        );
        assert_eq!(
            failure(Some(&json!({"message": "unknown variant `account/x`"}))),
            Failure::Outdated
        );
        assert_eq!(
            failure(Some(&json!({"message": "Not logged in"}))),
            Failure::NeedsSignIn
        );
        assert_eq!(
            failure(Some(&json!({"message": "rate limited"}))),
            Failure::Refused
        );
        assert_eq!(
            sign_in_failure(&json!({"account": {"type": "apiKey"}})),
            Some(Failure::NeedsSignIn)
        );
        assert_eq!(
            sign_in_failure(&json!({"account": {"type": "chatgpt", "planType": "plus"}})),
            None
        );
        assert_eq!(uuid().len(), 36);
    }
}
