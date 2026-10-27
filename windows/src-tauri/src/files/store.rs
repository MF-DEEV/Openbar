// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The shelf's items and the rules they live by (ShelfService.swift,
//! ShelfSupport.swift; spec 05 §6). Items are files, text, links and piles
//! ("batches"): a drop of several things at once becomes one pile. Pinned
//! items stay through a drag-out and Clear All. The list is saved as
//! `shelf\items.json`; pictures and virtual files dropped without a path of
//! their own are written under `shelf\files\` and leave with their item.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Ceilings so a stale or hand-edited file cannot balloon startup: the shelf
/// is a hand-curated surface, not an archive (ShelfPersistenceSupport).
pub const MAX_LEAVES: usize = 200;
pub const MAX_TEXT: usize = 200_000;
pub const MAX_DEPTH: usize = 4;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    File,
    Text,
    Link,
    Batch,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub kind: Kind,
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The volume serial and file id (FILE_ID_INFO), to find a file again
    /// after it was moved or renamed on the same volume. Best effort.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Item>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pinned: bool,
}

/// A random id in the UUID v4 text form, as the Mac's items carry.
pub fn new_id() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).unwrap_or_default();
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

impl Item {
    fn leaf(kind: Kind, title: String) -> Item {
        Item {
            id: new_id(),
            kind,
            title,
            path: None,
            file_id: None,
            text: None,
            url: None,
            children: Vec::new(),
            pinned: false,
        }
    }

