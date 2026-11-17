// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The SQLite calls the OpenCode reader and the downloads list use, on Linux,
//! where Windows has `winsqlite3.dll` linked in. libsqlite3 is looked up when
//! it is first needed and not linked: nothing is added to the build, and it is
//! there wherever the page can run, since WebKit uses it too.
//!
//! Each function has the signature of the C function of the same name, so the
//! readers written against `winsqlite3` use these unchanged. Without SQLite,
//! opening a database fails like any other file that cannot be opened.

use std::ffi::{CStr, c_char, c_int, c_void};
use std::sync::OnceLock;

struct Library {
    open_v2: unsafe extern "C" fn(*const c_char, *mut *mut c_void, c_int, *const c_char) -> c_int,
    close: unsafe extern "C" fn(*mut c_void) -> c_int,
    busy_timeout: unsafe extern "C" fn(*mut c_void, c_int) -> c_int,
    prepare_v2: unsafe extern "C" fn(
        *mut c_void,
        *const c_char,
        c_int,
        *mut *mut c_void,
        *mut *const c_char,
    ) -> c_int,
    step: unsafe extern "C" fn(*mut c_void) -> c_int,
    reset: unsafe extern "C" fn(*mut c_void) -> c_int,
    finalize: unsafe extern "C" fn(*mut c_void) -> c_int,
    bind_int64: unsafe extern "C" fn(*mut c_void, c_int, i64) -> c_int,
    bind_text: unsafe extern "C" fn(*mut c_void, c_int, *const c_char, c_int, isize) -> c_int,
    column_text: unsafe extern "C" fn(*mut c_void, c_int) -> *const c_char,
    column_int64: unsafe extern "C" fn(*mut c_void, c_int) -> i64,
    column_type: unsafe extern "C" fn(*mut c_void, c_int) -> c_int,
}

/// The symbol `name` of the library as the function type `F`.
///
/// # Safety
/// `F` is the `extern "C"` function pointer type SQLite documents for `name`.
unsafe fn function<F: Copy>(handle: *mut c_void, name: &CStr) -> Option<F> {
    assert_eq!(size_of::<F>(), size_of::<*mut c_void>());
    let found = unsafe { libc::dlsym(handle, name.as_ptr()) };
    // SAFETY: a function pointer is as large as the data pointer dlsym gives.
    (!found.is_null()).then(|| unsafe { std::mem::transmute_copy::<*mut c_void, F>(&found) })
}

fn load() -> Option<Library> {
    // SAFETY: the names are C strings, and the library is never closed.
    unsafe {
        let handle = [c"libsqlite3.so.0", c"libsqlite3.so"]
            .iter()
            .find_map(|name| {
                let handle = libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
                (!handle.is_null()).then_some(handle)
            })?;
        Some(Library {
            open_v2: function(handle, c"sqlite3_open_v2")?,
            close: function(handle, c"sqlite3_close")?,
            busy_timeout: function(handle, c"sqlite3_busy_timeout")?,
            prepare_v2: function(handle, c"sqlite3_prepare_v2")?,
            step: function(handle, c"sqlite3_step")?,
            reset: function(handle, c"sqlite3_reset")?,
            finalize: function(handle, c"sqlite3_finalize")?,
            bind_int64: function(handle, c"sqlite3_bind_int64")?,
            bind_text: function(handle, c"sqlite3_bind_text")?,
            column_text: function(handle, c"sqlite3_column_text")?,
            column_int64: function(handle, c"sqlite3_column_int64")?,
            column_type: function(handle, c"sqlite3_column_type")?,
        })
    }
}

fn library() -> Option<&'static Library> {
    static LIBRARY: OnceLock<Option<Library>> = OnceLock::new();
    LIBRARY.get_or_init(load).as_ref()
}

/// SQLITE_CANTOPEN and SQLITE_ERROR, what is answered where there is no SQLite.
const CANTOPEN: c_int = 14;
const ERROR: c_int = 1;

/// # Safety
/// As the C function: `path` and `vfs` are C strings or null, `db` is written.
pub unsafe fn sqlite3_open_v2(
    path: *const c_char,
    db: *mut *mut c_void,
    flags: c_int,
    vfs: *const c_char,
) -> c_int {
    match library() {
        Some(sqlite) => unsafe { (sqlite.open_v2)(path, db, flags, vfs) },
        None => {
            unsafe { *db = std::ptr::null_mut() };
            CANTOPEN
        }
    }
}

