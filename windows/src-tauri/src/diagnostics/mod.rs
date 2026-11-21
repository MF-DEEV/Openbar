// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Export diagnostics (#230): one zip with what it takes to understand a
//! bug, masked, saved where the person chooses. Nothing is sent anywhere.
//! The same file layout and masking as the Mac app, so one reader works
//! for every report.

pub mod hook_records;
pub mod journal;
pub mod log;
pub mod redact;
pub mod report;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, Runtime};

use crate::sessions::models::Provider;
use report::{Agent, Display, Hooks, Part};

#[cfg(windows)]
pub const PLATFORM: &str = "Windows";
#[cfg(target_os = "linux")]
pub const PLATFORM: &str = "Linux";
#[cfg(not(any(windows, target_os = "linux")))]
pub const PLATFORM: &str = "macOS";

/// Starts the log and the hook records, before anything else writes.
pub fn start<R: Runtime>(app: &AppHandle<R>) {
    if let Ok(folder) = app.path().app_log_dir() {
        log::init(folder);
    }
    if let Ok(folder) = app.path().app_config_dir() {
        hook_records::init(&folder);
        crate::hooks::page::init(&folder);
    }
    let os = os();
    log::info(
        "app",
        &format!(
            "Openbar {} started on {} {}",
            app.package_info().version,
            os.name,
            os.version
        ),
    );
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    /// The optional parts switched on; the others are always in.
    parts: Vec<String>,
    /// Which features are on, as the settings window reads its preferences.
    features: Vec<report::Feature>,
    language: String,
    channel: String,
    /// Where to save without asking; for tests.
    path: Option<String>,
}

/// Asks where to save, writes the zip and shows it in its folder. None when
/// the person closed the dialog.
#[tauri::command]
pub async fn diagnostics_export<R: Runtime>(
    app: AppHandle<R>,
    window: tauri::WebviewWindow<R>,
    request: ExportRequest,
) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || export(&app, &window, request))
        .await
        .map_err(|e| e.to_string())?
}

/// The system in one line, for a bug report's form: "Windows 11 Pro 24H2
/// (26100.4351)" or "Ubuntu 24.04 (wayland, GNOME)".
#[tauri::command]
pub fn diagnostics_system_line() -> String {
    let os = os();
    let detail: Vec<String> = [os.build, os.session, os.desktop]
        .into_iter()
        .flatten()
        .collect();
    let line = format!("{} {}", os.name, os.version).trim().to_string();
    if detail.is_empty() {
        line
    } else {
        format!("{line} ({})", detail.join(", "))
    }
}

fn export<R: Runtime>(
    app: &AppHandle<R>,
    window: &tauri::WebviewWindow<R>,
    request: ExportRequest,
) -> Result<Option<String>, String> {
    let path = match request.path.clone() {
        Some(path) => PathBuf::from(path),
        None => {
            use tauri_plugin_dialog::DialogExt;
            let folder = crate::hooks::installer::home_dir()
                .map(|h| h.join("Desktop"))
                .filter(|d| d.is_dir())
                .or_else(crate::hooks::installer::home_dir)
                .unwrap_or_default();
            let chosen = app
                .dialog()
                .file()
                .set_parent(window)
                .set_title(crate::locale::text(app, "exportDiagnostics"))
                .set_directory(&folder)
                .set_file_name(report::file_name(chrono::Local::now()))
                .add_filter("ZIP", &["zip"])
                .blocking_save_file();
            match chosen.and_then(|p| p.into_path().ok()) {
                Some(path) => path,
                None => return Ok(None),
            }
        }
    };
    let path = if path.extension().is_none() {
        path.with_extension("zip")
    } else {
        path
    };
    let parts: Vec<Part> = request
        .parts
        .iter()
        .filter_map(|p| Part::from_key(p))
        .collect();
    log::info(
        "diagnostics",
        &format!(
            "export started: {}",
            parts.iter().map(|p| p.key()).collect::<Vec<_>>().join(",")
        ),
    );
    let home = crate::hooks::installer::home_dir()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_default();
    let input = collect(app, &request, &parts);
    let files = report::files(&parts, &input, PLATFORM, chrono::Utc::now(), &home);
    match report::write_zip(&files, &path) {
        Ok(()) => log::info(
            "diagnostics",
            &format!("export saved ({} files)", files.len()),
        ),
        Err(error) => {
            log::error("diagnostics", &format!("export failed: {error}"));
            return Err(error);
        }
    }
    let shown = path.to_string_lossy().into_owned();
    if request.path.is_none() {
        crate::files::shell::reveal(vec![shown.clone()]);
    }
    Ok(Some(shown))
}

