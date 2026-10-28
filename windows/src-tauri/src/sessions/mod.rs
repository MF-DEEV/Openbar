// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The pure half of the Sessions section: what Claude Code's registry,
//! transcripts and hooks say, and the rules that join them into the state
//! the island shows. Nothing here touches files, clocks or threads; times are
//! passed in as epoch seconds, so every rule can be tested. The service that
//! reads the files and serves the hooks lives beside it.

// The service and the UI that call these land separately.
#![allow(dead_code)]

pub mod attachments;
pub mod codex;
pub mod engine;
pub mod hook_message;
pub mod models;
pub mod registry;
pub mod service;
pub mod transcript;
