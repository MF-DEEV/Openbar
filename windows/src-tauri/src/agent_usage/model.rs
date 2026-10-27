// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! What the agents' logs reduce to (AgentUsageModels.swift). Times are
//! seconds since 1970; the snapshot turns them into milliseconds for the page.

use serde::{Deserialize, Serialize};

pub type Time = f64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    Claude,
    Codex,
    Opencode,
    Copilot,
}

impl Provider {
    pub const ALL: [Provider; 4] = [Provider::Claude, Provider::Codex, Provider::Opencode, Provider::Copilot];

    pub fn key(self) -> &'static str {
        match self {
            Provider::Claude => "claude",
            Provider::Codex => "codex",
            Provider::Opencode => "opencode",
            Provider::Copilot => "copilot",
        }
    }

    /// The pref that switches the provider's reading (notchAgents<Name>).
    pub fn pref(self) -> &'static str {
        match self {
            Provider::Claude => "/island/agentsClaude",
            Provider::Codex => "/island/agentsCodex",
            Provider::Opencode => "/island/agentsOpenCode",
            Provider::Copilot => "/island/agentsCopilot",
        }
    }
}

/// Token counts both logs reduce to. `input` excludes cache traffic, and
/// `output` already holds the reasoning tokens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tokens {
    pub input: i64,
    pub cache_write: i64,
    pub cache_read: i64,
    pub output: i64,
    pub reasoning: i64,
}

impl Tokens {
    pub fn total(&self) -> i64 {
        self.input + self.cache_write + self.cache_read + self.output
    }
    /// Everything the model read, cached or not.
    pub fn prompt(&self) -> i64 {
        self.input + self.cache_write + self.cache_read
    }
    pub fn add(&mut self, other: &Tokens) {
        self.input += other.input;
        self.cache_write += other.cache_write;
        self.cache_read += other.cache_read;
        self.output += other.output;
        self.reasoning += other.reasoning;
    }
    /// Streamed replies are logged once per content block; the largest reading is final.
    pub fn merged(&self, other: &Tokens) -> Tokens {
        Tokens {
            input: self.input.max(other.input),
            cache_write: self.cache_write.max(other.cache_write),
            cache_read: self.cache_read.max(other.cache_read),
            output: self.output.max(other.output),
            reasoning: self.reasoning.max(other.reasoning),
        }
    }
    /// What `self` grew by since `before`, never negative.
    pub fn growth(&self, before: &Tokens) -> Tokens {
        Tokens {
            input: (self.input - before.input).max(0),
            cache_write: (self.cache_write - before.cache_write).max(0),
            cache_read: (self.cache_read - before.cache_read).max(0),
            output: (self.output - before.output).max(0),
            reasoning: (self.reasoning - before.reasoning).max(0),
        }
    }
    pub fn scaled(&self, by: f64) -> Tokens {
        let s = |v: i64| (v as f64 * by) as i64;
        Tokens { input: s(self.input), cache_write: s(self.cache_write), cache_read: s(self.cache_read), output: s(self.output), reasoning: s(self.reasoning) }
    }
}

/// What one response bills, beyond its model (AgentBillable).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Billable {
    pub tokens: Tokens,
    /// Session totals hide prompt sizes: priced at base rates.
    pub aggregate: bool,
    /// The part of `tokens.cache_write` kept for an hour.
    pub long_cache_write: i64,
    pub fast: bool,
    /// Inference pinned to the United States.
    pub domestic: bool,
    pub web_searches: i64,
}

/// One billed model response.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub provider: Provider,
    pub date: Time,
    pub model: String,
    pub project: String,
    pub session: String,
    pub requests: i64,
    pub tokens: Tokens,
    /// API list-price value in USD; None for a model without a known price.
    pub cost: Option<f64>,
    pub savings: f64,
    /// Cost the provider reported itself (OpenCode), never repriced.
    pub reported_cost: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowKind {
    Session,
    Weekly,
    Other,
}

/// An allowance and how much of it is spent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LimitWindow {
    pub id: String,
    pub kind: WindowKind,
    pub minutes: Option<i64>,
    pub scope: Option<String>,
    pub used_percent: f64,
    pub resets_at: Option<Time>,
    /// A presentation estimate, never a deadline.
    pub estimated_resets_at: Option<Time>,
}

impl LimitWindow {
    pub fn new(id: impl Into<String>, kind: WindowKind, minutes: Option<i64>, scope: Option<&str>, used: f64, resets_at: Option<Time>) -> LimitWindow {
        LimitWindow { id: id.into(), kind, minutes, scope: scope.map(str::to_string), used_percent: used.clamp(0.0, 100.0), resets_at, estimated_resets_at: None }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LimitSource {
    /// Saved by the Claude desktop app.
    ClaudeApp,
    /// Copied by the agent into its session log with each response.
    SessionLog,
    /// Asked of the agent's CLI, which checks the account itself.
    Account,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Limits {
    pub provider: Provider,
    pub windows: Vec<LimitWindow>,
    pub observed_at: Time,
    pub source: LimitSource,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub name: String,
    pub monthly_price: Option<f64>,
}

/// A turn an agent works on right now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LiveSession {
    pub id: String,
    pub provider: Provider,
    pub started: Time,
    pub last_activity: Time,
    pub model: String,
    pub project: String,
    pub tokens: Tokens,
    pub cost: f64,
}

/// Something worth a moment in the closed island (§9).
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Finished { provider: Provider, duration: f64, cost: f64, tokens: i64, project: String },
    LimitWarning { provider: Provider, window: LimitWindow },
    LimitReset { provider: Provider, window: LimitWindow },
    BudgetReached { spent: f64, budget: f64 },
}

pub fn now() -> Time {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}
