// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The clipboard history itself: its entries, how a copy joins them, and the
//! limits that trim them (ClipboardHistorySupport.swift and
//! ClipboardHistoryService.swift; spec 05 §3.3). Nothing here touches the
//! system clipboard or the disk, so all of it is tested on any platform.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A copied document stays, while a pathological payload still has a bound
/// (`ClipboardHistoryEditing.maxCharacters`, CHSup:294).
pub const MAX_CHARACTERS: usize = 1_000_000;
/// All text the history keeps, in UTF-8 bytes (CHSup:295).
pub const MAX_STORED_TEXT_BYTES: usize = 64 * 1024 * 1024;
/// A row only needs enough text for the lines it shows (CHSup:300).
pub const PREVIEW_CHARACTERS: usize = 2_000;
/// What the page's search reads of each text entry. The page gets every
/// entry on each change, so a history of long documents is cut here rather
/// than sent whole; the full text is still what Copy and Paste write.
pub const SEARCH_CHARACTERS: usize = 32_000;
/// Copied files the history takes at once (CHS:818).
pub const MAX_FILES: usize = 100;
/// Encoded PNG and raw bitmap bounds (CHS:819-820).
pub const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_RAW_IMAGE_BYTES: usize = 64 * 1024 * 1024;

/// `Defaults.allowedClipboardHistoryLimits`; 0 is unlimited.
pub const LIMITS: [i64; 8] = [20, 50, 100, 250, 500, 1_000, 10_000, 0];
/// `Defaults.allowedClipboardHistoryRetentionDays`; 0 keeps entries.
pub const RETENTION_DAYS: [i64; 6] = [1, 7, 30, 90, 365, 0];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    #[default]
    Text,
    Image,
    Files,
}

/// One saved copy (CHSup:135-178). Images and files keep an empty `text`;
/// their labels are derived so they never go stale in storage. Every field
/// but `text` has a default, so an older or hand-edited file still loads.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    #[serde(default = "new_id")]
    pub id: String,
    pub text: String,
    /// Milliseconds since the epoch.
    #[serde(default)]
    pub copied_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_at: Option<i64>,
    #[serde(default)]
    pub kind: Kind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_paths: Vec<String>,
    /// A PNG's name in the history's `images` folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_file: Option<String>,
    /// SHA-256 of the PNG, so copying the same picture again refreshes its entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_height: Option<u32>,
    /// The app the copy came from, when Windows could tell (best effort).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_exe: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_name: Option<String>,
}

