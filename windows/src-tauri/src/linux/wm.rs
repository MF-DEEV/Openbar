// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Other apps' windows on Wayland (#252). Wayland gives a client nothing about
//! windows that are not its own, so the compositor is asked over its own
//! socket where it has one: sway's IPC (`$SWAYSOCK`, i3's protocol) and
//! Hyprland's (`$HYPRLAND_INSTANCE_SIGNATURE`), or through a KWin script on
//! KDE Plasma (`kwin.rs`). Each lists every window with the
//! pid of its program, whether it is focused or fullscreen and where it is,
//! and focus a window when asked, which is what X11's `_NET_ACTIVE_WINDOW`,
//! `_NET_WM_STATE` and `_NET_WM_PID` give the rest of the Linux code.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

/// A window as the compositor lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toplevel {
    /// sway's container id, or Hyprland's window address.
    pub id: u64,
    pub pid: u32,
    pub title: String,
    /// The Wayland app id, or the X11 class of an XWayland window.
    pub app_id: String,
    pub focused: bool,
    pub fullscreen: bool,
    /// Left, top, right, bottom, in the compositor's logical pixels.
    pub rect: [i32; 4],
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Ipc {
    Sway(PathBuf),
    Hyprland(PathBuf),
    /// KDE Plasma: a KWin script reports to us (`kwin.rs`).
    KWin,
}

/// Whether GTK runs on Wayland itself rather than through XWayland, from what
/// it was told before it started (`linux::before_start` sets `GDK_BACKEND`).
pub fn native_wayland() -> bool {
    let env = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    session_is_wayland(env("WAYLAND_DISPLAY"), env("GDK_BACKEND"))
}

fn session_is_wayland(display: Option<String>, backend: Option<String>) -> bool {
    display.is_some()
        && backend.is_none_or(|b| {
            b.split(',')
                .next()
                .is_some_and(|first| first.trim() != "x11")
        })
}

fn hyprland_socket(runtime: Option<&str>, signature: &str) -> Vec<PathBuf> {
    // Hyprland 0.40 moved its sockets from /tmp/hypr to the runtime folder.
    let mut places: Vec<PathBuf> = runtime
        .map(|dir| {
            PathBuf::from(dir)
                .join("hypr")
                .join(signature)
                .join(".socket.sock")
        })
        .into_iter()
        .collect();
    places.push(
        PathBuf::from("/tmp/hypr")
            .join(signature)
            .join(".socket.sock"),
    );
    places
}

fn ipc() -> Option<Ipc> {
    if !native_wayland() {
        return None;
    }
    let env = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    if let Some(sock) = env("SWAYSOCK") {
        return Some(Ipc::Sway(sock.into()));
    }
    if let Some(signature) = env("HYPRLAND_INSTANCE_SIGNATURE") {
        return hyprland_socket(env("XDG_RUNTIME_DIR").as_deref(), &signature)
            .into_iter()
            .find(|p| p.exists())
            .map(Ipc::Hyprland);
    }
    super::kwin::available().then_some(Ipc::KWin)
}

/// Whether the windows of other apps can be listed and focused here.
pub fn available() -> bool {
    ipc().is_some()
}

const TIMEOUT: Duration = Duration::from_secs(2);