fn collect<R: Runtime>(
    app: &AppHandle<R>,
    request: &ExportRequest,
    parts: &[Part],
) -> report::Input {
    let home = crate::hooks::installer::home_dir().unwrap_or_default();
    let service = app
        .try_state::<crate::sessions::service::Shared>()
        .map(|s| s.inner().clone());
    let prefs = app
        .try_state::<crate::prefs::Prefs>()
        .map(|p| p.get())
        .unwrap_or(Value::Null);
    report::Input {
        system: system(app, request),
        features: request.features.clone(),
        agents: agents(&home, service.as_deref()),
        events: if parts.contains(&Part::Events) {
            journal::recent()
        } else {
            Vec::new()
        },
        hosts: if parts.contains(&Part::Hosts) {
            hosts(&home, service.as_deref(), &prefs)
        } else {
            report::Hosts::default()
        },
        logs: if parts.contains(&Part::Logs) {
            log::recent_files(log::EXPORT_DAYS, chrono::Utc::now())
                .into_iter()
                .filter_map(|path| {
                    let data = std::fs::read(&path).ok()?;
                    let name = path.file_name()?.to_string_lossy().into_owned();
                    Some((name, String::from_utf8_lossy(&data).into_owned()))
                })
                .collect()
        } else {
            Vec::new()
        },
    }
}

// MARK: System

fn system<R: Runtime>(app: &AppHandle<R>, request: &ExportRequest) -> report::System {
    let primary = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| (m.position().x, m.position().y));
    let displays = app
        .available_monitors()
        .unwrap_or_default()
        .into_iter()
        .map(|m| {
            let scale = m.scale_factor();
            Display {
                width: (m.size().width as f64 / scale).round() as i64,
                height: (m.size().height as f64 / scale).round() as i64,
                scale,
                main: primary == Some((m.position().x, m.position().y)),
                built_in: None,
                notch: None,
            }
        })
        .collect();
    report::System {
        app: report::App {
            name: "Openbar".into(),
            version: app.package_info().version.to_string(),
            build: None,
            commit: option_env!("OPENBAR_BUILD_COMMIT").map(str::to_owned),
            channel: request.channel.clone(),
            developer_build: cfg!(debug_assertions),
        },
        os: os(),
        cpu: format!(
            "{} ({})",
            cpu_name().unwrap_or_else(|| "unknown".into()),
            std::env::consts::ARCH
        ),
        model: model(),
        language: request.language.clone(),
        displays,
    }
}

#[cfg(windows)]
fn registry(path: &str, name: &str) -> Option<String> {
    use windows::Win32::System::Registry::{
        HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW,
    };
    use windows::core::HSTRING;
    let (key, value) = (HSTRING::from(path), HSTRING::from(name));
    let mut buffer = [0u16; 512];
    let mut size = (buffer.len() * 2) as u32;
    let text = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            &key,
            &value,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if text.is_ok() {
        let length = (size as usize / 2).saturating_sub(1).min(buffer.len());
        return Some(
            String::from_utf16_lossy(&buffer[..length])
                .trim()
                .to_string(),
        );
    }
    let mut number = 0u32;
    let mut size = 4u32;
    unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            &key,
            &value,
            RRF_RT_REG_DWORD,
            None,
            Some((&mut number as *mut u32).cast()),
            Some(&mut size),
        )
    }
    .ok()
    .ok()
    .map(|_| number.to_string())
}

