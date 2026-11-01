// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Claude's plan and account limits (AgentClaudeServer.swift; spec 03 §2.1.2,
//! §2.2). The limits come from the person's own `claude` CLI answering
//! `get_usage` headless, as `/usage` does; the app never reads Claude's
//! credentials. Only `oauthAccount` and `cachedUsageUtilization.fetchedAtMs`
//! are read from `.claude.json`.

use super::cli::{self, Conversation};
use super::model::{LimitSource, LimitWindow, Limits, Provider, Time, WindowKind};
use super::parse::timestamp;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// One deadline for every answer of a check.
const TIMEOUT: Duration = Duration::from_secs(25);
/// A saved copy older than this before the check began answered for the account.
const REUSE: f64 = 60.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Missing,
    NeedsSignIn,
    Outdated,
    Unreachable,
}

impl Failure {
    pub fn key(self) -> &'static str {
        match self {
            Failure::Missing => "missing",
            Failure::NeedsSignIn => "needsSignIn",
            Failure::Outdated => "outdated",
            Failure::Unreachable => "unreachable",
        }
    }
}

/// `%USERPROFILE%\.claude.json`, or inside `CLAUDE_CONFIG_DIR` when set.
pub fn profile_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty()) {
        let inside = PathBuf::from(dir).join(".claude.json");
        if inside.is_file() {
            return Some(inside);
        }
    }
    super::files::home().map(|h| h.join(".claude.json"))
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    pub organization: Option<String>,
    pub organization_type: Option<String>,
    pub rate_limit_tier: Option<String>,
    pub signed_in: bool,
    pub usage_fetched: Option<Time>,
    /// Whose plan the limits are.
    pub email: Option<String>,
}

/// The account fields of `.claude.json`; nothing else in it is kept.
pub fn profile(bytes: &[u8]) -> Profile {
    let Ok(json) = serde_json::from_slice::<Value>(bytes) else {
        return Profile::default();
    };
    let account = json.get("oauthAccount").filter(|a| a.is_object());
    let field = |k: &str| {
        account
            .and_then(|a| a.get(k))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let fetched = json
        .pointer("/cachedUsageUtilization/fetchedAtMs")
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite() && *n > 0.0)
        .map(|ms| ms / 1000.0);
    Profile {
        organization: field("organizationUuid"),
        organization_type: field("organizationType"),
        rate_limit_tier: field("organizationRateLimitTier").or_else(|| field("userRateLimitTier")),
        signed_in: account.is_some(),
        usage_fetched: fetched,
        email: field("emailAddress"),
    }
}

pub fn read_profile() -> Profile {
    profile_path()
        .and_then(|p| std::fs::read(p).ok())
        .map(|b| profile(&b))
        .unwrap_or_default()
}

/// The person's `claude`, never the Claude desktop app's bundled copy (§13.9).
pub fn executable() -> Option<PathBuf> {
    let home = super::files::home();
    let exts = cli::extensions();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(home) = &home {
        candidates.push(home.join(".local").join("bin").join("claude.exe"));
        candidates.push(home.join(".claude").join("local").join("claude.exe"));
        candidates.push(home.join(".claude").join("local").join("claude.cmd"));
    }
    candidates.extend(cli::in_dirs(&cli::search_path(), "claude", &exts));
    candidates.extend(cli::in_dirs(&cli::tool_dirs(), "claude", &exts));
    cli::first_file(
        candidates,
        &["anthropicclaude", "\\claude\\app-", "windowsapps\\claude"],
    )
}

/// Claude Code's settings override the environment; these opt-outs and
/// network settings pass on to the check (§2.2).
const PASSED: [&str; 7] = [
    "DISABLE_TELEMETRY",
    "DISABLE_ERROR_REPORTING",
    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
    "HTTPS_PROXY",
    "HTTP_PROXY",
    "NO_PROXY",
    "NODE_EXTRA_CA_CERTS",
];

pub fn environment(settings: Option<&[u8]>, executable: &Path) -> Vec<(String, String)> {
    let mut env = vec![
        ("DISABLE_AUTOUPDATER".to_string(), "1".to_string()),
        ("PATH".to_string(), cli::child_path(executable)),
    ];
    let values = settings.and_then(|b| serde_json::from_slice::<Value>(b).ok());
    if let Some(block) = values
        .as_ref()
        .and_then(|v| v.get("env"))
        .and_then(Value::as_object)
    {
        for key in PASSED {
            let text = match block.get(key) {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Bool(b)) => b.to_string(),
                Some(Value::Number(n)) => n.to_string(),
                _ => continue,
            };
            env.push((key.to_string(), text));
        }
    }
    env
}