fn connect(path: &PathBuf) -> Option<UnixStream> {
    let stream = UnixStream::connect(path).ok()?;
    stream.set_read_timeout(Some(TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(TIMEOUT)).ok()?;
    Some(stream)
}

const SWAY_RUN_COMMAND: u32 = 0;
const SWAY_GET_OUTPUTS: u32 = 3;
const SWAY_GET_TREE: u32 = 4;

/// One i3-ipc request and its answer.
fn sway(path: &PathBuf, kind: u32, payload: &str) -> Option<Value> {
    let mut stream = connect(path)?;
    let mut message = b"i3-ipc".to_vec();
    message.extend((payload.len() as u32).to_ne_bytes());
    message.extend(kind.to_ne_bytes());
    message.extend(payload.as_bytes());
    stream.write_all(&message).ok()?;
    let mut header = [0u8; 14];
    stream.read_exact(&mut header).ok()?;
    if &header[..6] != b"i3-ipc" {
        return None;
    }
    let length = u32::from_ne_bytes(header[6..10].try_into().ok()?) as usize;
    let mut body = vec![0u8; length];
    stream.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

/// One Hyprland request (`j/clients`, `dispatch ...`) and its answer.
fn hyprland(path: &PathBuf, request: &str) -> Option<String> {
    let mut stream = connect(path)?;
    stream.write_all(request.as_bytes()).ok()?;
    let mut answer = String::new();
    stream.read_to_string(&mut answer).ok()?;
    Some(answer)
}

/// The windows in sway's tree, floating ones included; the scratchpad is not on screen.
fn sway_toplevels(tree: &Value) -> Vec<Toplevel> {
    fn walk(node: &Value, out: &mut Vec<Toplevel>) {
        if node.get("name").and_then(Value::as_str) == Some("__i3_scratch") {
            return;
        }
        let kind = node.get("type").and_then(Value::as_str).unwrap_or("");
        let leaf = matches!(kind, "con" | "floating_con")
            && node
                .get("nodes")
                .and_then(Value::as_array)
                .is_none_or(Vec::is_empty);
        if leaf && let Some(pid) = node.get("pid").and_then(Value::as_u64) {
            let rect = node.get("rect");
            let n = |key: &str| {
                rect.and_then(|r| r.get(key))
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32
            };
            let (x, y) = (n("x"), n("y"));
            let app_id = node
                .get("app_id")
                .and_then(Value::as_str)
                .or_else(|| {
                    node.pointer("/window_properties/class")
                        .and_then(Value::as_str)
                })
                .unwrap_or("");
            out.push(Toplevel {
                id: node.get("id").and_then(Value::as_u64).unwrap_or(0),
                pid: pid as u32,
                title: node
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .into(),
                app_id: app_id.into(),
                focused: node.get("focused").and_then(Value::as_bool) == Some(true),
                fullscreen: node
                    .get("fullscreen_mode")
                    .and_then(Value::as_u64)
                    .is_some_and(|mode| mode > 0),
                rect: [x, y, x + n("width"), y + n("height")],
            });
        }
        for key in ["nodes", "floating_nodes"] {
            for child in node
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                walk(child, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(tree, &mut out);
    out
}

/// Hyprland's clients, least recently focused first (as X11's stacking list
/// runs from the bottom), with the one `active` names marked focused.
fn hyprland_toplevels(clients: &Value, active: Option<&str>) -> Vec<Toplevel> {
    let mut out: Vec<(i64, Toplevel)> = clients
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c.get("mapped").and_then(Value::as_bool) != Some(false))
        .filter_map(|c| {
            let address = c.get("address").and_then(Value::as_str)?;
            let id = u64::from_str_radix(address.trim_start_matches("0x"), 16).ok()?;
            let pair = |key: &str| -> [i32; 2] {
                let values = c.get(key).and_then(Value::as_array);
                let at = |i: usize| {
                    values
                        .and_then(|v| v.get(i))
                        .and_then(Value::as_i64)
                        .unwrap_or(0) as i32
                };
                [at(0), at(1)]
            };
            let ([x, y], [w, h]) = (pair("at"), pair("size"));
            // A bool before Hyprland 0.42, the fullscreen mode (0 for none) since.
            let fullscreen = match c.get("fullscreen") {
                Some(Value::Bool(on)) => *on,
                Some(Value::Number(mode)) => mode.as_u64().is_some_and(|m| m > 0),
                _ => false,
            };
            let history = c
                .get("focusHistoryID")
                .and_then(Value::as_i64)
                .unwrap_or(i64::MAX);
            Some((
                history,
                Toplevel {
                    id,
                    pid: u32::try_from(c.get("pid").and_then(Value::as_i64)?).ok()?,
                    title: c.get("title").and_then(Value::as_str).unwrap_or("").into(),
                    app_id: c.get("class").and_then(Value::as_str).unwrap_or("").into(),
                    focused: Some(address) == active,
                    fullscreen,
                    rect: [x, y, x + w, y + h],
                },
            ))
        })
        .collect();
    out.sort_by_key(|(history, _)| std::cmp::Reverse(*history));
    out.into_iter().map(|(_, t)| t).collect()
}

/// Every window, the focused one last where the compositor says which is
/// newer; None where no compositor can be asked.
pub fn toplevels() -> Option<Vec<Toplevel>> {
    match ipc()? {
        Ipc::Sway(path) => {
            let mut all = sway_toplevels(&sway(&path, SWAY_GET_TREE, "")?);
            // The focused window last, as the topmost of X11's stack.
            all.sort_by_key(|t| t.focused);
            Some(all)
        }
        Ipc::Hyprland(path) => {
            let clients: Value = serde_json::from_str(&hyprland(&path, "j/clients")?).ok()?;
            let active: Value =
                serde_json::from_str(&hyprland(&path, "j/activewindow")?).unwrap_or(Value::Null);
            let address = active.get("address").and_then(Value::as_str);
            Some(hyprland_toplevels(&clients, address))
        }
        Ipc::KWin => Some(super::kwin::report()?.windows),
    }
}

/// The window with the keyboard, if it is an app's and not a panel's.
pub fn active() -> Option<Toplevel> {
    toplevels()?.into_iter().find(|t| t.focused)
}

/// Asks the compositor to focus window `id`, as a taskbar does.
pub fn focus(id: u64) -> bool {
    match ipc() {
        Some(Ipc::Sway(path)) => sway(&path, SWAY_RUN_COMMAND, &format!("[con_id={id}] focus"))
            .and_then(|answer| answer.get(0)?.get("success")?.as_bool())
            .unwrap_or(false),
        Some(Ipc::Hyprland(path)) => {
            hyprland(&path, &format!("dispatch focuswindow address:0x{id:x}"))
                .is_some_and(|answer| answer.trim() == "ok")
        }
        Some(Ipc::KWin) => super::kwin::activate(id),
        None => false,
    }
}

/// Focuses `id` and waits until the compositor says it has the keyboard.
pub fn bring_forward(id: u64) -> bool {
    let in_front = || active().is_some_and(|t| t.id == id);
    if !in_front() && !focus(id) {
        return false;
    }
    let deadline = std::time::Instant::now() + Duration::from_millis(500);
    while !in_front() {
        if std::time::Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // The keyboard follows the window a moment after the compositor reports it.
    std::thread::sleep(Duration::from_millis(50));
    true
}

/// Focuses the window of the nearest program in `pids`, picked as on X11.
pub fn raise(pids: &[u32], cwd: &str) -> bool {
    let Some(all) = toplevels() else {
        return false;
    };
    // `x11::pick` names a window by an X id; here it is the place in the list.
    let candidates: Vec<super::x11::Candidate> = all
        .iter()
        .enumerate()
        .map(|(i, t)| super::x11::Candidate {
            window: i as u32,
            pid: t.pid,
            title: t.title.clone(),
        })
        .collect();
    super::x11::pick(&candidates, pids, cwd)
        .and_then(|i| all.get(i as usize))
        .is_some_and(|t| focus(t.id))
}

/// A display as the compositor lists it, in the physical pixels tao gives
/// displays (its logical place times its scale).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Output {
    pub index: usize,
    pub focused: bool,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

fn sway_outputs(outputs: &Value) -> Vec<Output> {
    outputs
        .as_array()
        .into_iter()
        .flatten()
        .filter(|o| o.get("active").and_then(Value::as_bool) != Some(false))
        .enumerate()
        .map(|(index, o)| {
            let scale = o.get("scale").and_then(Value::as_f64).unwrap_or(1.0);
            let n = |key: &str| {
                let logical = o.pointer(&format!("/rect/{key}")).and_then(Value::as_f64);
                (logical.unwrap_or(0.0) * scale).round() as i32
            };
            Output {
                index,
                focused: o.get("focused").and_then(Value::as_bool) == Some(true),
                x: n("x"),
                y: n("y"),
                width: n("width"),
                height: n("height"),
            }
        })
        .collect()
}

fn hyprland_outputs(monitors: &Value) -> Vec<Output> {
    monitors
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(index, m)| {
            let scale = m.get("scale").and_then(Value::as_f64).unwrap_or(1.0);
            let n = |key: &str| m.get(key).and_then(Value::as_f64).unwrap_or(0.0);
            Output {
                index,
                focused: m.get("focused").and_then(Value::as_bool) == Some(true),
                // The place is logical, the size is the mode's pixels.
                x: (n("x") * scale).round() as i32,
                y: (n("y") * scale).round() as i32,
                width: n("width") as i32,
                height: n("height") as i32,
            }
        })
        .collect()
}

/// The display with the keyboard's focus, the closest Wayland has to the one
/// under the pointer.
pub fn focused_output() -> Option<Output> {
    let outputs = match ipc()? {
        Ipc::Sway(path) => sway_outputs(&sway(&path, SWAY_GET_OUTPUTS, "")?),
        Ipc::Hyprland(path) => {
            hyprland_outputs(&serde_json::from_str(&hyprland(&path, "j/monitors")?).ok()?)
        }
        Ipc::KWin => super::kwin::report()?.screen.into_iter().collect(),
    };
    outputs
        .iter()
        .find(|o| o.focused)
        .or(outputs.first())
        .copied()
}

/// The program of the window with the keyboard.
pub fn active_pid() -> Option<u32> {
    active().map(|t| t.pid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn gtk_on_wayland_is_told_by_the_session_and_the_backend_it_was_given() {
        let s = |v: &str| Some(v.to_string());
        assert!(session_is_wayland(s("wayland-1"), None));
        assert!(session_is_wayland(s("wayland-1"), s("wayland,x11")));
        assert!(!session_is_wayland(s("wayland-1"), s("x11")));
        assert!(!session_is_wayland(s("wayland-1"), s("x11,wayland")));
        assert!(!session_is_wayland(None, None));
    }

    #[test]
    fn hyprland_is_looked_for_in_the_runtime_folder_then_in_tmp() {
        assert_eq!(
            hyprland_socket(Some("/run/user/1000"), "abc"),
            [
                PathBuf::from("/run/user/1000/hypr/abc/.socket.sock"),
                PathBuf::from("/tmp/hypr/abc/.socket.sock"),
            ]
        );
        assert_eq!(hyprland_socket(None, "abc").len(), 1);
    }

    #[test]
    fn sways_tree_gives_its_windows_with_pid_focus_fullscreen_and_place() {
        let tree = json!({
            "type": "root", "nodes": [
                {"type": "output", "name": "__i3", "nodes": [
                    {"type": "workspace", "name": "__i3_scratch", "nodes": [],
                     "floating_nodes": [{"type": "floating_con", "id": 9, "pid": 90, "nodes": []}]}
                ]},
                {"type": "output", "name": "HEADLESS-1", "nodes": [
                    {"type": "workspace", "name": "1", "nodes": [
                        {"type": "con", "id": 4, "pid": 40, "name": "~/work/app", "app_id": "foot",
                         "focused": true, "fullscreen_mode": 1,
                         "rect": {"x": 0, "y": 24, "width": 960, "height": 1056}, "nodes": []},
                        {"type": "con", "id": 5, "nodes": [
                            {"type": "con", "id": 6, "pid": 60, "name": "XTerm", "app_id": null,
                             "window_properties": {"class": "XTerm"}, "focused": false,
                             "fullscreen_mode": 0,
                             "rect": {"x": 960, "y": 24, "width": 960, "height": 1056}, "nodes": []}
                        ]}
                    ], "floating_nodes": []}
                ]}
            ]
        });
        let all = sway_toplevels(&tree);
        assert_eq!(all.len(), 2, "the scratchpad and the split are not windows");
        assert_eq!(
            all[0],
            Toplevel {
                id: 4,
                pid: 40,
                title: "~/work/app".into(),
                app_id: "foot".into(),
                focused: true,
                fullscreen: true,
                rect: [0, 24, 960, 1080],
            }
        );
        assert_eq!(all[1].app_id, "XTerm", "an XWayland window by its class");
        assert!(!all[1].focused && !all[1].fullscreen);
    }

    #[test]
    fn displays_are_in_physical_pixels_and_the_focused_one_is_marked() {
        let sway = json!([
            {"name": "eDP-1", "active": true, "focused": false, "scale": 2.0,
             "rect": {"x": 0, "y": 0, "width": 1280, "height": 800}},
            {"name": "off", "active": false, "rect": {"x": 0, "y": 0, "width": 1, "height": 1}},
            {"name": "DP-1", "active": true, "focused": true, "scale": 1.0,
             "rect": {"x": 1280, "y": 0, "width": 1920, "height": 1080}}
        ]);
        let all = sway_outputs(&sway);
        assert_eq!(all.len(), 2, "an output that is off is no display");
        assert_eq!((all[0].width, all[0].height), (2560, 1600));
        assert_eq!(
            all[1],
            Output {
                index: 1,
                focused: true,
                x: 1280,
                y: 0,
                width: 1920,
                height: 1080
            }
        );
        let hypr = json!([
            {"id": 0, "x": 0, "y": 0, "width": 2560, "height": 1600, "scale": 2.0, "focused": true}
        ]);
        let all = hyprland_outputs(&hypr);
        assert_eq!(
            (all[0].width, all[0].height, all[0].focused),
            (2560, 1600, true)
        );
    }

    #[test]
    fn hyprlands_clients_run_from_least_recently_focused_and_read_either_fullscreen() {
        let clients = json!([
            {"address": "0x55a1", "mapped": true, "at": [10, 40], "size": [800, 600],
             "pid": 11, "class": "kitty", "title": "a", "fullscreen": 0, "focusHistoryID": 0},
            {"address": "0x55a2", "mapped": true, "at": [0, 0], "size": [1920, 1080],
             "pid": 22, "class": "mpv", "title": "b", "fullscreen": true, "focusHistoryID": 1},
            {"address": "0x55a3", "mapped": false, "at": [0, 0], "size": [1, 1],
             "pid": 33, "class": "x", "title": "c", "fullscreen": 0, "focusHistoryID": 2},
            {"address": "0x55a4", "at": [0, 0], "size": [1, 1],
             "pid": 44, "class": "y", "title": "d", "fullscreen": 2, "focusHistoryID": 3}
        ]);
        let all = hyprland_toplevels(&clients, Some("0x55a1"));
        let pids: Vec<u32> = all.iter().map(|t| t.pid).collect();
        assert_eq!(pids, [44, 22, 11], "unmapped ones left out, focused last");
        assert!(all[0].fullscreen && all[1].fullscreen && !all[2].fullscreen);
        assert!(all[2].focused);
        assert_eq!(all[2].id, 0x55a1);
        assert_eq!(all[2].rect, [10, 40, 810, 640]);
        assert!(hyprland_toplevels(&json!({}), None).is_empty());
    }
}