    pub fn file(path: &str, file_id: Option<String>) -> Item {
        let mut item = Item::leaf(Kind::File, file_name(path));
        item.path = Some(path.to_string());
        item.file_id = file_id;
        item
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    /// A pasted picture keeps the Mac's "Image" title rather than its stored name.
    pub fn image(path: &str) -> Item {
        let mut item = Item::file(path, None);
        item.title = "Image".into();
        item
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    /// Titled by its first line, up to 48 characters; blank text is nothing.
    pub fn text(text: &str) -> Option<Item> {
        let bounded: String = text.chars().take(MAX_TEXT).collect();
        let trimmed = bounded.trim();
        if trimmed.is_empty() {
            return None;
        }
        let first = trimmed.lines().next().unwrap_or(trimmed).trim();
        let mut item = Item::leaf(Kind::Text, first.chars().take(48).collect());
        item.text = Some(bounded);
        Some(item)
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    /// A web (or other non-file) address, titled by its host.
    pub fn link(url: &str) -> Option<Item> {
        let url = url.trim();
        if !is_link(url) {
            return None;
        }
        let mut item = Item::leaf(Kind::Link, host(url).unwrap_or(url).to_string());
        item.url = Some(url.to_string());
        Some(item)
    }

    /// A pile of `children`, titled "first +N" by its leaf count.
    pub fn batch(children: Vec<Item>) -> Item {
        Item::batch_with_id(new_id(), children)
    }

    fn batch_with_id(id: String, children: Vec<Item>) -> Item {
        let mut item = Item::leaf(Kind::Batch, batch_title(&children));
        item.id = id;
        item.children = children;
        item
    }

    pub fn leaf_count(&self) -> usize {
        match self.kind {
            Kind::Batch => self.children.iter().map(Item::leaf_count).sum(),
            _ => 1,
        }
    }
}

pub fn batch_title(children: &[Item]) -> String {
    let total: usize = children.iter().map(Item::leaf_count).sum();
    children
        .first()
        .map(|first| format!("{} +{}", first.title, total.saturating_sub(1)))
        .unwrap_or_default()
}

fn file_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['\\', '/']);
    trimmed
        .rsplit(['\\', '/'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(trimmed)
        .to_string()
}

/// Has a scheme, and it is not `file:` (a file drop arrives as a path).
pub fn is_link(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once(':') else {
        return false;
    };
    // A drive letter ("C:\…") is a path, not a one-letter scheme.
    scheme.len() > 1
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        && scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && !scheme.eq_ignore_ascii_case("file")
        && !rest.trim().is_empty()
        && !url.contains(char::is_whitespace)
}

#[cfg_attr(not(windows), allow(dead_code))]
fn host(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    (!host.is_empty()).then_some(host)
}

#[cfg_attr(not(windows), allow(dead_code))]
/// What a drop held, in the order the Mac prefers: files (dropped paths,
/// then pictures and virtual files written to the store), else a link,
/// else text (ShelfService.singlePasteboardItems).
#[derive(Default, Debug)]
pub struct Dropped {
    pub paths: Vec<(String, Option<String>)>,
    pub stored: Vec<Item>,
    pub url: Option<String>,
    pub text: Option<String>,
}

#[cfg_attr(not(windows), allow(dead_code))]
pub fn items_from_drop(dropped: Dropped) -> Vec<Item> {
    let mut seen = HashSet::new();
    let mut files: Vec<Item> = dropped
        .paths
        .into_iter()
        .filter(|(path, _)| seen.insert(path.to_lowercase()))
        .map(|(path, id)| Item::file(&path, id))
        .collect();
    files.extend(dropped.stored);
    if !files.is_empty() {
        return files;
    }
    if let Some(link) = dropped.url.as_deref().and_then(Item::link) {
        return vec![link];
    }
    dropped
        .text
        .as_deref()
        .and_then(Item::text)
        .into_iter()
        .collect()
}

/// The live shelf.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct Shelf {
    pub items: Vec<Item>,
}

impl Shelf {
    pub fn leaf_count(&self) -> usize {
        self.items.iter().map(Item::leaf_count).sum()
    }

    pub fn can_add(&self, new_leaves: usize) -> bool {
        new_leaves > 0 && self.leaf_count() + new_leaves <= MAX_LEAVES
    }

    /// One item stays itself; several dropped together become a pile.
    /// Returns the new item's id, or None when it would pass the ceiling.
    pub fn add(&mut self, mut additions: Vec<Item>) -> Option<String> {
        let leaves: usize = additions.iter().map(Item::leaf_count).sum();
        if additions.is_empty() || !self.can_add(leaves) {
            return None;
        }
        let item = if additions.len() == 1 {
            additions.pop().unwrap()
        } else {
            Item::batch(additions)
        };
        let id = item.id.clone();
        self.items.push(item);
        Some(id)
    }

    pub fn find(&self, id: &str) -> Option<&Item> {
        fn walk<'a>(items: &'a [Item], id: &str) -> Option<&'a Item> {
            items
                .iter()
                .find_map(|i| if i.id == id { Some(i) } else { walk(&i.children, id) })
        }
        walk(&self.items, id)
    }

    /// Removes these ids anywhere. A pile left with one item dissolves into
    /// it, which inherits the pile's pin; an emptied pile disappears.
    /// Returns what was taken out.
    pub fn remove(&mut self, ids: &HashSet<String>) -> Vec<Item> {
        fn remove_in(items: Vec<Item>, ids: &HashSet<String>, removed: &mut Vec<Item>) -> Vec<Item> {
            let mut kept = Vec::new();
            for item in items {
                if ids.contains(&item.id) {
                    removed.push(item);
                    continue;
                }
                if item.kind != Kind::Batch {
                    kept.push(item);
                    continue;
                }
                let children = remove_in(item.children, ids, removed);
                match children.len() {
                    0 => {}
                    1 => {
                        let mut only = children.into_iter().next().unwrap();
                        only.pinned |= item.pinned;
                        kept.push(only);
                    }
                    _ => {
                        let mut pile = Item::batch_with_id(item.id, children);
                        pile.pinned = item.pinned;
                        kept.push(pile);
                    }
                }
            }
            kept
        }
        let mut removed = Vec::new();
        self.items = remove_in(std::mem::take(&mut self.items), ids, &mut removed);
        removed
    }

