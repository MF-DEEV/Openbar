// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! What a file change a session asks for would do, line by line, for the
//! person to read before allowing it (ClaudeFileDiff.swift). Built from the
//! request alone, with the file on disk read only for context; kept in
//! memory with the request and dropped once it is answered.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;
use serde_json::{Map, Value};

use super::models::{char_count, prefix, str_of};

/// Unchanged lines shown around each change.
pub const CONTEXT: usize = 3;
/// Lines kept for one request, across all of its files.
pub const LINE_LIMIT: usize = 400;
/// Characters kept of one line.
pub const TEXT_LIMIT: usize = 300;
/// Lines one click on a gap shows (FileDiffView.tsx shows them).
pub const EXPAND_STEP: usize = 20;
/// Files larger than this are not read for context.
pub const FILE_LIMIT: u64 = 2_000_000;
/// Above this many cells the middle of a diff is shown as removed, then
/// added, instead of being matched line by line.
pub const MATCH_LIMIT: usize = 4_000_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Change {
    /// Lines of an existing file change.
    #[default]
    Update,
    /// A new file, or a new notebook cell.
    Add,
    /// The file, or a notebook cell, goes.
    Delete,
    /// The whole file is written over, and what it holds now could not be read.
    Replace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineKind {
    Context,
    Added,
    Removed,
    /// Lines left out between two changes.
    Gap,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Line {
    pub kind: LineKind,
    pub text: String,
    /// The line's number in the file: the new one, or the old one for a
    /// removed line. None when the file could not be read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<usize>,
    /// What a gap leaves out of the file as read, for the card to show on a
    /// click. None for a gap with no file behind it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap: Option<Gap>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Edge {
    /// Above the first change: opens upward, from the change.
    Top,
    /// Between two changes: opens from either side.
    Middle,
    /// Below the last change: opens downward, from the change.
    Bottom,
}

/// The unchanged lines a gap leaves out, taken from the file read for
/// context when the request came, so showing them reads nothing.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Gap {
    pub edge: Edge,
    /// How many lines it leaves out.
    pub count: usize,
    /// The lines kept from its start, right below the lines above it.
    pub head: Vec<Line>,
    /// The lines kept from its end, right above the lines below it.
    pub tail: Vec<Line>,
}

impl Line {
    fn gap() -> Self {
        Line {
            kind: LineKind::Gap,
            text: String::new(),
            number: None,
            gap: None,
        }
    }

    fn edge(&self) -> Option<Edge> {
        self.gap.as_ref().map(|g| g.edge)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    pub change: Change,
    /// How many places a replace-all changes, when the file was read and
    /// holds more than one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub places: Option<usize>,
    /// A replace-all whose places could not be counted.
    pub every_match: bool,
    /// The notebook cell an edit is for; empty for a notebook edit that
    /// names none. None for anything but a notebook cell.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell: Option<String>,
    /// Where the file moves to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moved_to: Option<String>,
    pub added: usize,
    pub removed: usize,
    pub lines: Vec<Line>,
    /// Lines past the limit, left out.
    pub hidden: usize,
}

impl FileDiff {
    fn new(path: &str) -> Self {
        FileDiff {
            path: path.to_owned(),
            ..Default::default()
        }
    }

    fn count(&mut self) {
        self.added = self
            .lines
            .iter()
            .filter(|l| l.kind == LineKind::Added)
            .count();
        self.removed = self
            .lines
            .iter()
            .filter(|l| l.kind == LineKind::Removed)
            .count();
    }
}

// MARK: Claude Code's tools

/// The diffs for a Claude Code file tool; empty for any other tool.
pub fn diffs(tool: &str, input: &Map<String, Value>, cwd: Option<&str>) -> Vec<FileDiff> {
    diffs_with(tool, input, cwd, &|path| read_text(path), &|path| {
        Path::new(path).exists()
    })
}

/// `diffs` with the file reads passed in, for tests.
pub fn diffs_with(
    tool: &str,
    input: &Map<String, Value>,
    cwd: Option<&str>,
    read: &dyn Fn(&str) -> Option<String>,
    exists: &dyn Fn(&str) -> bool,
) -> Vec<FileDiff> {
    let text = |key: &str| str_of(input.get(key));
    let diff = match tool {
        "Edit" => {
            let (Some(path), Some(old), Some(new)) =
                (text("file_path"), text("old_string"), text("new_string"))
            else {
                return Vec::new();
            };
            let all = input
                .get("replace_all")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Some(edits(
                path,
                &[(old.to_owned(), new.to_owned(), all)],
                read(&resolve(path, cwd)).as_deref(),
            ))
        }
        "MultiEdit" => {
            let (Some(path), Some(list)) = (
                text("file_path"),
                input.get("edits").and_then(Value::as_array),
            ) else {
                return Vec::new();
            };
            let list: Vec<(String, String, bool)> = list
                .iter()
                .filter_map(|edit| {
                    let old = str_of(edit.get("old_string"))?;
                    let new = str_of(edit.get("new_string"))?;
                    let all = edit
                        .get("replace_all")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    Some((old.to_owned(), new.to_owned(), all))
                })
                .collect();
            if list.is_empty() {
                return Vec::new();
            }
            Some(edits(path, &list, read(&resolve(path, cwd)).as_deref()))
        }
        "Write" => {
            let (Some(path), Some(content)) = (text("file_path"), text("content")) else {
                return Vec::new();
            };
            let full = resolve(path, cwd);
            Some(write(path, content, exists(&full), read(&full).as_deref()))
        }
        "NotebookEdit" => {
            let Some(path) = text("notebook_path") else {
                return Vec::new();
            };
            Some(notebook(path, input, read(&resolve(path, cwd)).as_deref()))
        }
        _ => None,
    };
    limited(diff.into_iter().collect())
}

/// Each edit in turn, as Claude Code applies them: later ones see the file
/// as the earlier ones left it. With the file at hand each shows where it
/// lands, with lines around it; without, the edit's own text.
pub fn edits(path: &str, list: &[(String, String, bool)], file: Option<&str>) -> FileDiff {
    let mut diff = FileDiff::new(path);
    let mut content = file.map(normalized);
    // The file does not exist yet: an edit with nothing to replace makes it.
    if file.is_none() && list.len() == 1 && list[0].0.is_empty() {
        diff.change = Change::Add;
    }
    for (index, (old, new, all)) in list.iter().enumerate() {
        let (old, new) = (normalized(old), normalized(new));
        let placed = match &content {
            Some(current) if !old.is_empty() => place(&old, &new, current, *all),
            _ => None,
        };
        let lines = match placed {
            Some((lines, result, count)) => {
                content = Some(result);
                if *all && count > 1 {
                    diff.places = Some(diff.places.unwrap_or(0) + count);
                }
                lines
            }
            None => {
                // Not in the file as read, or no file: the edit's text alone.
                content = None;
                if *all {
                    diff.every_match = true;
                }
                hunks(
                    &matched(&split(&old), &split(&new)),
                    usize::MAX,
                    diff.change == Change::Add,
                )
            }
        };
        if index > 0 && !lines.is_empty() && !diff.lines.is_empty() {
            diff.lines.push(Line::gap());
        }
        diff.lines.extend(lines);
    }
    // Each edit is read against the file as the one before left it, so the
    // file's ends open only above the first and below the last.
    let last = diff.lines.len().saturating_sub(1);
    let mut index = 0;
    diff.lines.retain(|line| {
        let keep = match line.edge() {
            Some(Edge::Top) => index == 0,
            Some(Edge::Bottom) => index == last,
            _ => true,
        };
        index += 1;
        keep
    });
    diff.count();
    diff
}

/// A whole file written: against what is there now, or all new.
pub fn write(path: &str, content: &str, exists: bool, file: Option<&str>) -> FileDiff {
    let mut diff = FileDiff::new(path);
    let new = normalized(content);
    match file {
        Some(file) => {
            let old = normalized(file);
            diff.lines = hunks(&matched(&split(&old), &split(&new)), CONTEXT, true);
        }
        None => {
            diff.change = if exists { Change::Replace } else { Change::Add };
            diff.lines = hunks(&matched(&[], &split(&new)), CONTEXT, true);
        }
    }
    diff.count();
    diff
}

/// A notebook cell replaced, inserted or deleted.
pub fn notebook(path: &str, input: &Map<String, Value>, file: Option<&str>) -> FileDiff {
    let mode = str_of(input.get("edit_mode")).unwrap_or("replace");
    let id = str_of(input.get("cell_id")).filter(|id| !id.is_empty());
    let source = normalized(str_of(input.get("new_source")).unwrap_or(""));
    let mut diff = FileDiff::new(path);
    diff.cell = Some(id.unwrap_or("").to_owned());
    let old = file
        .and_then(|file| cell_source(file, id))
        .map(|text| normalized(&text));
    match mode {
        "insert" => {
            diff.change = Change::Add;
            diff.lines = hunks(&matched(&[], &split(&source)), usize::MAX, true);
        }
        "delete" => {
            diff.change = Change::Delete;
            let text = old.clone().unwrap_or_default();
            diff.lines = hunks(&matched(&split(&text), &[]), usize::MAX, old.is_some());
        }
        _ => {
            let text = old.clone().unwrap_or_default();
            diff.lines = hunks(
                &matched(&split(&text), &split(&source)),
                usize::MAX,
                old.is_some(),
            );
        }
    }
    diff.count();
    diff
}

/// A cell's source, found by its id or, as Claude Code numbers cells
/// without one, by `cell-N`.
pub fn cell_source(notebook: &str, id: Option<&str>) -> Option<String> {
    let id = id?;
    let json: Value = serde_json::from_str(notebook).ok()?;
    let cells = json.get("cells")?.as_array()?;
    let cell = cells
        .iter()
        .find(|cell| str_of(cell.get("id")) == Some(id))
        .or_else(|| {
            let index: usize = id.strip_prefix("cell-")?.parse().ok()?;
            cells.get(index)
        })?;
    match cell.get("source")? {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => Some(parts.iter().filter_map(Value::as_str).collect()),
        _ => None,
    }
}

// MARK: Other agents

/// Codex's file changes: each file with the patch its app server sends.
pub fn codex(changes: &[Value]) -> Vec<FileDiff> {
    limited(
        changes
            .iter()
            .filter_map(|change| {
                let path = str_of(change.get("path"))?;
                let kind = change.get("kind");
                let kind_type = kind.and_then(|k| str_of(k.get("type"))).unwrap_or("update");
                let text = str_of(change.get("diff")).unwrap_or("");
                let whole = match kind_type {
                    "add" => Some(Change::Add),
                    "delete" => Some(Change::Delete),
                    _ => None,
                };
                let mut diff = patch(path, text, whole)
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| FileDiff::new(path));
                diff.path = path.to_owned();
                if let Some(whole) = whole {
                    diff.change = whole;
                }
                diff.moved_to = kind
                    .and_then(|k| str_of(k.get("move_path")))
                    .filter(|p| !p.is_empty())
                    .map(str::to_owned);
                Some(diff)
            })
            .collect(),
    )
}

/// Gemini CLI's edit confirmation: the file before and after, or its patch.
pub fn gemini(details: &Map<String, Value>) -> Vec<FileDiff> {
    if str_of(details.get("type")) != Some("edit") {
        return Vec::new();
    }
    let path = str_of(details.get("filePath"))
        .or_else(|| str_of(details.get("fileName")))
        .unwrap_or("");
    if let Some(new) = str_of(details.get("newContent")) {
        let old = str_of(details.get("originalContent"));
        let modifies = details
            .get("isModifying")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if old.is_none() || (old == Some("") && !modifies) {
            return limited(vec![write(path, new, false, None)]);
        }
        return limited(vec![write(path, new, true, old)]);
    }
    match str_of(details.get("fileDiff")) {
        Some(text) => limited(patch(path, text, None)),
        None => Vec::new(),
    }
}

/// OpenCode's edit permission: the patch it would apply, per file when it
/// patches several.
pub fn opencode(metadata: &Map<String, Value>) -> Vec<FileDiff> {
    if let Some(files) = metadata
        .get("files")
        .and_then(Value::as_array)
        .filter(|f| !f.is_empty())
    {
        return limited(
            files
                .iter()
                .filter_map(|file| {
                    let path = str_of(file.get("filePath"))
                        .or_else(|| str_of(file.get("relativePath")))?;
                    let text = str_of(file.get("patch"))?;
                    let mut diff = patch(path, text, None)
                        .into_iter()
                        .next()
                        .unwrap_or_else(|| FileDiff::new(path));
                    diff.path = path.to_owned();
                    match str_of(file.get("type")) {
                        Some("add") => diff.change = Change::Add,
                        Some("delete") => diff.change = Change::Delete,
                        _ => {}
                    }
                    diff.moved_to = str_of(file.get("movePath"))
                        .filter(|p| !p.is_empty())
                        .map(str::to_owned);
                    Some(diff)
                })
                .collect(),
        );
    }
    let Some(text) = str_of(metadata.get("diff")).filter(|t| !t.is_empty()) else {
        return Vec::new();
    };
    let path = str_of(metadata.get("filepath"))
        .or_else(|| str_of(metadata.get("filePath")))
        .unwrap_or("");
    limited(patch(path, text, None))
}

/// A unified diff read into one diff per file it names. Text with no hunk at
/// all is a whole file's content, added or removed as `whole` says.
pub fn patch(path: &str, text: &str, whole: Option<Change>) -> Vec<FileDiff> {
    let normalized_text = normalized(text);
    let rows = split(&normalized_text);
    if !rows.iter().any(|row| row.starts_with("@@")) {
        let Some(whole) = whole else {
            return Vec::new();
        };
        let mut diff = FileDiff::new(path);
        diff.change = whole;
        let ops = if whole == Change::Delete {
            matched(&rows, &[])
        } else {
            matched(&[], &rows)
        };
        diff.lines = hunks(&ops, usize::MAX, true);
        diff.count();
        return vec![diff];
    }
    let mut files: Vec<FileDiff> = Vec::new();
    let mut current: Option<FileDiff> = None;
    let mut named: Option<String> = None;
    let (mut old_left, mut new_left, mut old_line, mut new_line) = (0usize, 0usize, 0usize, 0usize);
    let finish = |current: &mut Option<FileDiff>, files: &mut Vec<FileDiff>| {
        if let Some(mut diff) = current.take().filter(|diff| !diff.lines.is_empty()) {
            diff.count();
            files.push(diff);
        }
    };
    let header_name = |row: &str, strip: &str| -> Option<String> {
        let name = row[4..].split('\t').next().unwrap_or("");
        (name != "/dev/null").then(|| name.strip_prefix(strip).unwrap_or(name).to_owned())
    };
    for row in &rows {
        if old_left > 0 || new_left > 0 {
            if row.starts_with('\\') {
                continue;
            }
            let body: String = row.chars().skip(1).collect();
            let lines = &mut current.get_or_insert_with(|| FileDiff::new(path)).lines;
            match row.chars().next() {
                Some('+') => {
                    lines.push(Line {
                        kind: LineKind::Added,
                        text: body,
                        number: Some(new_line),
                        gap: None,
                    });
                    new_line += 1;
                    new_left = new_left.saturating_sub(1);
                }
                Some('-') => {
                    lines.push(Line {
                        kind: LineKind::Removed,
                        text: body,
                        number: Some(old_line),
                        gap: None,
                    });
                    old_line += 1;
                    old_left = old_left.saturating_sub(1);
                }
                _ => {
                    lines.push(Line {
                        kind: LineKind::Context,
                        text: body,
                        number: Some(new_line),
                        gap: None,
                    });
                    old_line += 1;
                    new_line += 1;
                    old_left = old_left.saturating_sub(1);
                    new_left = new_left.saturating_sub(1);
                }
            }
            continue;
        }
        if row.starts_with("@@") {
            if let Some((os, ol, ns, nl)) = hunk_header(row) {
                let diff =
                    current.get_or_insert_with(|| FileDiff::new(named.as_deref().unwrap_or(path)));
                if !diff.lines.is_empty() {
                    diff.lines.push(Line::gap());
                }
                (old_line, old_left, new_line, new_left) = (os, ol, ns, nl);
            }
        } else if row.starts_with("diff --git ") || row.starts_with("Index: ") {
            finish(&mut current, &mut files);
            named = None;
        } else if row.starts_with("+++ ") {
            if let Some(name) = header_name(row, "b/") {
                named = Some(name);
            }
            finish(&mut current, &mut files);
        } else if row.starts_with("--- ") {
            finish(&mut current, &mut files);
            if let Some(name) = header_name(row, "a/") {
                named = Some(name);
            }
        }
    }
    finish(&mut current, &mut files);
    // One file: the path the agent gave, which is whole, beats the patch's.
    if files.len() == 1 && !path.is_empty() {
        files[0].path = path.to_owned();
    }
    files
}

/// `@@ -a,b +c,d @@`: where each side starts and how many lines it has.
pub fn hunk_header(row: &str) -> Option<(usize, usize, usize, usize)> {
    let parts: Vec<&str> = row.split(' ').filter(|p| !p.is_empty()).collect();
    if parts.len() < 3 {
        return None;
    }
    let range = |part: &str, sign: char| -> Option<(usize, usize)> {
        let rest = part.strip_prefix(sign)?;
        let mut numbers = rest.splitn(2, ',');
        let start = numbers.next()?.parse().ok()?;
        let length = match numbers.next() {
            Some(text) => text.parse().ok()?,
            None => 1,
        };
        Some((start, length))
    };
    let old = range(parts[1], '-')?;
    let new = range(parts[2], '+')?;
    Some((old.0, old.1, new.0, new.1))
}

// MARK: Building

#[derive(Clone, Debug, PartialEq)]
pub struct Op {
    pub kind: LineKind,
    pub text: String,
    /// Indexes from 0 into the old and the new lines.
    pub old: usize,
    pub new: usize,
}

/// Lines `first..=last` and the places on them that change.
type Group = (usize, usize, Vec<(usize, usize)>);

/// Where `old` sits in `content`, and the lines that change around it: the
/// lines, the content after the change, and how many places changed.
pub fn place(old: &str, new: &str, content: &str, all: bool) -> Option<(Vec<Line>, String, usize)> {
    let found: Vec<(usize, usize)> = if all {
        content
            .match_indices(old)
            .map(|(at, m)| (at, at + m.len()))
            .collect()
    } else {
        content
            .find(old)
            .map(|at| (at, at + old.len()))
            .into_iter()
            .collect()
    };
    if found.is_empty() {
        return None;
    }
    // Where each line starts, to turn places into whole lines.
    let mut starts = vec![0usize];
    starts.extend(
        content
            .bytes()
            .enumerate()
            .filter(|(_, b)| *b == b'\n')
            .map(|(i, _)| i + 1),
    );
    let line_of = |offset: usize| -> usize { starts.partition_point(|&start| start <= offset) - 1 };
    // Places on the same lines change together.
    let mut groups: Vec<Group> = Vec::new();
    for &(start, end) in &found {
        let (first, last) = (line_of(start), line_of(end - 1));
        match groups.last_mut() {
            Some(group) if group.1 >= first => {
                group.1 = group.1.max(last);
                group.2.push((start, end));
            }
            _ => groups.push((first, last, vec![(start, end)])),
        }
    }
    let old_lines = split(content);
    let mut ops: Vec<Op> = Vec::new();
    let (mut old_index, mut new_index) = (0usize, 0usize);
    let mut result = String::with_capacity(content.len());
    let mut cursor = 0usize;
    for (first, last, ranges) in &groups {
        while old_index < *first && old_index < old_lines.len() {
            ops.push(Op {
                kind: LineKind::Context,
                text: old_lines[old_index].clone(),
                old: old_index,
                new: new_index,
            });
            old_index += 1;
            new_index += 1;
        }
        let start = starts[*first];
        let end = if last + 1 < starts.len() {
            starts[last + 1]
        } else {
            content.len()
        };
        let mut replaced = String::new();
        let mut at = start;
        for &(from, to) in ranges {
            replaced.push_str(&content[at..from]);
            replaced.push_str(new);
            at = to;
        }
        replaced.push_str(&content[at..end]);
        result.push_str(&content[cursor..start]);
        result.push_str(&replaced);
        cursor = end;
        let before = split(&content[start..end]);
        let after = split(&replaced);
        for op in matched(&before, &after) {
            ops.push(Op {
                old: old_index + op.old,
                new: new_index + op.new,
                ..op
            });
        }
        old_index += before.len();
        new_index += after.len();
    }
    result.push_str(&content[cursor..]);
    while old_index < old_lines.len() {
        ops.push(Op {
            kind: LineKind::Context,
            text: old_lines[old_index].clone(),
            old: old_index,
            new: new_index,
        });
        old_index += 1;
        new_index += 1;
    }
    Some((hunks(&ops, CONTEXT, true), result, found.len()))
}

/// The lines of `old` and `new` matched up: kept, removed and added, in
/// order. Lines both share at the start and the end are kept without
/// matching the rest; a middle too large to match is removed, then added.
pub fn matched(old: &[String], new: &[String]) -> Vec<Op> {
    let mut head = 0;
    while head < old.len() && head < new.len() && old[head] == new[head] {
        head += 1;
    }
    let (mut old_end, mut new_end) = (old.len(), new.len());
    while old_end > head && new_end > head && old[old_end - 1] == new[new_end - 1] {
        old_end -= 1;
        new_end -= 1;
    }
    let mut ops: Vec<Op> = (0..head)
        .map(|i| Op {
            kind: LineKind::Context,
            text: new[i].clone(),
            old: i,
            new: i,
        })
        .collect();
    let (n, m) = (old_end - head, new_end - head);
    if n == 0 || m == 0 || n.saturating_mul(m) > MATCH_LIMIT {
        ops.extend((head..old_end).map(|i| Op {
            kind: LineKind::Removed,
            text: old[i].clone(),
            old: i,
            new: head,
        }));
        ops.extend((head..new_end).map(|j| Op {
            kind: LineKind::Added,
            text: new[j].clone(),
            old: old_end,
            new: j,
        }));
    } else {
        // Lines as numbers, which compare far faster than text.
        let mut ids: HashMap<&str, u32> = HashMap::new();
        let mut a: Vec<u32> = Vec::with_capacity(n);
        for line in &old[head..old_end] {
            let next = ids.len() as u32;
            a.push(*ids.entry(line.as_str()).or_insert(next));
        }
        let mut b: Vec<u32> = Vec::with_capacity(m);
        for line in &new[head..new_end] {
            let next = ids.len() as u32;
            b.push(*ids.entry(line.as_str()).or_insert(next));
        }
        let width = m + 1;
        let mut table = vec![0u32; (n + 1) * width];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                table[i * width + j] = if a[i] == b[j] {
                    table[(i + 1) * width + j + 1] + 1
                } else {
                    table[(i + 1) * width + j].max(table[i * width + j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n || j < m {
            if i < n && j < m && a[i] == b[j] {
                ops.push(Op {
                    kind: LineKind::Context,
                    text: new[head + j].clone(),
                    old: head + i,
                    new: head + j,
                });
                i += 1;
                j += 1;
            } else if j >= m || (i < n && table[(i + 1) * width + j] >= table[i * width + j + 1]) {
                ops.push(Op {
                    kind: LineKind::Removed,
                    text: old[head + i].clone(),
                    old: head + i,
                    new: head + j,
                });
                i += 1;
            } else {
                ops.push(Op {
                    kind: LineKind::Added,
                    text: new[head + j].clone(),
                    old: head + i,
                    new: head + j,
                });
                j += 1;
            }
        }
    }
    ops.extend((0..old.len() - old_end).map(|k| Op {
        kind: LineKind::Context,
        text: old[old_end + k].clone(),
        old: old_end + k,
        new: new_end + k,
    }));
    ops
}

/// The changed lines with `context` unchanged ones around each, and a gap
/// where unchanged lines are left out. Numbered ops are the whole file, so
/// each gap keeps the lines it leaves out, the file's start and end
/// included, for the card to show on a click.
pub fn hunks(ops: &[Op], context: usize, numbered: bool) -> Vec<Line> {
    let changes: Vec<usize> = (0..ops.len())
        .filter(|&i| ops[i].kind != LineKind::Context)
        .collect();
    if changes.is_empty() {
        return Vec::new();
    }
    let mut keep = vec![context == usize::MAX; ops.len()];
    if context != usize::MAX {
        for &index in &changes {
            let low = index.saturating_sub(context);
            let high = (index + context).min(ops.len() - 1);
            for kept in &mut keep[low..=high] {
                *kept = true;
            }
        }
    }
    let line = |op: &Op| {
        Line {
        kind: op.kind,
        text: op.text.clone(),
        number: numbered.then(|| if op.kind == LineKind::Removed { op.old } else { op.new } + 1),
        gap: None,
    }
    };
    let gap = |lines: &mut Vec<Line>, skipped: &mut Vec<Line>, edge: Edge| {
        if skipped.is_empty() {
            return;
        }
        let left = std::mem::take(skipped);
        if !numbered {
            if edge == Edge::Middle {
                lines.push(Line::gap());
            }
            return;
        }
        lines.push(Line {
            gap: Some(Gap {
                edge,
                count: left.len(),
                head: left,
                tail: Vec::new(),
            }),
            ..Line::gap()
        });
    };
    let mut lines = Vec::new();
    let mut skipped = Vec::new();
    for (index, op) in ops.iter().enumerate() {
        if !keep[index] {
            skipped.push(line(op));
            continue;
        }
        let edge = if lines.is_empty() {
            Edge::Top
        } else {
            Edge::Middle
        };
        gap(&mut lines, &mut skipped, edge);
        lines.push(line(op));
    }
    gap(&mut lines, &mut skipped, Edge::Bottom);
    lines
}

/// At most `LINE_LIMIT` lines across the files, the rest counted, and each
/// line cut at `TEXT_LIMIT` characters. What the limit leaves over keeps
/// lines inside the gaps, a click's worth at a time for each side a gap
/// opens from, nearest the changes first.
pub fn limited(diffs: Vec<FileDiff>) -> Vec<FileDiff> {
    let mut left = LINE_LIMIT;
    let mut diffs: Vec<FileDiff> = diffs
        .into_iter()
        .map(|mut diff| {
            let shown = left.min(diff.lines.len());
            let cut_off = shown < diff.lines.len();
            diff.hidden += diff.lines.len() - shown;
            diff.lines.truncate(shown);
            diff.lines.iter_mut().for_each(cut);
            // A gap the limit left last has no change below it any more.
            if let (true, Some(last)) = (cut_off, diff.lines.last_mut()) {
                last.gap = None;
            }
            left -= shown;
            diff
        })
        .collect();
    // Every side a gap opens from: (file, line, from its end, lines kept).
    let mut sides: Vec<(usize, usize, bool, usize)> = Vec::new();
    for (file, diff) in diffs.iter().enumerate() {
        for (index, line) in diff.lines.iter().enumerate() {
            match line.edge() {
                Some(Edge::Top) => sides.push((file, index, true, 0)),
                Some(Edge::Bottom) => sides.push((file, index, false, 0)),
                Some(Edge::Middle) => {
                    sides.push((file, index, false, 0));
                    sides.push((file, index, true, 0));
                }
                None => {}
            }
        }
    }
    let mut grew = true;
    while left > 0 && grew {
        grew = false;
        for i in 0..sides.len() {
            if left == 0 {
                break;
            }
            let (file, index, from_end, kept) = sides[i];
            let count = diffs[file].lines[index]
                .gap
                .as_ref()
                .map_or(0, |g| g.head.len());
            // Both sides of one gap share its lines.
            let other: usize = sides
                .iter()
                .filter(|s| s.0 == file && s.1 == index && s.2 != from_end)
                .map(|s| s.3)
                .sum();
            let more = EXPAND_STEP.min(left).min(count - kept - other);
            if more > 0 {
                sides[i].3 += more;
                left -= more;
                grew = true;
            }
        }
    }
    let kept = |file: usize, index: usize, from_end: bool| {
        sides
            .iter()
            .find(|s| s.0 == file && s.1 == index && s.2 == from_end)
            .map_or(0, |s| s.3)
    };
    for (file, diff) in diffs.iter_mut().enumerate() {
        for (index, line) in diff.lines.iter_mut().enumerate() {
            let Some(gap) = line.gap.as_mut() else {
                continue;
            };
            let all = std::mem::take(&mut gap.head);
            let (down, up) = (kept(file, index, false), kept(file, index, true));
            gap.head = all[..down].to_vec();
            gap.tail = all[all.len() - up..].to_vec();
            gap.head.iter_mut().chain(gap.tail.iter_mut()).for_each(cut);
        }
    }
    diffs
}

fn cut(line: &mut Line) {
    if char_count(&line.text) > TEXT_LIMIT {
        line.text = format!("{}…", prefix(&line.text, TEXT_LIMIT));
    }
}

// MARK: Text

/// Windows and old Mac line ends read as one.
pub fn normalized(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_owned();
    }
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// The lines of a text; a final line break does not start another.
pub fn split(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

pub fn resolve(path: &str, cwd: Option<&str>) -> String {
    match cwd {
        Some(cwd)
            if !cwd.is_empty() && !Path::new(path).is_absolute() && !path.starts_with('/') =>
        {
            Path::new(cwd).join(path).to_string_lossy().into_owned()
        }
        _ => path.to_owned(),
    }
}

/// A file's text, when it is small, readable and not binary.
pub fn read_text(path: &str) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > FILE_LIMIT {
        return None;
    }
    let data = std::fs::read(path).ok()?;
    if data.iter().take(8192).any(|b| *b == 0) {
        return None;
    }
    let text = String::from_utf8(data).ok()?;
    // A byte order mark is not part of the first line.
    Some(
        text.strip_prefix('\u{FEFF}')
            .map(str::to_owned)
            .unwrap_or(text),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap()
    }

    fn kinds(diff: Option<&FileDiff>) -> String {
        diff.map(|d| d.lines.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(|line| match line.kind {
                LineKind::Added => format!("+{}", line.text),
                LineKind::Removed => format!("-{}", line.text),
                LineKind::Context => format!(" {}", line.text),
                LineKind::Gap => "~".to_owned(),
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    fn numbers(diff: Option<&FileDiff>) -> Vec<Option<usize>> {
        diff.map(|d| d.lines.as_slice())
            .unwrap_or(&[])
            .iter()
            .filter(|l| l.kind != LineKind::Gap)
            .map(|l| l.number)
            .collect()
    }

    fn file(count: usize) -> String {
        (1..=count).map(|i| format!("line {i}\n")).collect()
    }

    fn edit(old: &str, new: &str, all: bool, content: Option<&str>) -> Option<FileDiff> {
        let content = content.map(str::to_owned);
        diffs_with(
            "Edit",
            &obj(json!({"file_path": "C:\\p\\a.rs", "old_string": old, "new_string": new, "replace_all": all})),
            None,
            &move |_| content.clone(),
            &|_| false,
        )
        .into_iter()
        .next()
    }

    #[test]
    fn edits_read_alone_or_in_the_file() {
        let bare = edit("a\nb\nc", "a\nB\nc", false, None);
        assert_eq!(kinds(bare.as_ref()), " a|-b|+B| c");
        assert!(numbers(bare.as_ref()).iter().all(Option::is_none));
        let placed = edit("line 10\n", "ten\n", false, Some(&file(20)));
        assert_eq!(
            kinds(placed.as_ref()),
            "~| line 7| line 8| line 9|-line 10|+ten| line 11| line 12| line 13|~"
        );
        assert_eq!(
            numbers(placed.as_ref()),
            [7, 8, 9, 10, 10, 11, 12, 13].map(Some).to_vec()
        );
        let inside = edit("ine 2", "INE 2", false, Some(&file(3)));
        assert_eq!(kinds(inside.as_ref()), " line 1|-line 2|+lINE 2| line 3");
        let missing = edit("absent", "x", false, Some(&file(3)));
        assert_eq!(kinds(missing.as_ref()), "-absent|+x");
    }

    #[test]
    fn empty_strings() {
        let created = edit("", "first\nsecond", false, None).unwrap();
        assert_eq!(created.change, Change::Add);
        assert_eq!(kinds(Some(&created)), "+first|+second");
        assert_eq!(numbers(Some(&created)), vec![Some(1), Some(2)]);
        let removed = edit("gone\n", "", false, None).unwrap();
        assert_eq!((removed.added, removed.removed), (0, 1));
        assert!(edit("", "", false, None).unwrap().lines.is_empty());
        assert_eq!(
            kinds(edit("line 2\n", "", false, Some(&file(3))).as_ref()),
            " line 1|-line 2| line 3"
        );
    }

    #[test]
    fn replace_all_counts_its_places() {
        let mut rows: Vec<String> = (1..=20).map(|i| format!("row {i}")).collect();
        rows[1] = "foo".into();
        rows[14] = "foo".into();
        let all = edit("foo", "bar", true, Some(&rows.join("\n"))).unwrap();
        assert_eq!((all.places, all.added, all.removed), (Some(2), 2, 2));
        assert!(kinds(Some(&all)).contains('~'));
        let blind = edit("foo", "bar", true, None).unwrap();
        assert!(blind.every_match && blind.places.is_none());
        let once = edit("foo", "bar", true, Some("foo\n")).unwrap();
        assert!(once.places.is_none() && !once.every_match);
        let same_line = edit("o", "0", true, Some("foo boo\nzzz\n")).unwrap();
        assert_eq!(kinds(Some(&same_line)), "-foo boo|+f00 b00| zzz");
        assert_eq!(same_line.places, Some(4));
    }

    #[test]
    fn crlf_reads_as_lf() {
        let crlf = edit(
            "two\nthree",
            "2\n3",
            false,
            Some("one\r\ntwo\r\nthree\r\nfour\r\n"),
        );
        assert_eq!(kinds(crlf.as_ref()), " one|-two|-three|+2|+3| four");
        assert_eq!(numbers(crlf.as_ref())[0], Some(1));
        assert_eq!(
            kinds(edit("two\r\n", "2\r\n", false, Some("one\ntwo\n")).as_ref()),
            " one|-two|+2"
        );
        let same = write("C:\\p\\s", "a\n", true, Some("a\r\n"));
        assert!(same.lines.is_empty());
    }

    #[test]
    fn multi_edits_apply_in_turn() {
        let content = file(20);
        let multi = diffs_with(
            "MultiEdit",
            &obj(json!({"file_path": "/p/m.txt", "edits": [
                {"old_string": "line 2", "new_string": "two"},
                {"old_string": "two\nline 3", "new_string": "two\nthree"},
                {"old_string": "line 19", "new_string": "nineteen"},
                {"bad": true}
            ]})),
            None,
            &move |_| Some(content.clone()),
            &|_| true,
        );
        let text = kinds(multi.first());
        let hunks: Vec<&str> = text.split('~').collect();
        assert_eq!(hunks.len(), 3, "{text}");
        assert!(hunks[1].contains("-line 3|+three") && hunks[1].contains(" two"));
        assert_eq!((multi[0].added, multi[0].removed), (3, 3));
        let none = diffs_with(
            "MultiEdit",
            &obj(json!({"file_path": "/p", "edits": []})),
            None,
            &|_| None,
            &|_| false,
        );
        assert!(none.is_empty());
        assert!(
            diffs_with(
                "Bash",
                &obj(json!({"command": "ls"})),
                None,
                &|_| None,
                &|_| false
            )
            .is_empty()
        );
    }

    #[test]
    fn writes_and_notebooks() {
        let fresh = write("/p/new", "a\nb\n", false, None);
        assert_eq!(
            (fresh.change, kinds(Some(&fresh))),
            (Change::Add, "+a|+b".to_owned())
        );
        let unread = write("/p/locked", "x", true, None);
        assert_eq!(unread.change, Change::Replace);
        let cleared = write("/p/c", "", true, Some("a\nb"));
        assert_eq!(kinds(Some(&cleared)), "-a|-b");

        let notebook = r##"{"cells":[{"id":"intro","source":["# Title\n","text"]},{"source":"print(1)\nprint(2)"}]}"##;
        let cell = |input: Value| {
            let mut input = obj(input);
            input.insert("notebook_path".into(), json!("/p/n.ipynb"));
            diffs_with(
                "NotebookEdit",
                &input,
                None,
                &|_| Some(notebook.to_owned()),
                &|_| true,
            )
            .into_iter()
            .next()
            .unwrap()
        };
        let replaced = cell(json!({"cell_id": "intro", "new_source": "# Title\nmore"}));
        assert_eq!(kinds(Some(&replaced)), " # Title|-text|+more");
        assert_eq!(replaced.cell.as_deref(), Some("intro"));
        assert_eq!(
            kinds(Some(&cell(
                json!({"cell_id": "cell-1", "new_source": "print(3)\nprint(2)"})
            ))),
            "-print(1)|+print(3)| print(2)"
        );
        let inserted =
            cell(json!({"cell_id": "intro", "new_source": "x = 1", "edit_mode": "insert"}));
        assert_eq!(
            (inserted.change, inserted.cell.is_some()),
            (Change::Add, true)
        );
        let deleted = cell(json!({"cell_id": "intro", "edit_mode": "delete"}));
        assert_eq!(
            (deleted.change, kinds(Some(&deleted))),
            (Change::Delete, "-# Title|-text".to_owned())
        );
        let unknown = cell(json!({"cell_id": "nope", "new_source": "y"}));
        assert_eq!(numbers(Some(&unknown)), vec![None]);
    }

    #[test]
    fn real_files_are_read_with_care() {
        let folder = std::env::temp_dir().join(format!("vs-diff-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("f.txt"), "keep\r\nold\r\n").unwrap();
        let disk = diffs(
            "Write",
            &obj(json!({"file_path": "f.txt", "content": "keep\nnew\n"})),
            folder.to_str(),
        );
        assert_eq!(kinds(disk.first()), " keep|-old|+new");
        let absent = diffs(
            "Write",
            &obj(json!({"file_path": folder.join("no.txt").to_str().unwrap(), "content": "x"})),
            None,
        );
        assert_eq!(absent[0].change, Change::Add);
        std::fs::write(folder.join("b.bin"), [0x61u8, 0, 0x62]).unwrap();
        assert!(read_text(folder.join("b.bin").to_str().unwrap()).is_none());
        assert!(read_text(folder.to_str().unwrap()).is_none());
        std::fs::write(folder.join("big.txt"), vec![b'a'; FILE_LIMIT as usize + 1]).unwrap();
        let big = diffs(
            "Write",
            &obj(json!({"file_path": folder.join("big.txt").to_str().unwrap(), "content": "a"})),
            None,
        );
        assert_eq!(big[0].change, Change::Replace);
        std::fs::write(folder.join("bom.txt"), "\u{FEFF}first\n").unwrap();
        assert_eq!(
            read_text(folder.join("bom.txt").to_str().unwrap()).as_deref(),
            Some("first\n")
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn patches_from_other_agents() {
        let update = codex(&[
            json!({"path": "C:\\w\\a.rs", "kind": {"type": "update", "move_path": null},
                                    "diff": "@@ -3,3 +3,3 @@\n keep\n-old\n+new\n tail\n"}),
        ]);
        assert_eq!(kinds(update.first()), " keep|-old|+new| tail");
        assert_eq!(numbers(update.first()), [3, 4, 4, 5].map(Some).to_vec());
        assert_eq!(update[0].path, "C:\\w\\a.rs");
        let add = codex(&[json!({"path": "/w/n", "kind": {"type": "add"}, "diff": "one\ntwo\n"})]);
        assert_eq!(
            (add[0].change, kinds(add.first())),
            (Change::Add, "+one|+two".to_owned())
        );
        let delete = codex(&[json!({"path": "/w/d", "kind": {"type": "delete"}, "diff": "bye\n"})]);
        assert_eq!(kinds(delete.first()), "-bye");
        let moved = codex(&[
            json!({"path": "/w/a", "kind": {"type": "update", "move_path": "/w/b"}, "diff": ""}),
        ]);
        assert_eq!(moved[0].moved_to.as_deref(), Some("/w/b"));

        let hunks = patch(
            "/f",
            "--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n-a\n+A\n b\n@@ -10 +10 @@\n-j\n\\ No newline at end of file\n+J\n",
            None,
        );
        assert_eq!(kinds(hunks.first()), "-a|+A| b|~|-j|+J");
        assert_eq!(numbers(hunks.first()), [1, 1, 2, 10, 10].map(Some).to_vec());
        let tricky = patch("/f", "@@ -1,2 +1,1 @@\n--- not a header\n keep\n", None);
        assert_eq!(kinds(tricky.first()), "--- not a header| keep");

        let open = opencode(&obj(json!({"filepath": "/w/o.ts", "diff":
            "Index: /w/o.ts\n===================================================================\n--- /w/o.ts\n+++ /w/o.ts\n@@ -1,1 +1,1 @@\n-a\n+b\n"})));
        assert_eq!(
            (open.len(), open[0].path.as_str(), kinds(open.first())),
            (1, "/w/o.ts", "-a|+b".to_owned())
        );
        let files = opencode(&obj(json!({"files": [
            {"filePath": "/w/a", "type": "update", "patch": "@@ -1 +1 @@\n-1\n+2\n"},
            {"filePath": "/w/b", "type": "add", "patch": "@@ -0,0 +1 @@\n+new\n"}
        ]})));
        assert_eq!(
            files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            ["/w/a", "/w/b"]
        );
        assert_eq!(files[1].change, Change::Add);
        assert!(opencode(&obj(json!({"command": "ls"}))).is_empty());

        let gem = gemini(&obj(
            json!({"type": "edit", "filePath": "/w/g.py", "originalContent": "a\nb\n", "newContent": "a\nc\n"}),
        ));
        assert_eq!(kinds(gem.first()), " a|-b|+c");
        let gem_new = gemini(&obj(
            json!({"type": "edit", "filePath": "/w/n.py", "originalContent": null, "newContent": "z"}),
        ));
        assert_eq!(gem_new[0].change, Change::Add);
        let gem_patch = gemini(&obj(
            json!({"type": "edit", "filePath": "/w/p.py", "fileDiff": "@@ -1 +1 @@\n-q\n+r\n"}),
        ));
        assert_eq!(kinds(gem_patch.first()), "-q|+r");
        assert!(gemini(&obj(json!({"type": "exec", "command": "ls"}))).is_empty());
    }

    #[test]
    fn limits_keep_it_small_and_quick() {
        let long: String = (1..=5000).map(|i| format!("l{i}\n")).collect();
        let huge = diffs_with(
            "Write",
            &obj(json!({"file_path": "/x", "content": long})),
            None,
            &|_| None,
            &|_| false,
        );
        assert_eq!(
            (huge[0].added, huge[0].lines.len(), huge[0].hidden),
            (5000, LINE_LIMIT, 5000 - LINE_LIMIT)
        );
        let wide = edit("a", &"w".repeat(5000), false, None).unwrap();
        let last = wide.lines.last().unwrap();
        assert_eq!(char_count(&last.text), TEXT_LIMIT + 1);
        assert!(last.text.ends_with('…'));
        let changes: Vec<Value> = (1..=3)
            .map(|i| json!({"path": format!("/w/{i}"), "kind": {"type": "add"},
                            "diff": (1..=300).map(|n| n.to_string()).collect::<Vec<_>>().join("\n")}))
            .collect();
        let shared = codex(&changes);
        assert_eq!(
            shared.iter().map(|d| d.lines.len()).collect::<Vec<_>>(),
            [300, 100, 0]
        );
        assert_eq!(
            shared.iter().map(|d| d.hidden).collect::<Vec<_>>(),
            [0, 200, 300]
        );
        let a: Vec<String> = (1..=3000).map(|i| format!("a{i}")).collect();
        let b: Vec<String> = (1..=3000).map(|i| format!("b{i}")).collect();
        let ops = matched(&a, &b);
        assert_eq!(
            ops.iter().filter(|o| o.kind == LineKind::Removed).count(),
            3000
        );
        let big: String = (1..=50_000).map(|i| format!("row {i}\n")).collect();
        let one = edit("row 25000\n", "changed\n", false, Some(&big)).unwrap();
        assert_eq!((one.added, numbers(Some(&one))[0]), (1, Some(24997)));
    }

    #[test]
    fn serializes_for_the_card() {
        let diff = edit("a", "b", true, None).unwrap();
        let value = serde_json::to_value(&diff).unwrap();
        assert_eq!(value["change"], "update");
        assert_eq!(value["everyMatch"], true);
        assert_eq!(value["lines"][0]["kind"], "removed");
        assert!(value.get("places").is_none() && value["lines"][0].get("number").is_none());
    }

    fn gap_at(diff: &FileDiff, edge: Edge) -> Option<&Gap> {
        diff.lines
            .iter()
            .filter_map(|l| l.gap.as_ref())
            .find(|g| g.edge == edge)
    }

    fn numbers_of(lines: &[Line]) -> Vec<usize> {
        lines.iter().filter_map(|l| l.number).collect()
    }

    #[test]
    fn gaps_keep_the_lines_they_leave_out() {
        // First and last hunk: the file's start opens upward, its end downward.
        let mid = edit("line 10\n", "ten\n", false, Some(&file(20))).unwrap();
        let top = gap_at(&mid, Edge::Top).unwrap();
        assert_eq!(
            (top.count, numbers_of(&top.tail), top.head.len()),
            (6, (1..=6).collect(), 0)
        );
        let bottom = gap_at(&mid, Edge::Bottom).unwrap();
        assert_eq!(bottom.count, 7);
        assert_eq!(
            bottom
                .head
                .iter()
                .map(|l| l.text.clone())
                .collect::<Vec<_>>(),
            (14..=20).map(|i| format!("line {i}")).collect::<Vec<_>>()
        );
        assert!(bottom.tail.is_empty());
        assert!(top.tail.iter().all(
            |l| l.kind == LineKind::Context && l.text == format!("line {}", l.number.unwrap())
        ));

        // Between two changes: lines from both sides, numbered as in the file.
        let mut rows: Vec<String> = (1..=100).map(|i| format!("row {i}")).collect();
        rows[9] = "foo".into();
        rows[79] = "foo".into();
        let apart = edit("foo", "bar", true, Some(&rows.join("\n"))).unwrap();
        let middle = gap_at(&apart, Edge::Middle).unwrap();
        assert_eq!(middle.count, 63);
        let both: Vec<usize> = numbers_of(&middle.head)
            .into_iter()
            .chain(numbers_of(&middle.tail))
            .collect();
        assert_eq!(
            both,
            (14..=76).collect::<Vec<_>>(),
            "the whole gap is kept, once"
        );
        assert!(!middle.head.is_empty() && !middle.tail.is_empty());

        // Adjacent hunks: no gap between them.
        let mut close: Vec<String> = (1..=20).map(|i| format!("row {i}")).collect();
        close[4] = "foo".into();
        close[11] = "foo".into();
        let touching = edit("foo", "bar", true, Some(&close.join("\n"))).unwrap();
        assert!(gap_at(&touching, Edge::Middle).is_none());
        assert!(!kinds(Some(&touching)).contains("|~|"));
        let ends = edit("line 1\n", "one\n", false, Some(&file(4))).unwrap();
        assert!(ends.lines.iter().all(|l| l.kind != LineKind::Gap));

        // CRLF: kept lines carry no carriage return.
        let crlf: String = (1..=12).map(|i| format!("r{i}\r\n")).collect();
        let crlf = edit("r10", "ten", false, Some(&crlf)).unwrap();
        assert_eq!(
            gap_at(&crlf, Edge::Top)
                .unwrap()
                .tail
                .iter()
                .map(|l| l.text.clone())
                .collect::<Vec<_>>(),
            (1..=6).map(|i| format!("r{i}")).collect::<Vec<_>>()
        );

        // The file changed on disk after the request: the diff keeps what it
        // read, and an edit no longer in the file has nothing to open.
        let disk = std::cell::RefCell::new(file(20));
        let request = diffs_with(
            "Edit",
            &obj(
                json!({"file_path": "C:\\p\\a.rs", "old_string": "line 10\n", "new_string": "ten\n"}),
            ),
            None,
            &|_| Some(disk.borrow().clone()),
            &|_| true,
        );
        *disk.borrow_mut() = "changed\n".into();
        assert_eq!(
            gap_at(&request[0], Edge::Top).unwrap().tail[0].text,
            "line 1"
        );
        let stale = edit("line 10\n", "ten\n", false, Some("something else\n")).unwrap();
        assert!(stale.lines.iter().all(|l| l.gap.is_none()));

        // Nothing to show: no file, a new file, a bare patch.
        assert!(
            edit("line 10\n", "ten\n", false, None)
                .unwrap()
                .lines
                .iter()
                .all(|l| l.kind != LineKind::Gap)
        );
        let fresh = write("C:\\n", &file(30), false, None);
        assert!(fresh.lines.iter().all(|l| l.kind == LineKind::Added));
        let bare = codex(&[json!({"path": "/w/x", "kind": {"type": "update"},
                                  "diff": "@@ -1 +1 @@\n-a\n+b\n@@ -40 +40 @@\n-c\n+d\n"})]);
        assert!(bare[0].lines.iter().any(|l| l.kind == LineKind::Gap));
        assert!(bare[0].lines.iter().all(|l| l.gap.is_none()));
        let gem = gemini(&obj(
            json!({"type": "edit", "filePath": "/g", "originalContent": file(20),
                                     "newContent": file(20).replace("line 10\n", "ten\n"), "isModifying": true}),
        ));
        assert_eq!(gap_at(&gem[0], Edge::Top).unwrap().tail.len(), 6);

        // MultiEdit: the file's ends open only around the first and last edit.
        let multi = diffs_with(
            "MultiEdit",
            &obj(json!({"file_path": "C:\\m", "edits": [
                {"old_string": "line 10", "new_string": "ten"},
                {"old_string": "line 30", "new_string": "thirty"}]})),
            None,
            &|_| Some(file(40)),
            &|_| true,
        );
        let edges: Vec<Edge> = multi[0].lines.iter().filter_map(Line::edge).collect();
        assert_eq!(edges, [Edge::Top, Edge::Bottom]);
        assert_eq!(multi[0].lines.first().unwrap().edge(), Some(Edge::Top));
        assert_eq!(multi[0].lines.last().unwrap().edge(), Some(Edge::Bottom));
    }

    #[test]
    fn gaps_share_what_the_line_limit_leaves() {
        let large = edit("line 500\n", "five hundred\n", false, Some(&file(1000))).unwrap();
        let (top, bottom) = (
            gap_at(&large, Edge::Top).unwrap(),
            gap_at(&large, Edge::Bottom).unwrap(),
        );
        assert_eq!(
            large.lines.len() + top.tail.len() + bottom.head.len(),
            LINE_LIMIT
        );
        assert!(top.tail.len() > 150 && bottom.head.len() > 150);
        assert_eq!(
            (top.tail.last().unwrap().number, bottom.head[0].number),
            (Some(496), Some(504))
        );
        assert!(top.head.is_empty() && bottom.tail.is_empty());

        // A long gap between changes keeps lines from both of its sides.
        let mut rows: Vec<String> = (1..=2000).map(|i| format!("row {i}")).collect();
        rows[9] = "foo".into();
        rows[1989] = "foo".into();
        let apart = edit("foo", "bar", true, Some(&rows.join("\n"))).unwrap();
        let middle = gap_at(&apart, Edge::Middle).unwrap();
        assert!(middle.head.len() > 50 && middle.tail.len() > 50);
        assert_eq!(
            (middle.head[0].number, middle.tail.last().unwrap().number),
            (Some(14), Some(1986))
        );

        // Long kept lines are cut like any other.
        let wide: String = (1..=20)
            .map(|i| if i == 2 { "w".repeat(900) } else { format!("line {i}") } + "\n")
            .collect();
        let wide = edit("line 10\n", "ten\n", false, Some(&wide)).unwrap();
        assert_eq!(
            char_count(&gap_at(&wide, Edge::Top).unwrap().tail[1].text),
            TEXT_LIMIT + 1
        );

        // A diff cut by the limit never leaves an open gap last.
        let spread: String = (1..=2000)
            .map(|i| {
                if i % 10 == 0 {
                    format!("changed {i}\n")
                } else {
                    format!("line {i}\n")
                }
            })
            .collect();
        let cut_off = diffs_with(
            "Write",
            &obj(json!({"file_path": "C:\\c", "content": spread})),
            None,
            &|_| Some(file(2000)),
            &|_| true,
        );
        assert!(cut_off[0].hidden > 0);
        assert!(cut_off[0].lines.last().unwrap().gap.is_none());
        // Nothing left over: the gaps keep nothing.
        assert!(
            cut_off[0]
                .lines
                .iter()
                .filter_map(|l| l.gap.as_ref())
                .all(|g| g.head.is_empty() && g.tail.is_empty())
        );
    }

    #[test]
    fn gaps_serialize_for_the_card() {
        let diff = edit("line 10\n", "ten\n", false, Some(&file(20))).unwrap();
        let value = serde_json::to_value(&diff).unwrap();
        assert_eq!(value["lines"][0]["gap"]["edge"], "top");
        assert_eq!(value["lines"][0]["gap"]["count"], 6);
        assert_eq!(value["lines"][0]["gap"]["tail"][0]["number"], 1);
        assert!(value["lines"][1].get("gap").is_none());
        assert_eq!(
            value["lines"].as_array().unwrap().last().unwrap()["gap"]["edge"],
            "bottom"
        );
    }
}