pub fn new_id() -> String {
    let mut bytes = [0u8; 16];
    let _ = getrandom::fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl Entry {
    pub fn is_pinned(&self) -> bool {
        self.pinned_at.is_some()
    }

    /// The row's one-paragraph text: at most 2,000 characters with line
    /// breaks and tabs folded to spaces, and an ellipsis when cut (CHSup:192-207).
    pub fn preview(&self) -> String {
        match self.kind {
            Kind::Text => {
                let (prefix, cut) = prefix(&self.text, PREVIEW_CHARACTERS);
                let collapsed = prefix.replace("\r\n", " ").replace(['\n', '\r', '\t'], " ");
                let collapsed = collapsed.trim();
                let visible = if collapsed.is_empty() { prefix } else { collapsed };
                if cut { format!("{visible}…") } else { visible.to_string() }
            }
            Kind::Image => self.dimensions(),
            Kind::Files => self.file_names().join(", "),
        }
    }

    /// The preview with its line breaks kept, for a card tall enough to show
    /// a snippet's shape. Tabs still become spaces (CHSup:211-219).
    pub fn card_preview(&self) -> String {
        if self.kind != Kind::Text {
            return self.preview();
        }
        let (prefix, cut) = prefix(&self.text, PREVIEW_CHARACTERS);
        let spaced = prefix.replace("\r\n", "\n").replace('\r', "\n").replace('\t', " ");
        let spaced = spaced.trim();
        let visible = if spaced.is_empty() { prefix } else { spaced };
        if cut { format!("{visible}…") } else { visible.to_string() }
    }

    pub fn dimensions(&self) -> String {
        match (self.image_width, self.image_height) {
            (Some(w), Some(h)) => format!("{w}×{h}"),
            _ => String::new(),
        }
    }

    pub fn file_names(&self) -> Vec<String> {
        self.file_paths
            .iter()
            .map(|p| p.rsplit(['\\', '/']).next().unwrap_or(p).to_string())
            .collect()
    }

    /// Same content, whenever it was copied (`matchesContent`, CHSup:247-254).
    pub fn matches(&self, content: &Content) -> bool {
        match (self.kind, content) {
            (Kind::Text, Content::Text(text)) => &self.text == text,
            (Kind::Image, Content::Image { hash, .. }) => self.image_hash.as_deref() == Some(hash),
            (Kind::Files, Content::Files(paths)) => &self.file_paths == paths,
            _ => false,
        }
    }
}

/// The first `n` characters, and whether anything was left out.
fn prefix(text: &str, n: usize) -> (&str, bool) {
    match text.char_indices().nth(n) {
        Some((end, _)) => (&text[..end], true),
        None => (text, false),
    }
}

/// What a clipboard read hands to the history.
#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    Text(String),
    Image { png: Vec<u8>, hash: String, width: u32, height: u32 },
    Files(Vec<String>),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Source {
    pub exe: Option<String>,
    pub name: Option<String>,
}

/// The text a copy is saved as: trimmed, never empty, and within the bound (CHS:924-926).
pub fn storable_text(raw: &str) -> Option<String> {
    let text = raw.trim();
    (!text.is_empty() && text.chars().count() <= MAX_CHARACTERS).then(|| text.to_string())
}

pub fn sanitized_limit(value: i64) -> i64 {
    if LIMITS.contains(&value) { value } else { 50 }
}

pub fn sanitized_retention(value: i64) -> i64 {
    if RETENTION_DAYS.contains(&value) { value } else { 0 }
}

/// Pinned entries first, then recent ones, each group in its own order.
pub fn normalized(entries: Vec<Entry>) -> Vec<Entry> {
    let (pinned, recent): (Vec<_>, Vec<_>) = entries.into_iter().partition(Entry::is_pinned);
    pinned.into_iter().chain(recent).collect()
}

/// Pinned entries never expire and sit outside the limit; recent ones keep
/// the newest `limit` (0 = all) copied within `retention_days` (0 = forever),
/// and all text stays within `text_bytes` (`retainedEntries`, CHSup:335-364).
pub fn retained(entries: &[Entry], limit: i64, retention_days: i64, now: i64, text_bytes: usize) -> Vec<Entry> {
    let mut remaining = text_bytes;
    let mut keep = |candidates: Vec<&Entry>, limit: Option<usize>| {
        let mut out = Vec::new();
        for entry in candidates {
            if limit.is_some_and(|l| out.len() >= l) {
                break;
            }
            let bytes = if entry.kind == Kind::Text { entry.text.len() } else { 0 };
            if bytes > remaining {
                continue;
            }
            remaining -= bytes;
            out.push(entry.clone());
        }
        out
    };
    let pinned = keep(entries.iter().filter(|e| e.is_pinned()).collect(), None);
    let cutoff = (retention_days > 0).then(|| now - retention_days * 86_400_000);
    let recent = keep(
        entries
            .iter()
            .filter(|e| !e.is_pinned() && cutoff.is_none_or(|c| e.copied_at >= c))
            .collect(),
        (limit > 0).then_some(limit as usize),
    );
    pinned.into_iter().chain(recent).collect()
}

#[derive(Default)]
pub struct History {
    pub entries: Vec<Entry>,
}

impl History {
    fn first_recent(&self) -> usize {
        self.entries.iter().position(|e| !e.is_pinned()).unwrap_or(self.entries.len())
    }

    /// A new copy, or an old one copied again, lands on top of the recent
    /// entries; a copy of a pinned entry keeps its pin and place among the
    /// pins. The copy takes the app it came from this time (CHS:851-985).
    /// `store_image` saves a new picture and names its file. Returns the id.
    pub fn promote(
        &mut self,
        content: Content,
        source: Source,
        now: i64,
        store_image: impl FnOnce(&[u8]) -> Option<String>,
    ) -> Option<String> {
        let existing = self.entries.iter().position(|e| e.matches(&content)).map(|i| self.entries.remove(i));
        let mut entry = match (existing, content) {
            (Some(old), _) => Entry { copied_at: now, ..old },
            (None, Content::Text(text)) => blank(text, Kind::Text, now),
            (None, Content::Image { png, hash, width, height }) => Entry {
                image_file: Some(store_image(&png)?),
                image_hash: Some(hash),
                image_width: Some(width),
                image_height: Some(height),
                ..blank(String::new(), Kind::Image, now)
            },
            (None, Content::Files(paths)) => Entry { file_paths: paths, ..blank(String::new(), Kind::Files, now) },
        };
        entry.source_exe = source.exe;
        entry.source_name = source.name;
        let id = entry.id.clone();
        let at = if entry.is_pinned() { 0 } else { self.first_recent() };
        self.entries.insert(at, entry);
        self.entries = normalized(std::mem::take(&mut self.entries));
        Some(id)
    }

    /// Copying an entry back counts as using it: a recent one moves to the
    /// top of the recent list (`touch`, CHS:278-307).
    pub fn touch(&mut self, id: &str, now: i64) -> bool {
        let Some(index) = self.entries.iter().position(|e| e.id == id) else { return false };
        let mut entry = self.entries.remove(index);
        entry.copied_at = now;
        let at = if entry.is_pinned() { index } else { self.first_recent() };
        self.entries.insert(at, entry);
        true
    }

    /// A pin goes to the top of the pins; an unpinned entry to the top of the
    /// recent ones, dated now so retention doesn't drop it on the spot (CHS:309-346).
    pub fn toggle_pin(&mut self, id: &str, now: i64) -> bool {
        let Some(index) = self.entries.iter().position(|e| e.id == id) else { return false };
        let mut entry = self.entries.remove(index);
        if entry.is_pinned() {
            entry.pinned_at = None;
            entry.copied_at = now;
            let at = self.first_recent();
            self.entries.insert(at, entry);
        } else {
            entry.pinned_at = Some(now);
            self.entries.insert(0, entry);
        }
        true
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() != before
    }

    /// The neighbour an entry swaps with, within its own group (CHS:992-1007).
    fn move_destination(&self, id: &str, up: bool) -> Option<(usize, usize)> {
        let from = self.entries.iter().position(|e| e.id == id)?;
        let pinned = self.entries[from].is_pinned();
        let group: Vec<usize> = (0..self.entries.len()).filter(|&i| self.entries[i].is_pinned() == pinned).collect();
        let at = group.iter().position(|&i| i == from)?;
        let to = if up { group.get(at.checked_sub(1)?)? } else { group.get(at + 1)? };
        Some((from, *to))
    }

    pub fn move_entry(&mut self, id: &str, up: bool) -> bool {
        let Some((from, to)) = self.move_destination(id, up) else { return false };
        self.entries.swap(from, to);
        true
    }

    /// Deletes only the unpinned entries the confirmation counted, as they
    /// were then: a copy that lands while it was open survives (CHS:382-386).
    pub fn clear_recent(&mut self, confirmed: &HashMap<String, i64>) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.is_pinned() || confirmed.get(&e.id) != Some(&e.copied_at));
        self.entries.len() != before
    }

    /// Applies the limit and retention; true when anything was dropped.
    pub fn trim(&mut self, limit: i64, retention_days: i64, now: i64) -> bool {
        let next = retained(&self.entries, limit, retention_days, now, MAX_STORED_TEXT_BYTES);
        let changed = next.len() != self.entries.len();
        self.entries = next;
        changed
    }

    /// Image files the entries still use.
    pub fn image_files(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().filter_map(|e| e.image_file.as_deref())
    }
}

