// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
//! The user's notification history, independent of Windows' current toast list.
use crate::notifications::Item;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, path::PathBuf};

#[derive(Default, Serialize, Deserialize)]
pub struct Document {
    pub items: Vec<Item>,
    #[serde(default)]
    pub seen: HashSet<u32>,
}

pub struct History {
    path: PathBuf,
    blocked: bool,
}

impl History {
    pub fn open(path: PathBuf) -> (Self, Result<Document, String>) {
        let read = (|| -> Result<Document, String> {
            match fs::metadata(&path) {
                Ok(meta) if meta.len() > 16 * 1024 * 1024 => {
                    return Err("Notification history is too large to read safely.".into());
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(Document::default());
                }
                Err(e) => return Err(format!("Cannot read notification history: {e}")),
                _ => {}
            }
            let bytes = fs::read(&path).map_err(|e| e.to_string())?;
            let mut document: Document = serde_json::from_slice(&bytes).map_err(|_| {
                "Cannot read saved notification history. The file has been preserved.".to_string()
            })?;
            document
                .items
                .retain(|item| item.time.is_finite() && item.time >= 0.0);
            document
                .items
                .sort_by(|a, b| b.time.total_cmp(&a.time).then_with(|| b.id.cmp(&a.id)));
            document.items.truncate(super::notifications::INBOX_LIMIT);
            document
                .seen
                .extend(document.items.iter().map(|item| item.id));
            Ok(document)
        })();
        (
            Self {
                path,
                blocked: read.is_err(),
            },
            read,
        )
    }

    pub fn save(&self, items: &[Item], seen: &HashSet<u32>) -> Result<(), String> {
        if self.blocked {
            return Err("The unreadable notification history has been preserved; new entries cannot be saved.".into());
        }
        let result = (|| -> std::io::Result<()> {
            fs::create_dir_all(self.path.parent().unwrap())?;
            #[derive(Serialize)]
            struct Saved<'a> {
                items: &'a [Item],
                seen: &'a HashSet<u32>,
            }
            let tmp = self.path.with_extension("json.tmp");
            use std::io::Write;
            let mut file = fs::File::create(&tmp)?;
            file.write_all(&serde_json::to_vec(&Saved { items, seen })?)?;
            file.sync_all()?;
            drop(file);
            fs::rename(tmp, &self.path)
        })();
        result.map_err(|e| format!("Cannot save notification history: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_and_dismissed_ids_survive_a_fresh_store() {
        let path = std::env::temp_dir()
            .join(format!("vors-history-{}-roundtrip", std::process::id()))
            .join("history.json");
        let (store, _) = History::open(path.clone());
        let item = Item {
            id: 42,
            app: "Messages".into(),
            aumid: "app".into(),
            time: 100.0,
            title: "Ada".into(),
            body: "Meeting at 10".into(),
            icon: None,
        };
        store.save(&[item], &HashSet::from([42, 43])).unwrap();
        drop(store);
        let (_, restored) = History::open(path.clone());
        let restored = restored.unwrap();
        assert_eq!(restored.items[0].body, "Meeting at 10");
        assert!(
            restored.seen.contains(&43),
            "A dismissed toast must not return on restart"
        );
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(path.parent().unwrap());
    }
    #[test]
    fn unreadable_history_is_never_overwritten() {
        let path = std::env::temp_dir()
            .join(format!("vors-history-{}-corrupt", std::process::id()))
            .join("history.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"broken").unwrap();
        let (store, loaded) = History::open(path.clone());
        assert!(loaded.is_err());
        assert!(store.save(&[], &HashSet::new()).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"broken");
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(path.parent().unwrap());
    }
}
