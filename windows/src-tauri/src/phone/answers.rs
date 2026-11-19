// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Requests answered from the phone (#214), listed on their session's page
//! as the island's own answers are not: so the person can tell afterwards
//! what came from Telegram.

use std::collections::{BTreeMap, HashSet};
use std::sync::Mutex;

use serde::Serialize;

use crate::sessions::hook_message::Decision;
use crate::sessions::models::{Permission, Session, Time};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    Allowed,
    Always,
    Denied,
    PlanApproved,
    KeptPlanning,
    Answered,
    Skipped,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneAnswer {
    pub id: String,
    pub tool: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub at: Time,
    pub verdict: Verdict,
}

impl PhoneAnswer {
    pub fn new(permission: Option<&Permission>, request: &str, decision: &Decision, at: Time) -> Self {
        let plan = permission.is_some_and(|p| p.tool == super::messages::PLAN_TOOL);
        let question = permission.is_some_and(|p| !p.questions.is_empty());
        let verdict = match decision {
            Decision::Allow if plan => Verdict::PlanApproved,
            Decision::Allow => Verdict::Allowed,
            Decision::AllowAlways => Verdict::Always,
            Decision::ApprovePlan(_) => Verdict::PlanApproved,
            Decision::Deny if question => Verdict::Skipped,
            Decision::Deny if plan => Verdict::KeptPlanning,
            Decision::Deny => Verdict::Denied,
            Decision::Answer(_) => Verdict::Answered,
        };
        PhoneAnswer {
            id: request.to_owned(),
            tool: permission.map_or_else(|| "Request".to_owned(), |p| p.tool.clone()),
            summary: permission.and_then(|p| p.summary.clone()),
            at,
            verdict,
        }
    }
}

/// What was answered from the phone, by session, newest last.
pub struct Log {
    entries: Mutex<BTreeMap<String, Vec<PhoneAnswer>>>,
}

impl Log {
    pub const KEPT: usize = 30;

    pub const fn new() -> Self {
        Self { entries: Mutex::new(BTreeMap::new()) }
    }

    pub fn add(&self, entry: PhoneAnswer, session: &str) {
        let mut entries = self.entries.lock().unwrap();
        let list = entries.entry(session.to_owned()).or_default();
        list.push(entry);
        if list.len() > Self::KEPT {
            let extra = list.len() - Self::KEPT;
            list.drain(..extra);
        }
    }

    pub fn decorate(&self, sessions: &mut [Session]) {
        let entries = self.entries.lock().unwrap();
        for session in sessions {
            if let Some(list) = entries.get(&session.id) {
                session.phone_answers = list.clone();
            }
        }
    }

    /// Forgets sessions no longer listed, once a while passed since their last entry.
    pub fn prune(&self, listed: &HashSet<String>, now: Time) {
        self.entries
            .lock()
            .unwrap()
            .retain(|id, list| listed.contains(id) || list.last().is_some_and(|e| now - e.at < 600.0));
    }
}

pub static LOG: Log = Log::new();

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::models::Question;
    use crate::sessions::plan_modes::PlanMode;

    #[test]
    fn each_answer_is_named_as_the_phone_gave_it() {
        let bash = Permission::new("b", "Bash", Some("ls"), true, 0.0);
        let plan = Permission::new("p", "ExitPlanMode", None, false, 0.0);
        let mut question = Permission::new("q", "AskUserQuestion", None, false, 0.0);
        question.questions = vec![Question { question: "Q?".into(), header: None, options: vec![], multi_select: false }];
        let cases = [
            (Some(&bash), Decision::Allow, Verdict::Allowed),
            (Some(&bash), Decision::AllowAlways, Verdict::Always),
            (Some(&bash), Decision::Deny, Verdict::Denied),
            (Some(&plan), Decision::Allow, Verdict::PlanApproved),
            (Some(&plan), Decision::ApprovePlan(PlanMode::Auto), Verdict::PlanApproved),
            (Some(&plan), Decision::Deny, Verdict::KeptPlanning),
            (Some(&question), Decision::Deny, Verdict::Skipped),
            (Some(&question), Decision::Answer(BTreeMap::new()), Verdict::Answered),
            (None, Decision::Deny, Verdict::Denied),
        ];
        for (p, decision, verdict) in cases {
            assert_eq!(PhoneAnswer::new(p, "x", &decision, 1.0).verdict, verdict);
        }
        let entry = PhoneAnswer::new(Some(&bash), "b", &Decision::Allow, 1.0);
        assert_eq!(entry.summary.as_deref(), Some("ls"));
        assert_eq!(serde_json::to_value(&entry).unwrap()["verdict"], "allowed");
    }

    #[test]
    fn answers_go_to_their_session_and_are_forgotten_with_it() {
        let log = Log::new();
        log.add(PhoneAnswer::new(None, "r", &Decision::Allow, 1.0), "a");
        let mut sessions = vec![crate::sessions::sample::waiting(1, "/w", 0.0)];
        sessions[0].id = "a".into();
        log.decorate(&mut sessions);
        assert_eq!(sessions[0].phone_answers.len(), 1);
        log.prune(&HashSet::new(), 700.0);
        let mut again = vec![crate::sessions::sample::waiting(1, "/w", 0.0)];
        again[0].id = "a".into();
        log.decorate(&mut again);
        assert!(again[0].phone_answers.is_empty());
    }
}