#[cfg(windows)]
fn os() -> report::Os {
    const KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    let build = registry(KEY, "CurrentBuild");
    let release = registry(KEY, "DisplayVersion");
    // ProductName still says Windows 10 on 11; the build tells them apart.
    let eleven = build
        .as_deref()
        .and_then(|b| b.parse::<u32>().ok())
        .is_some_and(|b| b >= 22000);
    let name = registry(KEY, "ProductName")
        .map(|n| {
            if eleven {
                n.replacen("Windows 10", "Windows 11", 1)
            } else {
                n
            }
        })
        .unwrap_or_else(|| "Windows".into());
    report::Os {
        name,
        version: release.unwrap_or_default(),
        build: build.map(|b| match registry(KEY, "UBR") {
            Some(ubr) => format!("{b}.{ubr}"),
            None => b,
        }),
        ..Default::default()
    }
}

#[cfg(windows)]
fn cpu_name() -> Option<String> {
    registry(
        r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
        "ProcessorNameString",
    )
}

#[cfg(windows)]
fn model() -> Option<String> {
    registry(r"HARDWARE\DESCRIPTION\System\BIOS", "SystemProductName")
}

#[cfg(not(windows))]
fn os() -> report::Os {
    let release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let field = |key: &str| {
        release.lines().find_map(|line| {
            line.strip_prefix(key)
                .and_then(|rest| rest.strip_prefix('='))
                .map(|v| v.trim_matches('"').to_string())
        })
    };
    let env = |key: &str| std::env::var(key).ok().filter(|v| !v.is_empty());
    report::Os {
        name: field("NAME").unwrap_or_else(|| "Linux".into()),
        version: field("VERSION_ID")
            .or_else(|| field("VERSION"))
            .unwrap_or_default(),
        build: None,
        kernel: std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .ok()
            .map(|k| k.trim().to_string()),
        session: env("XDG_SESSION_TYPE"),
        desktop: env("XDG_CURRENT_DESKTOP"),
    }
}

#[cfg(not(windows))]
fn cpu_name() -> Option<String> {
    let info = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    info.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        matches!(key.trim(), "model name" | "Model" | "Hardware")
            .then(|| value.trim().to_string())
            .filter(|v| !v.is_empty())
    })
}

#[cfg(not(windows))]
fn model() -> Option<String> {
    std::fs::read_to_string("/sys/devices/virtual/dmi/id/product_name")
        .ok()
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
}

// MARK: Agents

const AGENTS: [Provider; 12] = [
    Provider::Claude,
    Provider::Codex,
    Provider::Gemini,
    Provider::Copilot,
    Provider::Opencode,
    Provider::Cursor,
    Provider::Pi,
    Provider::Vibe,
    Provider::Devin,
    Provider::Antigravity,
    Provider::Grok,
    Provider::Zcode,
];

fn agent_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "Claude Code",
        Provider::Codex => "Codex",
        Provider::Gemini => "Gemini CLI",
        Provider::Copilot => "GitHub Copilot CLI",
        Provider::Opencode => "OpenCode",
        Provider::Cursor => "Cursor",
        Provider::Pi => "Pi",
        Provider::Vibe => "Mistral Vibe",
        Provider::Devin => "Devin CLI",
        Provider::Antigravity => "Google Antigravity",
        Provider::Grok => "Grok CLI",
        Provider::Zcode => "ZCode",
        Provider::Custom => "Other tool",
    }
}

fn executable(provider: Provider) -> Option<PathBuf> {
    let name = match provider {
        Provider::Claude => Some("claude"),
        Provider::Codex => Some("codex"),
        other => crate::sessions::hook_agents::command_name(other),
    }?;
    crate::agent_usage::agent_executable(name)
}

