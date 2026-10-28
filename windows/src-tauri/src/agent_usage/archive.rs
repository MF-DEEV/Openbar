// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! What is kept between launches so a relaunch reads only newly appended
//! bytes (AgentUsageArchive.swift; spec 03 §7). Counters and cursors only,
//! never text, and nothing from OpenCode. Unlike the Mac, the archive is
//! versioned by the parser, not the build, so an app update that leaves the
//! parsers alone does not re-read 91 days of logs (§13.12).

use super::files::SavedCursor;
use super::model::Provider;
use super::store::Saved;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Bump whenever a parser, the store or the cursor format changes meaning.
pub const PARSER_VERSION: u32 = 1;
const NAME: &str = "agent-usage.json";

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Archive {
    pub version: u32,
    pub providers: Vec<Provider>,
    pub store: Saved,
    pub cursors: Vec<SavedCursor>,
}

/// `%LOCALAPPDATA%\com.vorssaint.island\AgentUsage\`, a cache safe to delete.
pub fn folder() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|l| {
        PathBuf::from(l)
            .join("com.vorssaint.island")
            .join("AgentUsage")
    })
}

/// The archive, only when saved by this parser for this provider set.
pub fn load(dir: &Path, providers: &[Provider]) -> Option<Archive> {
    let bytes = std::fs::read(dir.join(NAME)).ok()?;
    let archive: Archive = serde_json::from_slice(&bytes).ok()?;
    (archive.version == PARSER_VERSION && archive.providers == providers).then_some(archive)
}

/// Written beside, then moved over, so a crash never leaves half a file.
pub fn save(dir: &Path, archive: &Archive) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let temporary = dir.join(format!("{NAME}.tmp"));
    std::fs::write(
        &temporary,
        serde_json::to_vec(archive).map_err(std::io::Error::other)?,
    )?;
    std::fs::rename(&temporary, dir.join(NAME))
}

/// Switching the section off forgets what it read.
pub fn remove(dir: &Path) {
    let _ = std::fs::remove_file(dir.join(NAME));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_usage::model::{Billable, Record, Tokens};
    use crate::agent_usage::store::SavedRecord;

    #[test]
    fn an_archive_resumes_only_for_its_parser_and_provider_set() {
        let dir = std::env::temp_dir().join(format!("vorssaint-archive-{}", std::process::id()));
        let record = Record {
            provider: Provider::Codex,
            date: 1.0,
            model: "gpt-5.6".into(),
            project: "p".into(),
            session: "s".into(),
            requests: 1,
            tokens: Tokens {
                output: 5,
                ..Default::default()
            },
            cost: Some(0.1),
            savings: 0.0,
            reported_cost: false,
        };
        let archive = Archive {
            version: PARSER_VERSION,
            providers: vec![Provider::Claude, Provider::Codex],
            store: Saved {
                records: vec![SavedRecord {
                    key: "codex:r".into(),
                    record,
                    billable: Billable::default(),
                    sources: vec!["a".into()],
                }],
                ..Default::default()
            },
            cursors: vec![],
        };
        save(&dir, &archive).unwrap();
        assert_eq!(
            load(&dir, &[Provider::Claude, Provider::Codex]),
            Some(archive)
        );
        assert!(
            load(&dir, &[Provider::Claude]).is_none(),
            "another provider set reads again"
        );
        let mut text: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join(NAME)).unwrap()).unwrap();
        text["version"] = serde_json::Value::from(PARSER_VERSION + 1);
        std::fs::write(dir.join(NAME), text.to_string()).unwrap();
        assert!(
            load(&dir, &[Provider::Claude, Provider::Codex]).is_none(),
            "another parser version reads again"
        );
        remove(&dir);
        assert!(!dir.join(NAME).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
