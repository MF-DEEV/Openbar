// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! OpenCode's database, read only (AgentOpenCodeReader.swift; spec 03 §4).
//! Only the JSON paths the parser needs are extracted inside SQLite, so
//! prompt and reply text never leaves it. Windows' own `winsqlite3` is used,
//! as in downloads.rs, so no SQLite ships with the app.

use super::files::Cursor;
use super::model::Time;

/// Rows remembered to notice a revert, which hands their rowids to new rows.
const TAIL: usize = 64;
/// A reply still being written is looked at again for at most a day.
const ABANDONED: f64 = 86_400.0;

const VALUES: [(&str, &str); 12] = [("role", "$.role"), ("parentID", "$.parentID"), ("modelID", "$.modelID"), ("model_id", "$.model_id"), ("model", "$.model"), ("tokens", "$.tokens"), ("cost", "$.cost"), ("finish", "$.finish"), ("time", "$.time"), ("cwd", "$.path.cwd"), ("status", "$.status"), ("outcome", "$.outcome")];

fn field(column: &str, path: &str) -> String {
    format!("(CASE WHEN json_valid({column}) THEN json_extract({column}, '{path}') END)")
}

/// The SELECT handing over each message row (Mac: the same query).
fn select(version2: bool, has_parent: bool, has_parts: bool, floor: i64) -> String {
    let (messages, sessions) = if version2 { ("session_message", "session_v2") } else { ("message", "session") };
    let recent = if version2 && floor > 0 { format!("m.time_created >= {floor} AND ") } else { String::new() };
    let role = if version2 { "m.type".to_string() } else { field("m.data", "$.role") };
    let tool_calls = if version2 {
        format!("CASE WHEN {recent}{role} = 'assistant' AND {} NOT IN ('tool-calls', 'unknown') THEN EXISTS (SELECT 1 FROM json_each(CASE WHEN json_valid(m.data) THEN json_extract(m.data, '$.content') ELSE '[]' END) c WHERE json_extract(c.value, '$.type') = 'tool' AND coalesce(json_extract(c.value, '$.executed'), 0) = 0) ELSE 0 END", field("m.data", "$.finish"))
    } else if has_parts {
        format!(
            "CASE WHEN {} = 'assistant' AND {} NOT IN ('tool-calls', 'unknown') THEN EXISTS (SELECT 1 FROM part p WHERE p.message_id = m.id AND {} = 'tool' AND coalesce({}, 0) = 0 AND NOT (coalesce({}, '') = 'error' AND coalesce({}, 0) = 1)) ELSE 0 END",
            field("m.data", "$.role"),
            field("m.data", "$.finish"),
            field("p.data", "$.type"),
            field("p.data", "$.metadata.providerExecuted"),
            field("p.data", "$.state.status"),
            field("p.data", "$.state.metadata.interrupted")
        )
    } else {
        "0".into()
    };
    let auto_compaction = if !version2 && has_parts {
        format!("CASE WHEN {} = 'assistant' AND {} = 1 THEN EXISTS (SELECT 1 FROM part p WHERE p.message_id = {} AND {} = 'compaction' AND {} = 1) ELSE 0 END", field("m.data", "$.role"), field("m.data", "$.summary"), field("m.data", "$.parentID"), field("p.data", "$.type"), field("p.data", "$.auto"))
    } else {
        "0".into()
    };
    let writing = format!("({recent}(coalesce({role} = 'assistant' AND {} IS NULL, 0) OR coalesce({role} = 'compaction' AND {} = 'running', 0)))", field("m.data", "$.time.completed"), field("m.data", "$.status"));
    let parent = if has_parent { "s.parent_id" } else { "NULL" };
    let fields = VALUES.iter().map(|(_, p)| format!("json_extract(m.data, '{p}')")).collect::<Vec<_>>().join(", ");
    let size = if recent.is_empty() { "length(m.data)".to_string() } else { format!("CASE WHEN {recent}1 THEN length(m.data) END") };
    format!(
        "SELECT m.rowid, m.id, m.session_id, m.time_created, m.time_updated, {size}, s.directory, {parent}, CASE WHEN {recent}json_valid(m.data) THEN json_array({role}, {fields}) END, {tool_calls}, {auto_compaction}, {writing}, CASE WHEN {recent}json_valid(m.data) THEN json_type(m.data, '$.error') END FROM {messages} m JOIN {sessions} s ON m.session_id = s.id"
    )
}