/// The windows a `get_usage` answer holds; a window that renewed before the
/// reading says nothing about the window now.
pub fn limits(result: &Value, observed: Time) -> Option<Limits> {
    if result.get("rate_limits_available").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let all = result.get("rate_limits")?;
    let mut windows = Vec::new();
    for (key, id, kind, scope) in [
        ("five_hour", "fh", WindowKind::Session, None),
        ("seven_day", "sd", WindowKind::Weekly, None),
        ("seven_day_opus", "so", WindowKind::Weekly, Some("Opus")),
        ("seven_day_sonnet", "sn", WindowKind::Weekly, Some("Sonnet")),
    ] {
        let Some(window) = all.get(key).filter(|w| w.is_object()) else {
            continue;
        };
        let Some(percent) = window
            .get("utilization")
            .and_then(Value::as_f64)
            .filter(|p| p.is_finite())
        else {
            continue;
        };
        let resets = window.get("resets_at").filter(|r| !r.is_null());
        let end = resets.and_then(|r| timestamp(Some(r)));
        if resets.is_some() && end.is_none_or(|e| e <= observed) {
            continue;
        }
        let minutes = if kind == WindowKind::Session {
            300
        } else {
            10_080
        };
        windows.push(LimitWindow::new(
            format!("claude.{id}"),
            kind,
            Some(minutes),
            scope,
            percent,
            end,
        ));
    }
    (!windows.is_empty()).then_some(Limits {
        provider: Provider::Claude,
        windows,
        observed_at: observed,
        source: LimitSource::Account,
    })
}

/// A window disappears at its renewal; one without a renewal after its own
/// length for a session, else after a day. 0 % is never invented.
pub fn current(reading: Option<&Limits>, now: Time) -> Option<Limits> {
    let mut reading = reading?.clone();
    let age = now - reading.observed_at;
    reading.windows.retain(|w| match w.resets_at {
        Some(r) => r > now,
        None => {
            age < if w.kind == WindowKind::Session {
                w.minutes.unwrap_or(300) as f64 * 60.0
            } else {
                86_400.0
            }
        }
    });
    (!reading.windows.is_empty()).then_some(reading)
}

fn error_failure(error: &str) -> Failure {
    let error = error.to_lowercase();
    if error.contains("supported") || error.contains("unknown") {
        Failure::Outdated
    } else if error.contains("auth") || error.contains("login") || error.contains("sign in") {
        Failure::NeedsSignIn
    } else {
        Failure::Unreachable
    }
}

/// Waits for the `control_response` to `id`, skipping every other line.
pub fn answer(next: &mut dyn FnMut() -> Option<Value>, id: &str) -> Result<Value, Failure> {
    while let Some(message) = next() {
        if message.get("type").and_then(Value::as_str) != Some("control_response") {
            continue;
        }
        let Some(response) = message
            .get("response")
            .filter(|r| r.get("request_id").and_then(Value::as_str) == Some(id))
        else {
            continue;
        };
        if response.get("subtype").and_then(Value::as_str) != Some("success") {
            return Err(error_failure(
                response.get("error").and_then(Value::as_str).unwrap_or(""),
            ));
        }
        return Ok(response.get("response").cloned().unwrap_or(json!({})));
    }
    Err(Failure::Unreachable)
}

const NO_SERVERS: &str = "{\"mcpServers\":{}}";
const NO_HOOKS: &str = "{\"disableAllHooks\":true}";

/// The empty MCP config and the hooks switch, as JSON text for an executable.
/// A `.cmd` shim runs through cmd.exe, whose quoting of JSON is fragile, so
/// it gets the same JSON as files instead; both flags take either.
fn config_args(executable: &Path) -> Option<(String, String)> {
    let shim = executable
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"));
    if !shim {
        return Some((NO_SERVERS.into(), NO_HOOKS.into()));
    }
    let dir = super::archive::folder().unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&dir).ok()?;
    let (mcp, hooks) = (
        dir.join("claude-mcp.json"),
        dir.join("claude-settings.json"),
    );
    std::fs::write(&mcp, NO_SERVERS).ok()?;
    std::fs::write(&hooks, NO_HOOKS).ok()?;
    Some((
        mcp.to_string_lossy().into_owned(),
        hooks.to_string_lossy().into_owned(),
    ))
}