/// # Safety
/// As the C function: `db` came from `sqlite3_open_v2`.
pub unsafe fn sqlite3_close(db: *mut c_void) -> c_int {
    match library() {
        Some(sqlite) => unsafe { (sqlite.close)(db) },
        None => ERROR,
    }
}

/// # Safety
/// As the C function.
pub unsafe fn sqlite3_busy_timeout(db: *mut c_void, ms: c_int) -> c_int {
    match library() {
        Some(sqlite) => unsafe { (sqlite.busy_timeout)(db, ms) },
        None => ERROR,
    }
}

/// # Safety
/// As the C function: `sql` is a C string, `statement` is written.
pub unsafe fn sqlite3_prepare_v2(
    db: *mut c_void,
    sql: *const c_char,
    len: c_int,
    statement: *mut *mut c_void,
    tail: *mut *const c_char,
) -> c_int {
    match library() {
        Some(sqlite) => unsafe { (sqlite.prepare_v2)(db, sql, len, statement, tail) },
        None => {
            unsafe { *statement = std::ptr::null_mut() };
            ERROR
        }
    }
}

/// # Safety
/// As the C function.
pub unsafe fn sqlite3_step(statement: *mut c_void) -> c_int {
    match library() {
        Some(sqlite) => unsafe { (sqlite.step)(statement) },
        None => ERROR,
    }
}

/// # Safety
/// As the C function.
pub unsafe fn sqlite3_reset(statement: *mut c_void) -> c_int {
    match library() {
        Some(sqlite) => unsafe { (sqlite.reset)(statement) },
        None => ERROR,
    }
}

/// # Safety
/// As the C function.
pub unsafe fn sqlite3_finalize(statement: *mut c_void) -> c_int {
    match library() {
        Some(sqlite) => unsafe { (sqlite.finalize)(statement) },
        None => ERROR,
    }
}

/// # Safety
/// As the C function.
pub unsafe fn sqlite3_bind_int64(statement: *mut c_void, index: c_int, value: i64) -> c_int {
    match library() {
        Some(sqlite) => unsafe { (sqlite.bind_int64)(statement, index, value) },
        None => ERROR,
    }
}

/// # Safety
/// As the C function: `text` holds `len` bytes.
pub unsafe fn sqlite3_bind_text(
    statement: *mut c_void,
    index: c_int,
    text: *const c_char,
    len: c_int,
    destructor: isize,
) -> c_int {
    match library() {
        Some(sqlite) => unsafe { (sqlite.bind_text)(statement, index, text, len, destructor) },
        None => ERROR,
    }
}

/// # Safety
/// As the C function: the text is valid until the next step or reset.
pub unsafe fn sqlite3_column_text(statement: *mut c_void, column: c_int) -> *const c_char {
    match library() {
        Some(sqlite) => unsafe { (sqlite.column_text)(statement, column) },
        None => std::ptr::null(),
    }
}

/// # Safety
/// As the C function.
pub unsafe fn sqlite3_column_int64(statement: *mut c_void, column: c_int) -> i64 {
    match library() {
        Some(sqlite) => unsafe { (sqlite.column_int64)(statement, column) },
        None => 0,
    }
}

/// # Safety
/// As the C function.
pub unsafe fn sqlite3_column_type(statement: *mut c_void, column: c_int) -> c_int {
    match library() {
        Some(sqlite) => unsafe { (sqlite.column_type)(statement, column) },
        None => 5,
    }
}