fn agents(home: &Path, service: Option<&crate::sessions::service::Service>) -> Vec<Agent> {
    let records = hook_records::all();
    let found: Vec<(Provider, Option<PathBuf>)> =
        AGENTS.iter().map(|&p| (p, executable(p))).collect();
    let versions = probe_versions(&found);
    let exe = crate::hooks::current_program().ok();
    let endpoint = crate::hooks::transport::endpoint(crate::hooks::APP_ID);
    found
        .iter()
        .map(|(provider, path)| {
            let provider = *provider;
            let version = versions
                .iter()
                .find(|(p, _)| *p == provider)
                .and_then(|(_, v)| v.clone());
            match provider {
                Provider::Claude => Agent {
                    id: provider.key().into(),
                    name: agent_name(provider).into(),
                    installed: path.is_some() || home.join(".claude").is_dir(),
                    version,
                    status: None,
                    hooks: crate::hooks::diagnostics_hooks(&records),
                },
                Provider::Codex => Agent {
                    id: provider.key().into(),
                    name: agent_name(provider).into(),
                    installed: path.is_some()
                        || crate::sessions::codex::client::codex_home().is_some_and(|h| h.is_dir()),
                    version,
                    status: Some(
                        service
                            .map(crate::sessions::service::codex_status_name)
                            .unwrap_or("unchecked")
                            .into(),
                    ),
                    hooks: Vec::new(),
                },
                _ => {
                    use crate::sessions::hook_agents::installer;
                    let base = match &exe {
                        Some(exe) => {
                            installer::state(provider, home, exe, &endpoint, crate::hooks::APP_ID)
                        }
                        None => installer::State::Unreadable,
                    };
                    let state = hook_records::agent_state(
                        provider,
                        base,
                        home,
                        crate::hooks::APP_ID,
                        &records,
                    );
                    let file = hook_records::hook_file(provider, home, crate::hooks::APP_ID);
                    Agent {
                        id: provider.key().into(),
                        name: agent_name(provider).into(),
                        installed: installer::is_present(provider, home),
                        version,
                        status: None,
                        hooks: vec![Hooks {
                            account: None,
                            state: state.name().into(),
                            config_path: file.map(|(f, _)| f.to_string_lossy().into_owned()),
                        }],
                    }
                }
            }
        })
        .collect()
}

/// Runs each found command with `--version`, all at once, giving each
/// three seconds.
fn probe_versions(found: &[(Provider, Option<PathBuf>)]) -> Vec<(Provider, Option<String>)> {
    let handles: Vec<_> = found
        .iter()
        .map(|(provider, path)| {
            let provider = *provider;
            let path = path.clone();
            std::thread::spawn(move || {
                (
                    provider,
                    path.and_then(|p| version_of(&p, Duration::from_secs(3))),
                )
            })
        })
        .collect();
    handles.into_iter().filter_map(|h| h.join().ok()).collect()
}

fn version_of(path: &Path, timeout: Duration) -> Option<String> {
    let mut command = Command::new(path);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console flashes for the check.
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().ok()?;
    let mut out = child.stdout.take()?;
    let mut err = child.stderr.take()?;
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut text = String::new();
        let _ = out.read_to_string(&mut text);
        let mut more = String::new();
        let _ = err.read_to_string(&mut more);
        text + "\n" + &more
    });
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    // A program that left a child holding its output is read no further.
    let (done, wait) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = done.send(reader.join().unwrap_or_default());
    });
    let output = wait.recv_timeout(Duration::from_millis(500)).ok()?;
    report::version_in(&output)
}

// MARK: Hosts

