// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Codex sessions (spec 07-codex-sessions): the island joins Codex's own
//! per-user app-server as one more client, so it sees every live thread's
//! status, answers approvals first-wins alongside the terminal, and queues
//! replies into the thread, without typing into any terminal.

pub mod client;
pub mod state;