    /// Everything except pinned items, the way the clipboard keeps its pins.
    pub fn clear(&mut self) -> Vec<Item> {
        let protected = self.protected_ids();
        if protected.is_empty() {
            return std::mem::take(&mut self.items);
        }
        let removable: HashSet<String> = leaf_ids(&self.items).difference(&protected).cloned().collect();
        self.remove(&removable)
    }

    pub fn toggle_pin(&mut self, id: &str) -> bool {
        fn walk(items: &mut [Item], id: &str) -> bool {
            for item in items {
                if item.id == id {
                    item.pinned = !item.pinned;
                    return true;
                }
                if walk(&mut item.children, id) {
                    return true;
                }
            }
            false
        }
        walk(&mut self.items, id)
    }

    /// Pinned items and everything nested in a pinned pile.
    pub fn protected_ids(&self) -> HashSet<String> {
        fn walk(items: &[Item], inherited: bool, out: &mut HashSet<String>) {
            for item in items {
                let protected = inherited || item.pinned;
                if protected {
                    out.insert(item.id.clone());
                }
                walk(&item.children, protected, out);
            }
        }
        let mut out = HashSet::new();
        walk(&self.items, false, &mut out);
        out
    }

    /// The leaves under these ids, each once, in shelf order: what a drag or
    /// a file action works on (ShelfService.dragItems).
    pub fn leaves(&self, ids: &[String]) -> Vec<Item> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for id in ids {
            if let Some(item) = self.find(id) {
                collect_leaves(item, &mut out, &mut seen);
            }
        }
        out
    }

    /// Swaps a file's path after it was found again under a new name.
    pub fn heal(&mut self, id: &str, path: &str) {
        fn walk(items: &mut [Item], id: &str, path: &str) {
            for item in items {
                if item.id == id {
                    item.title = file_name(path);
                    item.path = Some(path.to_string());
                    return;
                }
                walk(&mut item.children, id, path);
            }
        }
        walk(&mut self.items, id, path);
        for pile in self.items.iter_mut().filter(|i| i.kind == Kind::Batch) {
            retitle(pile);
        }
    }
}

fn retitle(item: &mut Item) {
    for child in item.children.iter_mut().filter(|c| c.kind == Kind::Batch) {
        retitle(child);
    }
    item.title = batch_title(&item.children);
}

fn collect_leaves(item: &Item, out: &mut Vec<Item>, seen: &mut HashSet<String>) {
    if item.kind == Kind::Batch {
        for child in &item.children {
            collect_leaves(child, out, seen);
        }
    } else if seen.insert(item.id.clone()) {
        out.push(item.clone());
    }
}

pub fn leaf_ids(items: &[Item]) -> HashSet<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for item in items {
        collect_leaves(item, &mut out, &mut seen);
    }
    seen
}

/// Every file path in these items, piles included.
pub fn paths(items: &[Item]) -> Vec<String> {
    let mut out = Vec::new();
    for item in items {
        out.extend(item.path.clone());
        out.extend(paths(&item.children));
    }
    out
}

// MARK: Persistence

/// What `items.json` turned out to be. A file that decoded with entries this
/// build could not read keeps the payload files they point at.
#[derive(Debug, PartialEq)]
pub enum Load {
    Items { items: Vec<Item>, complete: bool },
    Unreadable,
}

/// Entries decode one by one, so an unknown kind written by a newer build
/// drops only itself (ShelfPersistenceSupport.load).
pub fn load(bytes: Option<&[u8]>) -> Load {
    let Some(bytes) = bytes else {
        return Load::Items { items: Vec::new(), complete: true };
    };
    let text = String::from_utf8_lossy(bytes);
    let Ok(Value::Array(entries)) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) else {
        return Load::Unreadable;
    };
    let mut dropped = 0;
    let items: Vec<Item> = entries.iter().filter_map(|v| lossy(v, &mut dropped)).collect();
    if !entries.is_empty() && items.is_empty() {
        return Load::Unreadable;
    }
    Load::Items { items, complete: dropped == 0 }
}