/// Terminals and multiplexers looked for on this computer, by command.
#[cfg(windows)]
const TERMINALS: [(&str, &str); 5] = [
    ("wt", "Windows Terminal"),
    ("wezterm", "WezTerm"),
    ("alacritty", "Alacritty"),
    ("kitty", "kitty"),
    ("tmux", "tmux"),
];
#[cfg(not(windows))]
const TERMINALS: [(&str, &str); 14] = [
    ("gnome-terminal", "GNOME Terminal"),
    ("kgx", "Console"),
    ("konsole", "Konsole"),
    ("xfce4-terminal", "Xfce Terminal"),
    ("tilix", "Tilix"),
    ("kitty", "kitty"),
    ("wezterm", "WezTerm"),
    ("alacritty", "Alacritty"),
    ("ghostty", "Ghostty"),
    ("foot", "foot"),
    ("xterm", "XTerm"),
    ("tmux", "tmux"),
    ("zellij", "zellij"),
    ("screen", "screen"),
];

fn hosts(
    home: &Path,
    service: Option<&crate::sessions::service::Service>,
    prefs: &Value,
) -> report::Hosts {
    let terminals = TERMINALS
        .iter()
        .filter(|(command, _)| crate::agent_usage::agent_executable(command).is_some())
        .map(|(_, name)| name.to_string())
        .collect();
    let editors = crate::hosts::bridge_installer::editor_states()
        .into_iter()
        .map(|(name, bridge)| report::Editor { name, bridge })
        .collect();
    let mut in_use = std::collections::BTreeMap::new();
    for host in service.map(|s| s.host_names()).unwrap_or_default() {
        *in_use.entry(host).or_insert(0) += 1;
    }
    let claude_accounts = crate::sessions::accounts::current(home)
        .into_iter()
        .map(|a| {
            if a.hidden {
                format!("{} (hidden)", a.name)
            } else {
                a.name
            }
        })
        .collect();
    let codex_accounts = codex_profiles(prefs);
    let aliases = crate::sessions::remote::hosts_from_prefs(prefs);
    let remote_hosts = match service {
        Some(service) => service.remote_states(&aliases),
        None => aliases
            .iter()
            .map(|a| (a.clone(), "off".to_string()))
            .collect(),
    }
    .into_iter()
    .map(|(label, state)| report::Remote { label, state })
    .collect();
    report::Hosts {
        terminals,
        editors,
        in_use,
        claude_accounts,
        codex_accounts,
        remote_hosts,
    }
}

/// The Codex accounts' names, from the settings (a JSON list, or the text of one).
fn codex_profiles(prefs: &Value) -> Vec<String> {
    let raw = prefs.pointer("/island/agentsCodexProfiles");
    let list = match raw {
        Some(Value::String(text)) => serde_json::from_str::<Value>(text).unwrap_or(Value::Null),
        Some(value) => value.clone(),
        None => Value::Null,
    };
    list.as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|p| p.get("name").and_then(Value::as_str).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_profiles_from_either_form() {
        let text = serde_json::json!({"island": {"agentsCodexProfiles": "[{\"id\":\"a\",\"name\":\"Work\",\"directory\":\"/x\"}]"}});
        assert_eq!(codex_profiles(&text), ["Work"]);
        let list = serde_json::json!({"island": {"agentsCodexProfiles": [{"name": "Home"}]}});
        assert_eq!(codex_profiles(&list), ["Home"]);
        assert!(codex_profiles(&Value::Null).is_empty());
    }

    #[test]
    #[cfg(unix)]
    fn versions_come_from_the_command() {
        let dir = std::env::temp_dir().join(format!("openbar-version-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("agent");
        std::fs::write(&script, "#!/bin/sh\necho 'agent 1.2.3 (build)'\n").unwrap();
        let slow = dir.join("slow");
        std::fs::write(&slow, "#!/bin/sh\nsleep 5\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        for file in [&script, &slow] {
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(
            version_of(&script, Duration::from_secs(3)).as_deref(),
            Some("1.2.3")
        );
        let started = Instant::now();
        assert_eq!(version_of(&slow, Duration::from_millis(300)), None);
        assert!(started.elapsed() < Duration::from_secs(2));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