/// Runs `statements` on the database at `path`, made if it is not there: the
/// readers' tests make the databases they read with it. None where there is no SQLite.
#[cfg(test)]
pub fn make<S: AsRef<str>>(path: &std::path::Path, statements: &[S]) -> Option<()> {
    const READWRITE_CREATE: c_int = 0x2 | 0x4;
    const DONE: c_int = 101;
    library()?;
    let path = std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap();
    let mut db = std::ptr::null_mut();
    unsafe {
        assert_eq!(
            sqlite3_open_v2(path.as_ptr(), &mut db, READWRITE_CREATE, std::ptr::null()),
            0
        );
        for text in statements {
            let text = text.as_ref();
            let sql = std::ffi::CString::new(text).unwrap();
            let mut statement = std::ptr::null_mut();
            assert_eq!(
                sqlite3_prepare_v2(db, sql.as_ptr(), -1, &mut statement, std::ptr::null_mut()),
                0,
                "{text}"
            );
            assert_eq!(sqlite3_step(statement), DONE, "{text}");
            sqlite3_finalize(statement);
        }
        sqlite3_close(db);
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    const ROW: c_int = 100;
    const DONE: c_int = 101;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("openbar-sqlite-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_database_is_made_written_read_back_and_asked_in_json() {
        let dir = temp("roundtrip");
        let file = dir.join("t.db");
        let Some(()) = make(
            &file,
            &[
                "CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT, size INTEGER, note TEXT)",
                "INSERT INTO t VALUES(1, 'first', 10, NULL)",
                "INSERT INTO t VALUES(2, 'zażółć', 20, 'x')",
            ],
        ) else {
            eprintln!("skipped: no libsqlite3");
            return;
        };
        let path = CString::new(file.to_string_lossy().as_bytes()).unwrap();
        let mut db = std::ptr::null_mut();
        unsafe {
            // Read only, as the readers open the databases of other programs.
            assert_eq!(
                sqlite3_open_v2(path.as_ptr(), &mut db, 0x1, std::ptr::null()),
                0
            );
            assert_eq!(sqlite3_busy_timeout(db, 50), 0);
            let sql = c"SELECT name, size, note, json_extract('{\"a\":[1,2]}', '$.a[1]') FROM t WHERE id >= ? ORDER BY id";
            let mut statement = std::ptr::null_mut();
            assert_eq!(
                sqlite3_prepare_v2(db, sql.as_ptr(), -1, &mut statement, std::ptr::null_mut()),
                0
            );
            sqlite3_bind_int64(statement, 1, 1);
            assert_eq!(sqlite3_step(statement), ROW);
            assert_eq!(
                CStr::from_ptr(sqlite3_column_text(statement, 0)).to_str(),
                Ok("first")
            );
            assert_eq!(sqlite3_column_int64(statement, 1), 10);
            assert_eq!(sqlite3_column_type(statement, 2), 5, "NULL");
            assert_eq!(
                sqlite3_column_int64(statement, 3),
                2,
                "the JSON functions are there"
            );
            assert_eq!(sqlite3_step(statement), ROW);
            assert_eq!(
                CStr::from_ptr(sqlite3_column_text(statement, 0)).to_str(),
                Ok("zażółć")
            );
            assert_eq!(sqlite3_step(statement), DONE);
            // Again from the start, for another id and a text one.
            sqlite3_reset(statement);
            sqlite3_bind_int64(statement, 1, 2);
            assert_eq!(sqlite3_step(statement), ROW);
            sqlite3_finalize(statement);

            let sql = c"SELECT id FROM t WHERE name = ?";
            let mut statement = std::ptr::null_mut();
            assert_eq!(
                sqlite3_prepare_v2(db, sql.as_ptr(), -1, &mut statement, std::ptr::null_mut()),
                0
            );
            let name = "zażółć";
            sqlite3_bind_text(statement, 1, name.as_ptr().cast(), name.len() as c_int, -1);
            assert_eq!(sqlite3_step(statement), ROW);
            assert_eq!(sqlite3_column_int64(statement, 0), 2);
            sqlite3_finalize(statement);
            sqlite3_close(db);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_that_is_no_database_or_is_not_there_does_not_open() {
        if library().is_none() {
            return;
        }
        let dir = temp("missing");
        for (name, content) in [
            ("missing.db", None),
            ("text.db", Some("not a database at all")),
        ] {
            let file = dir.join(name);
            if let Some(content) = content {
                std::fs::write(&file, content).unwrap();
            }
            let path = CString::new(file.to_string_lossy().as_bytes()).unwrap();
            let mut db = std::ptr::null_mut();
            unsafe {
                let opened = sqlite3_open_v2(path.as_ptr(), &mut db, 0x1, std::ptr::null());
                let sql = c"SELECT 1 FROM sqlite_master";
                let mut statement = std::ptr::null_mut();
                let prepared = if opened == 0 {
                    sqlite3_prepare_v2(db, sql.as_ptr(), -1, &mut statement, std::ptr::null_mut())
                } else {
                    opened
                };
                assert_ne!(prepared, 0, "{name}");
                sqlite3_finalize(statement);
                sqlite3_close(db);
            }
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