/// The parser's row from the columns `select` hands back.
#[allow(clippy::too_many_arguments)]
pub fn row(id: &str, session: &str, created: i64, updated: i64, directory: &str, parent: &str, extracted: &str, tool_calls: bool, auto_compaction: bool, error: Option<&str>, version2: bool) -> Option<String> {
    let found: Vec<serde_json::Value> = serde_json::from_str(extracted).ok()?;
    if found.len() != VALUES.len() + 1 {
        return None;
    }
    let mut json = serde_json::Map::new();
    if !found[0].is_null() {
        json.insert("role".into(), found[0].clone());
    }
    for ((name, _), value) in VALUES.iter().zip(found.iter().skip(1)) {
        if !value.is_null() {
            json.insert((*name).into(), value.clone());
        }
    }
    if let Some(cwd) = json.remove("cwd") {
        json.insert("path".into(), serde_json::json!({"cwd": cwd}));
    }
    json.insert("id".into(), id.into());
    json.insert("session_id".into(), session.into());
    json.insert("parent_session_id".into(), parent.into());
    json.insert("directory".into(), directory.into());
    json.insert("time_created".into(), created.into());
    json.insert("time_updated".into(), updated.into());
    if tool_calls {
        json.insert("tool_calls".into(), true.into());
    }
    if auto_compaction {
        json.insert("auto_compaction".into(), true.into());
    }
    if error.is_some_and(|e| e != "null") {
        json.insert("error".into(), true.into());
    }
    // OpenCode 2 completes a step it cut short only as the next one starts:
    // without a finish, an error or usage it neither ends nor starts a turn.
    if version2 && json.get("role").and_then(|r| r.as_str()) == Some("assistant") && !json.contains_key("finish") && !json.contains_key("error") && !json.contains_key("tokens") && json.get("time").and_then(|t| t.get("completed")).is_some() {
        return None;
    }
    if version2 {
        json.insert("version".into(), 2.into());
    }
    Some(serde_json::Value::Object(json).to_string())
}