/// One account check (§2.2).
pub fn check(now: &dyn Fn() -> Time) -> Result<Limits, Failure> {
    let executable = executable().ok_or(Failure::Missing)?;
    let started = now();
    let organization = read_profile().organization;
    let settings =
        super::files::claude_dir().and_then(|d| std::fs::read(d.join("settings.json")).ok());
    let env = environment(settings.as_deref(), &executable);
    let (mcp, hooks) = config_args(&executable).ok_or(Failure::Unreachable)?;
    let args = [
        "--print",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--no-session-persistence",
        "--tools",
        "",
        "--strict-mcp-config",
        "--mcp-config",
        &mcp,
        "--setting-sources",
        "",
        "--settings",
        &hooks,
    ];
    let mut channel = Conversation::start(&executable, &args, &env, &["CLAUDECODE"], TIMEOUT)
        .ok_or(Failure::Unreachable)?;
    let ask = |channel: &mut Conversation, id: &str, request: Value| -> Result<Value, Failure> {
        if !channel.send(&json!({"type": "control_request", "request_id": id, "request": request}))
        {
            return Err(Failure::Unreachable);
        }
        answer(&mut || channel.next(), id)
    };
    ask(&mut channel, "initialize", json!({"subtype": "initialize"}))?;
    let result = ask(
        &mut channel,
        "usage",
        json!({"subtype": "get_usage", "skip_behaviors": true}),
    )?;
    if result.get("rate_limits_available").and_then(Value::as_bool) != Some(true) {
        return Err(Failure::NeedsSignIn);
    }
    // The saved copy is read once Claude Code has exited, so what it fetched
    // for this answer is on disk.
    channel.end(Duration::from_secs(3));
    let saved = read_profile();
    accept(&result, organization.as_deref(), &saved, started, now())
}

/// The staleness guard: the same account, and an answer fetched for this check.
pub fn accept(
    result: &Value,
    organization: Option<&str>,
    saved: &Profile,
    started: Time,
    now: Time,
) -> Result<Limits, Failure> {
    if saved.organization.as_deref() != organization {
        return Err(Failure::Unreachable);
    }
    if saved.usage_fetched.is_some_and(|f| started - f > REUSE) {
        return Err(Failure::Unreachable);
    }
    limits(result, now.min(saved.usage_fetched.unwrap_or(now))).ok_or(Failure::Unreachable)
}