fn lossy(value: &Value, dropped: &mut usize) -> Option<Item> {
    let mut object = value.as_object()?.clone();
    let children = object.remove("children");
    let Ok(mut item) = serde_json::from_value::<Item>(Value::Object(object)) else {
        *dropped += 1;
        return None;
    };
    if let Some(Value::Array(children)) = children {
        item.children = children.iter().filter_map(|c| lossy(c, dropped)).collect();
    }
    Some(item)
}

pub fn encode(items: &[Item]) -> Vec<u8> {
    serde_json::to_vec_pretty(items).unwrap_or_else(|_| b"[]".to_vec())
}

/// Drops what can no longer be honored and mirrors the live pile rules: a
/// missing file gets one chance to be found again by its file id; blank
/// text, file or invalid links go; an emptied pile disappears and a pile of
/// one collapses into its child, which keeps the pin (ShelfPersistenceSupport.sanitized).
pub fn sanitized(
    items: Vec<Item>,
    exists: &dyn Fn(&str) -> bool,
    refind: &dyn Fn(&Item) -> Option<String>,
) -> Vec<Item> {
    fn walk(
        items: Vec<Item>,
        depth: usize,
        remaining: &mut usize,
        exists: &dyn Fn(&str) -> bool,
        refind: &dyn Fn(&Item) -> Option<String>,
    ) -> Vec<Item> {
        if depth >= MAX_DEPTH {
            return Vec::new();
        }
        let mut out = Vec::new();
        for mut item in items {
            if *remaining == 0 {
                break;
            }
            match item.kind {
                Kind::File => {
                    let Some(path) = item.path.clone().filter(|p| !p.is_empty()) else {
                        continue;
                    };
                    if !exists(&path) {
                        let Some(found) = refind(&item).filter(|p| exists(p)) else {
                            continue;
                        };
                        item.title = file_name(&found);
                        item.path = Some(found);
                    }
                    item.children.clear();
                    *remaining -= 1;
                    out.push(item);
                }
                Kind::Text => {
                    let Some(text) = item.text.take().filter(|t| !t.trim().is_empty()) else {
                        continue;
                    };
                    item.text = Some(text.chars().take(MAX_TEXT).collect());
                    item.children.clear();
                    *remaining -= 1;
                    out.push(item);
                }
                Kind::Link => {
                    if !item.url.as_deref().is_some_and(is_link) {
                        continue;
                    }
                    item.children.clear();
                    *remaining -= 1;
                    out.push(item);
                }
                Kind::Batch => {
                    let pinned = item.pinned;
                    let children = walk(item.children, depth + 1, remaining, exists, refind);
                    match children.len() {
                        0 => {}
                        1 => {
                            let mut only = children.into_iter().next().unwrap();
                            only.pinned |= pinned;
                            out.push(only);
                        }
                        _ => {
                            let mut pile = Item::batch_with_id(item.id, children);
                            pile.pinned = pinned;
                            out.push(pile);
                        }
                    }
                }
            }
        }
        out
    }
    let mut remaining = MAX_LEAVES;
    walk(items, 0, &mut remaining, exists, refind)
}

