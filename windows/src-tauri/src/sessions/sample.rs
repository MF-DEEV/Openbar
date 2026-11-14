// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The made-up session onboarding's Try it shows in the island (#168), as
//! the Mac's ClaudeSessionSample: a Claude Code session asking to run the
//! tests. No agent stands behind it, so it has no process, no terminal and no
//! transcript, and answering it goes nowhere.

use super::models::{HostKind, Permission, Provider, Session, SessionPhase, Time, Tokens, ToolUse};

pub const ID: &str = "openbar-sample-session";
pub const PROJECT: &str = "sample-app";
pub const COMMAND: &str = "npm test";

pub fn waiting(round: u64, home: &str, t: Time) -> Session {
    let mut permission = Permission::new(
        &format!("openbar-sample-{round}"),
        "Bash",
        Some(COMMAND),
        true,
        t,
    );
    permission.detail = COMMAND.to_owned();
    Session {
        id: ID.to_owned(),
        provider: Provider::Claude,
        pid: 0,
        cwd: format!("{home}\\Projects\\{PROJECT}"),
        project: PROJECT.to_owned(),
        name: PROJECT.to_owned(),
        name_source: None,
        branch: Some("main".to_owned()),
        model: None,
        model_name: None,
        effort: None,
        edits: None,
        failed_at: None,
        host: HostKind::Unknown,
        accepts_replies: false,
        host_name: None,
        remote: None,
        phase: SessionPhase::WaitingPermission,
        phase_since: t,
        turn_started: Some(t - 12.0),
        last_active: Some(t),
        looked_at: None,
        waiting_for: None,
        current_tool: None,
        permission: Some(permission),
        waiting_diffs: Vec::new(),
        timeline: Vec::new(),
        subagents: Vec::new(),
        tasks: None,
        tokens: Tokens::default(),
        cost: 0.0,
        title: None,
        last_text: None,
        hooked: true,
        chat: Vec::new(),
        background_label: None,
        services: 0,
        background: None,
        limit_stop: None,
        account: None,
        account_label: None,
        account_id: None,
    }
}

/// Right after the answer: an allowed command runs, a denied one leaves the
/// turn over.
pub fn answered(mut session: Session, allowed: bool, t: Time) -> Session {
    session.permission = None;
    session.phase_since = t;
    session.last_active = Some(t);
    if allowed {
        session.phase = SessionPhase::RunningTool;
        session.current_tool = Some(ToolUse {
            id: "openbar-sample-tool".to_owned(),
            name: "Bash".to_owned(),
            summary: Some(COMMAND.to_owned()),
            started: t,
            finished: None,
            failed: false,
            input_key: None,
        });
    } else {
        session.phase = SessionPhase::Finished;
    }
    session
}

pub fn finished(mut session: Session, t: Time) -> Session {
    if let Some(mut tool) = session.current_tool.take() {
        tool.finished = Some(t);
        session.timeline.push(tool);
    }
    session.phase = SessionPhase::Finished;
    session.phase_since = t;
    session.last_active = Some(t);
    session
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plays_out_an_answer() {
        let waiting = waiting(2, "C:\\Users\\someone", 100.0);
        assert_eq!(waiting.id, ID);
        assert_eq!(waiting.phase, SessionPhase::WaitingPermission);
        let permission = waiting.permission.as_ref().unwrap();
        assert_eq!(permission.request_id, "openbar-sample-2");
        assert_eq!(permission.detail, COMMAND);
        assert!(waiting.hooked && waiting.pid == 0);
        assert_eq!(waiting.cwd, "C:\\Users\\someone\\Projects\\sample-app");

        let allowed = answered(waiting.clone(), true, 102.0);
        assert!(allowed.permission.is_none());
        assert_eq!(allowed.phase, SessionPhase::RunningTool);
        let denied = answered(waiting, false, 102.0);
        assert_eq!(denied.phase, SessionPhase::Finished);
        assert!(denied.current_tool.is_none());

        let done = finished(allowed, 104.0);
        assert_eq!(done.phase, SessionPhase::Finished);
        assert_eq!(done.timeline.len(), 1);
        assert_eq!(done.timeline[0].finished, Some(104.0));
    }
}