/// When the next check may run (§2.2 scheduling): 60 s with the page
/// shown, 5 min otherwise; failures back off.
pub fn interval(failure: Option<Failure>, unreachable_streak: u32, page_visible: bool) -> f64 {
    match failure {
        None => {
            if page_visible {
                60.0
            } else {
                300.0
            }
        }
        Some(Failure::Unreachable) => {
            (300.0 * 2f64.powi(unreachable_streak.saturating_sub(1).min(3) as i32)).min(1800.0)
        }
        Some(_) => 1800.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANSWER: &str = r#"{"rate_limits_available":true,"rate_limits":{"five_hour":{"utilization":42.0,"resets_at":"2026-10-24T18:00:00Z"},"seven_day":{"utilization":61.0,"resets_at":"2026-10-28T09:00:00Z"},"seven_day_opus":{"utilization":10.0,"resets_at":null},"seven_day_sonnet":{"utilization":3.0,"resets_at":"2026-10-20T00:00:00Z"}}}"#;

    #[test]
    fn a_get_usage_answer_becomes_account_windows() {
        let observed = 1_792_850_000.0; // 2026-10-24 13:53 UTC
        let reading = limits(&serde_json::from_str(ANSWER).unwrap(), observed).unwrap();
        let ids: Vec<_> = reading
            .windows
            .iter()
            .map(|w| (w.id.as_str(), w.used_percent, w.scope.as_deref()))
            .collect();
        assert_eq!(
            ids,
            [
                ("claude.fh", 42.0, None),
                ("claude.sd", 61.0, None),
                ("claude.so", 10.0, Some("Opus"))
            ],
            "a window renewed before the reading is dropped"
        );
        assert_eq!(reading.windows[0].resets_at, Some(1_792_864_800.0));
        assert_eq!(reading.source, LimitSource::Account);
        assert!(limits(&json!({"rate_limits_available": false}), observed).is_none());
        // Expiry: the idle Opus window lasts a day, the session its own reset.
        let later = current(Some(&reading), observed + 5.0 * 3600.0).unwrap();
        assert_eq!(later.windows.len(), 2, "the session renewed at 18:00");
        assert!(current(Some(&reading), observed + 30.0 * 86_400.0).is_none());
    }

    #[test]
    fn control_responses_are_matched_by_id_and_errors_mapped() {
        let lines = [
            json!({"type": "system", "subtype": "init"}),
            json!({"type": "control_response", "response": {"request_id": "other", "subtype": "success"}}),
            json!({"type": "control_response", "response": {"request_id": "usage", "subtype": "success", "response": serde_json::from_str::<Value>(ANSWER).unwrap()}}),
        ];
        let mut it = lines.into_iter();
        let result = answer(&mut || it.next(), "usage").unwrap();
        assert_eq!(result["rate_limits"]["five_hour"]["utilization"], 42.0);
        let error = |text: &str| {
            let mut once = Some(
                json!({"type": "control_response", "response": {"request_id": "x", "subtype": "error", "error": text}}),
            );
            answer(&mut || once.take(), "x").unwrap_err()
        };
        assert_eq!(
            error("Unsupported control request subtype: get_usage"),
            Failure::Outdated
        );
        assert_eq!(
            error("Not logged in · Please run /login"),
            Failure::NeedsSignIn
        );
        assert_eq!(error("socket hang up"), Failure::Unreachable);
        assert_eq!(answer(&mut || None, "x").unwrap_err(), Failure::Unreachable);
    }

    #[test]
    fn the_staleness_guard_refuses_cached_or_foreign_answers() {
        let result: Value = serde_json::from_str(ANSWER).unwrap();
        let saved = |org: &str, fetched: f64| Profile {
            organization: Some(org.into()),
            usage_fetched: Some(fetched),
            signed_in: true,
            ..Default::default()
        };
        let start = 1_793_300_000.0;
        assert!(
            accept(
                &result,
                Some("o"),
                &saved("o", start + 2.0),
                start,
                start + 5.0
            )
            .is_ok()
        );
        assert_eq!(
            accept(
                &result,
                Some("o"),
                &saved("p", start + 2.0),
                start,
                start + 5.0
            )
            .unwrap_err(),
            Failure::Unreachable
        );
        assert_eq!(
            accept(
                &result,
                Some("o"),
                &saved("o", start - 120.0),
                start,
                start + 5.0
            )
            .unwrap_err(),
            Failure::Unreachable
        );
        assert_eq!(
            accept(
                &result,
                Some("o"),
                &saved("o", start + 2.0),
                start,
                start + 5.0
            )
            .unwrap()
            .observed_at,
            start + 2.0
        );
    }

    #[test]
    fn the_profile_and_settings_give_only_what_is_used() {
        let p = profile(br#"{"oauthAccount":{"organizationUuid":"org","organizationType":"claude_max","organizationRateLimitTier":"default_claude_max_20x","emailAddress":"x"},"cachedUsageUtilization":{"fetchedAtMs":1790000000000}}"#);
        assert_eq!(
            (
                p.organization.as_deref(),
                p.organization_type.as_deref(),
                p.usage_fetched
            ),
            (Some("org"), Some("claude_max"), Some(1_790_000_000.0))
        );
        assert!(!profile(b"{}").signed_in);
        let env = environment(Some(br#"{"env":{"DISABLE_TELEMETRY":1,"HTTPS_PROXY":"http://p","NO_PROXY":true,"SECRET":"x"}}"#), Path::new("C:/bin/claude.exe"));
        assert!(env.contains(&("DISABLE_AUTOUPDATER".into(), "1".into())));
        assert!(env.contains(&("DISABLE_TELEMETRY".into(), "1".into())));
        assert!(env.contains(&("NO_PROXY".into(), "true".into())));
        assert!(!env.iter().any(|(k, _)| k == "SECRET"));
    }

    #[test]
    fn a_cmd_shim_gets_its_json_as_files() {
        assert_eq!(
            config_args(Path::new(r"C:\bin\claude.exe")).unwrap(),
            (NO_SERVERS.into(), NO_HOOKS.into())
        );
        let (mcp, hooks) = config_args(Path::new(r"C:\npm\claude.CMD")).unwrap();
        assert_eq!(std::fs::read_to_string(&mcp).unwrap(), NO_SERVERS);
        assert_eq!(std::fs::read_to_string(&hooks).unwrap(), NO_HOOKS);
    }

    #[test]
    fn checks_back_off_after_failures() {
        assert_eq!(interval(None, 0, true), 60.0);
        assert_eq!(interval(None, 0, false), 300.0);
        assert_eq!(
            [1, 2, 3, 4, 9].map(|n| interval(Some(Failure::Unreachable), n, true)),
            [300.0, 600.0, 1200.0, 1800.0, 1800.0]
        );
        assert_eq!(interval(Some(Failure::NeedsSignIn), 0, true), 1800.0);
    }
}
