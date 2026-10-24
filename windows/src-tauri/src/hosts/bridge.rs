// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The client side of the IDE bridge extension (windows/tools/ide-bridge):
//! every running IDE window registers its pipe and token under
//! `%LOCALAPPDATA%\Vorssaint\terminals`, and the window holding the session's
//! terminal answers ok; the others say "missing".

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::Duration;

use interprocess::local_socket::{GenericFilePath, Stream, ToFsName, prelude::*};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Registration {
    pipe: String,
    token: String,
    pid: u32,
}

#[derive(Debug)]
pub enum BridgeError {
    /// No IDE window has that terminal (or no bridge is installed).
    Missing,
}

fn folder() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
    Some(base.join("Vorssaint").join("terminals"))
}

fn registrations() -> Vec<Registration> {
    let Some(dir) = folder() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter(|e| e.metadata().is_ok_and(|m| m.len() <= 64 * 1024))
        .filter_map(|e| serde_json::from_slice::<Registration>(&std::fs::read(e.path()).ok()?).ok())
        // A window that closed without cleaning up leaves a file behind.
        .filter(|r| super::process::is_running(r.pid))
        .collect()
}

/// One request to one window, given up after `timeout` so a hung IDE never holds a reply.
fn ask(registration: &Registration, mut request: Value, timeout: Duration) -> Option<Value> {
    request["token"] = json!(registration.token);
    let pipe = registration.pipe.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(exchange(&pipe, &request));
    });
    rx.recv_timeout(timeout).ok().flatten()
}

/// A JSON line out, the reply line back.
fn exchange(pipe: &str, request: &Value) -> Option<Value> {
    let name = pipe.to_string().to_fs_name::<GenericFilePath>().ok()?;
    let mut stream = Stream::connect(name).ok()?;
    let mut line = serde_json::to_vec(request).ok()?;
    line.push(b'\n');
    stream.write_all(&line).ok()?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).ok()?;
    serde_json::from_str(&reply).ok()
}

fn first_ok(request: Value, timeout: Duration) -> Result<(), BridgeError> {
    for registration in registrations() {
        if let Some(reply) = ask(&registration, request.clone(), timeout)
            && reply["ok"] == json!(true)
        {
            return Ok(());
        }
    }
    Err(BridgeError::Missing)
}

pub fn send(shell_pids: &[u32], text: &str, images: &[String]) -> Result<(), BridgeError> {
    let request = if images.is_empty() {
        json!({ "command": "send", "shellPids": shell_pids, "text": text })
    } else {
        json!({ "command": "send-images", "shellPids": shell_pids, "text": text, "images": images })
    };
    let timeout = Duration::from_secs_f64(4.0 + 1.2 + 0.3 * images.len() as f64);
    first_ok(request, timeout)
}

pub fn focus(shell_pids: &[u32]) -> Result<(), BridgeError> {
    first_ok(
        json!({ "command": "focus", "shellPids": shell_pids }),
        Duration::from_secs(4),
    )
}

/// Whether any IDE window runs the bridge, for the settings row.
#[allow(dead_code)]
pub fn installed_anywhere() -> bool {
    !registrations().is_empty()
}