/// A file the shelf itself wrote, which it may therefore delete; files the
/// person dropped are never touched.
pub fn is_owned(path: &str, store: &Path) -> bool {
    let path = PathBuf::from(path);
    path.starts_with(store) && path != store
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(items: &[Item]) -> Vec<&str> {
        items.iter().map(|i| i.title.as_str()).collect()
    }

    fn file(path: &str) -> Item {
        Item::file(path, None)
    }

    #[test]
    fn several_dropped_together_become_one_pile_titled_by_its_first_item() {
        let mut shelf = Shelf::default();
        shelf.add(vec![file(r"C:\a\one.txt")]).unwrap();
        let pile = shelf.add(vec![file(r"C:\a\two.txt"), file(r"C:\a\three.png")]).unwrap();
        assert_eq!(ids(&shelf.items), ["one.txt", "two.txt +1"]);
        let pile = shelf.find(&pile).unwrap();
        assert_eq!(pile.kind, Kind::Batch);
        assert_eq!(pile.leaf_count(), 2);
        assert_eq!(shelf.leaf_count(), 3);
    }

    #[test]
    fn a_pile_left_with_one_item_dissolves_into_it_and_keeps_its_pin() {
        let mut shelf = Shelf::default();
        let pile = shelf.add(vec![file("C:/x/a"), file("C:/x/b")]).unwrap();
        shelf.toggle_pin(&pile);
        let first = shelf.find(&pile).unwrap().children[0].id.clone();
        shelf.remove(&HashSet::from([first]));
        assert_eq!(shelf.items.len(), 1);
        assert_eq!(shelf.items[0].kind, Kind::File);
        assert_eq!(shelf.items[0].title, "b");
        assert!(shelf.items[0].pinned);
    }

    #[test]
    fn removing_from_a_bigger_pile_retitles_it() {
        let mut shelf = Shelf::default();
        let pile = shelf.add(vec![file("C:/x/a"), file("C:/x/b"), file("C:/x/c")]).unwrap();
        let first = shelf.find(&pile).unwrap().children[0].id.clone();
        shelf.remove(&HashSet::from([first]));
        assert_eq!(shelf.find(&pile).unwrap().title, "b +1");
    }

    #[test]
    fn clear_all_keeps_pinned_items_and_everything_in_a_pinned_pile() {
        let mut shelf = Shelf::default();
        let kept = shelf.add(vec![file("C:/x/keep")]).unwrap();
        shelf.add(vec![file("C:/x/gone")]).unwrap();
        let pile = shelf.add(vec![file("C:/x/p1"), file("C:/x/p2")]).unwrap();
        shelf.toggle_pin(&kept);
        shelf.toggle_pin(&pile);
        let removed = shelf.clear();
        assert_eq!(ids(&removed), ["gone"]);
        assert_eq!(ids(&shelf.items), ["keep", "p1 +1"]);
        // With nothing pinned, everything goes.
        shelf.toggle_pin(&kept);
        shelf.toggle_pin(&pile);
        shelf.clear();
        assert!(shelf.items.is_empty());
    }

    #[test]
    fn the_shelf_refuses_more_than_its_ceiling() {
        let mut shelf = Shelf::default();
        for i in 0..MAX_LEAVES {
            assert!(shelf.add(vec![file(&format!("C:/x/{i}"))]).is_some());
        }
        assert!(shelf.add(vec![file("C:/x/over")]).is_none());
        assert_eq!(shelf.leaf_count(), MAX_LEAVES);
    }

    #[test]
    fn leaves_flatten_piles_once_in_order() {
        let mut shelf = Shelf::default();
        let a = shelf.add(vec![file("C:/x/a")]).unwrap();
        let pile = shelf.add(vec![file("C:/x/b"), file("C:/x/c")]).unwrap();
        let b = shelf.find(&pile).unwrap().children[0].id.clone();
        let leaves = shelf.leaves(&[pile.clone(), a, b]);
        assert_eq!(ids(&leaves), ["b", "c", "a"]);
    }

    #[test]
    fn a_drop_prefers_files_then_a_link_then_text() {
        let both = Dropped {
            paths: vec![("C:/a/one.txt".into(), None), ("c:/A/ONE.TXT".into(), None)],
            url: Some("https://example.com/x".into()),
            text: Some("hello".into()),
            ..Default::default()
        };
        assert_eq!(ids(&items_from_drop(both)), ["one.txt"]);
        let link = Dropped {
            url: Some("https://user@example.com:8080/path?q".into()),
            text: Some("ignored".into()),
            ..Default::default()
        };
        let link = items_from_drop(link);
        assert_eq!(link[0].kind, Kind::Link);
        assert_eq!(link[0].title, "example.com:8080");
        let text = Dropped { text: Some("  \n First line  \nsecond".into()), ..Default::default() };
        assert_eq!(ids(&items_from_drop(text)), ["First line"]);
        assert!(items_from_drop(Dropped { text: Some("   ".into()), ..Default::default() }).is_empty());
    }

    #[test]
    fn links_need_a_real_scheme() {
        assert!(is_link("https://example.com"));
        assert!(is_link("mailto:someone@example.com"));
        assert!(!is_link(r"C:\Users\file.txt"));
        assert!(!is_link("file:///C:/x"));
        assert!(!is_link("just words"));
        assert!(!is_link("https://exa mple.com"));
    }

    #[test]
    fn the_saved_shelf_round_trips() {
        let mut shelf = Shelf::default();
        let a = shelf.add(vec![Item::file(r"C:\a\one.txt", Some("1:2".into()))]).unwrap();
        shelf.add(vec![Item::text("note").unwrap(), Item::link("https://example.com").unwrap()]).unwrap();
        shelf.toggle_pin(&a);
        let bytes = encode(&shelf.items);
        assert_eq!(load(Some(&bytes)), Load::Items { items: shelf.items.clone(), complete: true });
        // Unpinned items don't write the flag at all.
        assert_eq!(String::from_utf8(bytes).unwrap().matches("pinned").count(), 1);
    }

    #[test]
    fn an_unknown_kind_drops_only_itself_and_marks_the_load_partial() {
        let json = br#"[{"id":"1","kind":"file","title":"a","path":"C:/a"},
            {"id":"2","kind":"batch","title":"p","children":[{"id":"3","kind":"hologram"},{"id":"4","kind":"text","text":"t"}]},
            {"id":"5","kind":"hologram"}]"#;
        let Load::Items { items, complete } = load(Some(json)) else { panic!() };
        assert!(!complete);
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].children.len(), 1);
        assert_eq!(load(Some(b"{}")), Load::Unreadable);
        assert_eq!(load(Some(br#"[{"kind":"hologram"}]"#)), Load::Unreadable);
        assert_eq!(load(None), Load::Items { items: vec![], complete: true });
    }

    #[test]
    fn restoring_heals_moved_files_and_collapses_piles() {
        let mut shelf = Shelf::default();
        shelf.add(vec![Item::file("C:/gone/old.txt", Some("7:9".into()))]).unwrap();
        let pile = shelf.add(vec![file("C:/x/alive"), file("C:/x/dead")]).unwrap();
        shelf.toggle_pin(&pile);
        let mut stale = Item::text("x").unwrap();
        stale.text = Some("   ".into());
        shelf.items.push(stale);
        let exists = |p: &str| p == "C:/x/alive" || p == "C:/moved/new.txt";
        let refind = |i: &Item| (i.file_id.as_deref() == Some("7:9")).then(|| "C:/moved/new.txt".to_string());
        let restored = sanitized(shelf.items, &exists, &refind);
        assert_eq!(ids(&restored), ["new.txt", "alive"]);
        assert_eq!(restored[0].path.as_deref(), Some("C:/moved/new.txt"));
        assert!(restored[1].pinned, "the survivor inherits the pile's pin");
    }

    #[test]
    fn owned_payloads_are_only_under_the_store() {
        let store = Path::new("/data/shelf/files");
        assert!(is_owned("/data/shelf/files/1/a.png", store));
        assert!(!is_owned("/data/shelf/files", store));
        assert!(!is_owned("/data/shelf/filesx/a.png", store));
        assert!(!is_owned("/home/a.png", store));
    }
}