fn blank(text: String, kind: Kind, now: i64) -> Entry {
    Entry {
        id: new_id(),
        text,
        copied_at: now,
        pinned_at: None,
        kind,
        file_paths: Vec::new(),
        image_file: None,
        image_hash: None,
        image_width: None,
        image_height: None,
        source_exe: None,
        source_name: None,
    }
}

/// The saved file's entries. A stored picture's name must be a bare file
/// name, so a hand-edited file can't point the history elsewhere.
pub fn decode(bytes: &[u8]) -> Option<Vec<Entry>> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut entries: Vec<Entry> = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    for entry in &mut entries {
        if entry.image_file.as_deref().is_some_and(|n| n.is_empty() || n.contains(['\\', '/', ':']) || n == "..") {
            entry.image_file = None;
        }
    }
    entries.retain(|e| match e.kind {
        Kind::Text => !e.text.is_empty(),
        Kind::Image => e.image_file.is_some(),
        Kind::Files => !e.file_paths.is_empty(),
    });
    Some(normalized(entries))
}

pub fn encode(entries: &[Entry]) -> Vec<u8> {
    serde_json::to_vec(entries).unwrap_or_else(|_| b"[]".to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(t: &str) -> Content {
        Content::Text(t.into())
    }

    fn history(items: &[&str]) -> History {
        let mut h = History::default();
        for (i, t) in items.iter().enumerate() {
            h.promote(text(t), Source::default(), i as i64, |_| None);
        }
        h
    }

    fn texts(h: &History) -> Vec<&str> {
        h.entries.iter().map(|e| e.text.as_str()).collect()
    }

    #[test]
    fn preview_folds_breaks_and_cuts_at_two_thousand_characters() {
        let mut e = blank("  first\r\nsecond\tthird\n".into(), Kind::Text, 0);
        assert_eq!(e.preview(), "first second third");
        assert_eq!(e.card_preview(), "first\nsecond third");
        e.text = "é".repeat(PREVIEW_CHARACTERS + 5);
        let preview = e.preview();
        assert!(preview.ends_with('…'));
        assert_eq!(preview.chars().count(), PREVIEW_CHARACTERS + 1);
        e.text = "é".repeat(PREVIEW_CHARACTERS);
        assert!(!e.preview().ends_with('…'));
        // Only blanks keeps the blanks rather than showing nothing.
        e.text = "\n\n".into();
        assert_eq!(e.preview(), "\n\n");
    }

    #[test]
    fn image_and_file_labels_derive_from_their_fields() {
        let mut e = blank(String::new(), Kind::Image, 0);
        e.image_width = Some(640);
        e.image_height = Some(480);
        assert_eq!(e.preview(), "640×480");
        let mut f = blank(String::new(), Kind::Files, 0);
        f.file_paths = vec![r"C:\a\one.txt".into(), "D:/b/two.png".into()];
        assert_eq!(f.preview(), "one.txt, two.png");
    }

    #[test]
    fn storable_text_trims_and_bounds() {
        assert_eq!(storable_text("  hi \n").as_deref(), Some("hi"));
        assert_eq!(storable_text(" \t\n"), None);
        assert!(storable_text(&"a".repeat(MAX_CHARACTERS)).is_some());
        assert!(storable_text(&"a".repeat(MAX_CHARACTERS + 1)).is_none());
    }

    #[test]
    fn copying_again_refreshes_the_entry_instead_of_duplicating_it() {
        let mut h = history(&["a", "b", "c"]);
        let id = h.entries.iter().find(|e| e.text == "a").unwrap().id.clone();
        let source = Source { exe: Some(r"C:\app.exe".into()), name: Some("App".into()) };
        assert_eq!(h.promote(text("a"), source.clone(), 10, |_| None).as_deref(), Some(id.as_str()));
        assert_eq!(texts(&h), ["a", "c", "b"]);
        assert_eq!(h.entries[0].copied_at, 10);
        assert_eq!(h.entries[0].source_exe, source.exe);
    }

    #[test]
    fn a_pinned_entry_copied_again_keeps_its_pin_and_stays_with_the_pins() {
        let mut h = history(&["a", "b", "c"]);
        let b = h.entries[1].id.clone();
        h.toggle_pin(&b, 5);
        h.promote(text("b"), Source::default(), 6, |_| None);
        assert_eq!(texts(&h), ["b", "c", "a"]);
        assert!(h.entries[0].is_pinned());
        // A new copy goes below the pins.
        h.promote(text("d"), Source::default(), 7, |_| None);
        assert_eq!(texts(&h), ["b", "d", "c", "a"]);
    }

    #[test]
    fn images_match_by_hash_and_files_by_their_paths() {
        let mut h = History::default();
        let image = |hash: &str| Content::Image { png: vec![1], hash: hash.into(), width: 2, height: 3 };
        let mut stored = 0;
        h.promote(image("x"), Source::default(), 1, |_| { stored += 1; Some("x.png".into()) });
        h.promote(image("x"), Source::default(), 2, |_| { stored += 1; Some("y.png".into()) });
        assert_eq!((h.entries.len(), stored), (1, 1));
        assert_eq!(h.entries[0].image_file.as_deref(), Some("x.png"));
        // A picture that couldn't be saved isn't recorded.
        assert_eq!(h.promote(image("z"), Source::default(), 3, |_| None), None);
        let files = |p: &[&str]| Content::Files(p.iter().map(|s| s.to_string()).collect());
        h.promote(files(&["a", "b"]), Source::default(), 4, |_| None);
        h.promote(files(&["b", "a"]), Source::default(), 5, |_| None);
        h.promote(files(&["a", "b"]), Source::default(), 6, |_| None);
        assert_eq!(h.entries.len(), 3);
        // Text that reads like a file list is still text.
        assert!(!h.entries[0].matches(&text("a")));
    }

    #[test]
    fn limit_and_retention_trim_recent_entries_but_never_pins() {
        let day = 86_400_000;
        let mut h = history(&["old", "a", "b", "c"]);
        for (i, e) in h.entries.iter_mut().enumerate() {
            e.copied_at = 100 * day - i as i64 * day;
        }
        // Newest first: c (day 100), b (99), a (98), old (97).
        let old = h.entries[3].id.clone();
        h.toggle_pin(&old, 0);
        h.entries[0].copied_at = 0;
        assert!(h.trim(2, 0, 100 * day));
        assert_eq!(texts(&h), ["old", "c", "b"]);
        assert!(h.trim(0, 1, 100 * day + 1));
        assert_eq!(texts(&h), ["old", "c"]);
        assert!(!h.trim(0, 0, 100 * day));
        // Unlimited keeps everything; retention counts whole days back from now.
        let entries = h.entries.clone();
        assert_eq!(retained(&entries, 0, 0, i64::MAX, usize::MAX).len(), 2);
    }

    #[test]
    fn the_text_budget_keeps_pins_first_and_skips_what_no_longer_fits() {
        let mut h = history(&["aaaa", "bb", "cccc"]);
        let a = h.entries[2].id.clone();
        h.toggle_pin(&a, 0);
        let kept = retained(&h.entries, 0, 0, 0, 6);
        let kept: Vec<_> = kept.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(kept, ["aaaa", "bb"]);
    }

    #[test]
    fn unpinning_returns_to_the_top_of_recent_dated_now() {
        let mut h = history(&["a", "b"]);
        let a = h.entries[1].id.clone();
        h.toggle_pin(&a, 1);
        assert_eq!(texts(&h), ["a", "b"]);
        h.toggle_pin(&a, 42);
        assert_eq!(texts(&h), ["a", "b"]);
        assert_eq!((h.entries[0].is_pinned(), h.entries[0].copied_at), (false, 42));
    }

    #[test]
    fn moving_swaps_within_the_group_only() {
        let mut h = history(&["a", "b", "c"]);
        let c = h.entries[0].id.clone();
        let b = h.entries[1].id.clone();
        h.toggle_pin(&c, 0);
        assert!(!h.move_entry(&c, true));
        assert!(!h.move_entry(&c, false));
        assert!(!h.move_entry(&b, true));
        assert!(h.move_entry(&b, false));
        assert_eq!(texts(&h), ["c", "a", "b"]);
        assert!(!h.move_entry("missing", true));
    }

    #[test]
    fn touching_a_recent_entry_moves_it_to_the_top_of_recent() {
        let mut h = history(&["a", "b", "c"]);
        let a = h.entries[2].id.clone();
        assert!(h.touch(&a, 99));
        assert_eq!(texts(&h), ["a", "c", "b"]);
        assert_eq!(h.entries[0].copied_at, 99);
    }

    #[test]
    fn clear_recent_spares_pins_and_anything_copied_since() {
        let mut h = history(&["a", "b", "c"]);
        let c = h.entries[0].id.clone();
        h.toggle_pin(&c, 0);
        let confirmed: HashMap<_, _> = h.entries.iter().filter(|e| !e.is_pinned()).map(|e| (e.id.clone(), e.copied_at)).collect();
        // "a" is copied again while the confirmation is open.
        h.promote(text("a"), Source::default(), 50, |_| None);
        assert!(h.clear_recent(&confirmed));
        assert_eq!(texts(&h), ["c", "a"]);
    }

    #[test]
    fn history_json_round_trips_and_tolerates_old_or_hostile_files() {
        let mut h = history(&["a"]);
        h.promote(Content::Files(vec![r"C:\x.txt".into()]), Source { exe: Some("e".into()), name: None }, 3, |_| None);
        let id = h.entries[0].id.clone();
        h.toggle_pin(&id, 9);
        let bytes = encode(&h.entries);
        assert_eq!(decode(&bytes).unwrap(), h.entries);
        let json = String::from_utf8(bytes).unwrap();
        assert!(json.contains("\"copiedAt\"") && json.contains("\"pinnedAt\":9") && json.contains("\"filePaths\""));
        // A minimal older entry decodes as text; a path in place of a file name is dropped.
        let old = br#"[{"text":"hi"},{"text":"","kind":"image","imageFile":"..\\..\\secret.png"}]"#;
        let decoded = decode(old).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!((decoded[0].kind, decoded[0].text.as_str(), decoded[0].id.len()), (Kind::Text, "hi", 32));
        assert!(decode(b"{not json").is_none());
        assert!(decode("\u{feff}[]".as_bytes()).unwrap().is_empty());
    }

    #[test]
    fn preference_values_outside_the_mac_choices_read_as_defaults() {
        assert_eq!(sanitized_limit(250), 250);
        assert_eq!(sanitized_limit(0), 0);
        assert_eq!(sanitized_limit(33), 50);
        assert_eq!(sanitized_retention(30), 30);
        assert_eq!(sanitized_retention(2), 0);
    }
}