fn restart(cursor: &mut Cursor, announce: bool, line: &mut dyn FnMut(&mut Cursor, &[u8])) {
    cursor.offset = 0;
    cursor.opencode = Default::default();
    cursor.state = Default::default();
    if announce {
        line(cursor, br#"{"type":"reset"}"#);
    }
}

#[cfg(windows)]
pub fn read_appended(cursor: &mut Cursor, horizon: Time, now: Time, line: &mut dyn FnMut(&mut Cursor, &[u8])) {
    use sqlite::*;
    let path = std::path::PathBuf::from(&cursor.path);
    let Ok(file) = super::files::open_shared(&path) else { return };
    let identity = super::files::identity(&file);
    drop(file);
    if identity != cursor.identity {
        let replaced = cursor.identity != 0;
        cursor.identity = identity;
        restart(cursor, replaced, line);
    }
    cursor.modified = super::files::opencode_modified(&path).unwrap_or(cursor.modified);
    let Some(db) = Db::open(&cursor.path) else { return };
    // OpenCode 1 keeps a session_message table of its own since 1.15, so
    // only session_v2 tells OpenCode 2 apart.
    let version2 = db.exists("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'session_v2'");
    let (messages, sessions) = if version2 { ("session_message", "session_v2") } else { ("message", "session") };
    if !resumes(&db, cursor, messages) {
        restart(cursor, true, line);
    }
    let floor = (horizon.max(0.0) * 1000.0) as i64;
    if cursor.offset == 0 {
        let indexed = if version2 { first_indexed_row(&db, floor) } else { None };
        cursor.offset = indexed.unwrap_or_else(|| first_row(&db, messages, floor)).max(0) as u64;
    }
    let has_parent = db.exists(&format!("SELECT 1 FROM pragma_table_info('{sessions}') WHERE name = 'parent_id'"));
    let has_parts = db.exists("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'part'");
    let select = select(version2, has_parent, has_parts, if version2 { floor } else { 0 });

    // Replies still being written, before anything saved after them.
    let open: Vec<String> = cursor.opencode.open.keys().cloned().collect();
    if !open.is_empty() {
        let marks = vec!["?"; open.len()].join(", ");
        let Some(stmt) = db.prepare(&format!("{select} WHERE m.id IN ({marks}) ORDER BY m.rowid")) else { return };
        for (i, id) in open.iter().enumerate() {
            stmt.bind_text(i as i32 + 1, id);
        }
        let mut found = std::collections::HashSet::new();
        while stmt.step() {
            let Some(id) = stmt.text(1) else { continue };
            let Some(&(updated, length, changed)) = cursor.opencode.open.get(&id) else { continue };
            found.insert(id.clone());
            if stmt.int(4) == updated && stmt.int(5) == length {
                if now - changed > ABANDONED {
                    cursor.opencode.open.remove(&id);
                }
                continue;
            }
            hand_over(&stmt, cursor, now, version2, line);
        }
        cursor.opencode.open.retain(|id, _| found.contains(id));
    }

    let Some(stmt) = db.prepare(&format!("{select} WHERE m.rowid > ? ORDER BY m.rowid")) else { return };
    stmt.bind_int(1, cursor.offset as i64);
    while stmt.step() {
        let rowid = stmt.int(0);
        cursor.offset = rowid.max(0) as u64;
        if let Some(id) = stmt.text(1) {
            cursor.opencode.tail.push((rowid, id));
            if cursor.opencode.tail.len() > TAIL {
                cursor.opencode.tail.remove(0);
            }
        }
        hand_over(&stmt, cursor, now, version2, line);
    }
}

#[cfg(windows)]
fn hand_over(stmt: &sqlite::Statement, cursor: &mut Cursor, now: Time, version2: bool, line: &mut dyn FnMut(&mut Cursor, &[u8])) {
    let (Some(id), Some(session)) = (stmt.text(1), stmt.text(2)) else { return };
    if stmt.is_null(3) {
        return;
    }
    let created = stmt.int(3);
    let updated = if stmt.is_null(4) { created } else { stmt.int(4) };
    if stmt.int(11) != 0 {
        cursor.opencode.open.insert(id.clone(), (stmt.int(4), stmt.int(5), now));
    } else {
        cursor.opencode.open.remove(&id);
    }
    let Some(extracted) = stmt.text(8) else { return };
    let error = stmt.text(12);
    if let Some(text) = row(&id, &session, created, updated, &stmt.text(6).unwrap_or_default(), &stmt.text(7).unwrap_or_default(), &extracted, stmt.int(9) != 0, stmt.int(10) != 0, error.as_deref(), version2) {
        line(cursor, text.as_bytes());
    }
}

/// False when a revert replaced the rows read last.
#[cfg(windows)]
fn resumes(db: &sqlite::Db, cursor: &mut Cursor, table: &str) -> bool {
    if cursor.opencode.tail.is_empty() {
        return true;
    }
    let Some(stmt) = db.prepare(&format!("SELECT id FROM {table} WHERE rowid = ?")) else { return true };
    for position in (0..cursor.opencode.tail.len()).rev() {
        stmt.reset();
        let (rowid, ref id) = cursor.opencode.tail[position];
        stmt.bind_int(1, rowid);
        if stmt.step() && stmt.text(0).as_deref() == Some(id.as_str()) {
            cursor.offset = rowid as u64;
            cursor.opencode.tail.truncate(position + 1);
            return true;
        }
    }
    false
}

/// The last row older than the horizon, by bisection over rowids.
#[cfg(windows)]
fn first_row(db: &sqlite::Db, table: &str, floor: i64) -> i64 {
    if floor <= 0 {
        return 0;
    }
    let mut high = db.prepare(&format!("SELECT max(rowid) FROM {table}")).filter(|s| s.step()).map_or(0, |s| s.int(0));
    let Some(stmt) = db.prepare(&format!("SELECT rowid, time_created FROM {table} WHERE rowid >= ? ORDER BY rowid LIMIT 1")) else { return 0 };
    let mut low = 0;
    while low < high {
        let middle = low + (high - low + 1) / 2;
        stmt.reset();
        stmt.bind_int(1, middle);
        if !stmt.step() {
            high = middle - 1;
            continue;
        }
        if stmt.is_null(1) || stmt.int(1) < floor {
            low = stmt.int(0);
        } else {
            high = middle - 1;
        }
    }
    low
}

#[cfg(windows)]
fn first_indexed_row(db: &sqlite::Db, floor: i64) -> Option<i64> {
    if floor <= 0 {
        return Some(0);
    }
    let stmt = db.prepare("SELECT (SELECT min(rowid) FROM session_message INDEXED BY session_message_time_created_idx WHERE time_created >= ?), (SELECT max(rowid) FROM session_message)")?;
    stmt.bind_int(1, floor);
    if !stmt.step() {
        return None;
    }
    Some(if stmt.is_null(0) { stmt.int(1) } else { (stmt.int(0) - 1).max(0) })
}

#[cfg(not(windows))]
pub fn read_appended(_cursor: &mut Cursor, _horizon: Time, _now: Time, _line: &mut dyn FnMut(&mut Cursor, &[u8])) {
    let _ = restart;
}

/// The few calls into Windows' `winsqlite3.dll`.
#[cfg(windows)]
mod sqlite {
    use std::ffi::{CStr, CString, c_char, c_int, c_void};
    #[link(name = "winsqlite3")]
    unsafe extern "C" {
        fn sqlite3_open_v2(path: *const c_char, db: *mut *mut c_void, flags: c_int, vfs: *const c_char) -> c_int;
        fn sqlite3_close(db: *mut c_void) -> c_int;
        fn sqlite3_busy_timeout(db: *mut c_void, ms: c_int) -> c_int;
        fn sqlite3_prepare_v2(db: *mut c_void, sql: *const c_char, len: c_int, statement: *mut *mut c_void, tail: *mut *const c_char) -> c_int;
        fn sqlite3_step(statement: *mut c_void) -> c_int;
        fn sqlite3_reset(statement: *mut c_void) -> c_int;
        fn sqlite3_finalize(statement: *mut c_void) -> c_int;
        fn sqlite3_bind_int64(statement: *mut c_void, index: c_int, value: i64) -> c_int;
        fn sqlite3_bind_text(statement: *mut c_void, index: c_int, text: *const c_char, len: c_int, destructor: isize) -> c_int;
        fn sqlite3_column_text(statement: *mut c_void, column: c_int) -> *const c_char;
        fn sqlite3_column_int64(statement: *mut c_void, column: c_int) -> i64;
        fn sqlite3_column_type(statement: *mut c_void, column: c_int) -> c_int;
    }
    const READONLY: c_int = 0x1;
    const NOMUTEX: c_int = 0x8000;
    const ROW: c_int = 100;
    const NULL: c_int = 5;
    /// SQLITE_TRANSIENT: SQLite copies the text.
    const TRANSIENT: isize = -1;

    pub struct Db(*mut c_void);
    impl Drop for Db {
        fn drop(&mut self) {
            unsafe {
                sqlite3_close(self.0);
            }
        }
    }
    impl Db {
        pub fn open(path: &str) -> Option<Db> {
            let path = CString::new(path).ok()?;
            let mut pointer = std::ptr::null_mut();
            unsafe {
                let result = sqlite3_open_v2(path.as_ptr(), &mut pointer, READONLY | NOMUTEX, std::ptr::null());
                if pointer.is_null() {
                    return None;
                }
                let db = Db(pointer);
                if result != 0 {
                    return None;
                }
                sqlite3_busy_timeout(db.0, 2000);
                Some(db)
            }
        }
        pub fn prepare(&self, sql: &str) -> Option<Statement> {
            let sql = CString::new(sql).ok()?;
            let mut pointer = std::ptr::null_mut();
            unsafe {
                let result = sqlite3_prepare_v2(self.0, sql.as_ptr(), -1, &mut pointer, std::ptr::null_mut());
                if pointer.is_null() {
                    return None;
                }
                let stmt = Statement(pointer);
                (result == 0).then_some(stmt)
            }
        }
        pub fn exists(&self, sql: &str) -> bool {
            self.prepare(sql).is_some_and(|s| s.step())
        }
    }
    pub struct Statement(*mut c_void);
    impl Drop for Statement {
        fn drop(&mut self) {
            unsafe {
                sqlite3_finalize(self.0);
            }
        }
    }
    impl Statement {
        pub fn step(&self) -> bool {
            unsafe { sqlite3_step(self.0) == ROW }
        }
        pub fn reset(&self) {
            unsafe {
                sqlite3_reset(self.0);
            }
        }
        pub fn bind_int(&self, index: i32, value: i64) {
            unsafe {
                sqlite3_bind_int64(self.0, index, value);
            }
        }
        pub fn bind_text(&self, index: i32, text: &str) {
            let len = text.len() as c_int;
            unsafe {
                sqlite3_bind_text(self.0, index, text.as_ptr().cast(), len, TRANSIENT);
            }
        }
        pub fn int(&self, column: i32) -> i64 {
            unsafe { sqlite3_column_int64(self.0, column) }
        }
        pub fn is_null(&self, column: i32) -> bool {
            unsafe { sqlite3_column_type(self.0, column) == NULL }
        }
        pub fn text(&self, column: i32) -> Option<String> {
            unsafe {
                let p = sqlite3_column_text(self.0, column);
                (!p.is_null()).then(|| CStr::from_ptr(p).to_string_lossy().into_owned())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracted_values_become_the_parsers_row() {
        let extracted = r#"["assistant","assistant","msg_user","gpt-5.6",null,null,{"input":10,"output":2},0.01,"stop",{"created":1790000000000,"completed":1790000001000},"C:\\proj",null,null]"#;
        let text = row("msg_1", "ses_1", 1_790_000_000_000, 1_790_000_001_000, r"C:\proj", "", extracted, true, false, Some("null"), true).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["role"], "assistant");
        assert_eq!(json["path"]["cwd"], r"C:\proj");
        assert_eq!(json["tool_calls"], true);
        assert_eq!(json["version"], 2);
        assert!(json.get("error").is_none() && json.get("cwd").is_none());
        let cut_short = r#"["assistant",null,null,null,null,null,null,null,null,{"completed":1},null,null,null]"#;
        assert!(row("m", "s", 1, 1, "", "", cut_short, false, false, None, true).is_none());
        assert!(select(true, true, true, 1).contains("session_message m JOIN session_v2 s"));
        assert!(select(false, false, false, 0).contains("FROM message m JOIN session s"));
    }
}
