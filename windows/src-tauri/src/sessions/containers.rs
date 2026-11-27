// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Docker containers and devcontainers (issue #330; Containers.swift on the
//! Mac, docs/containers.md). The person picks running containers in
//! Settings; each picked one gets a link like an SSH host's
//! (sessions::remote), started with `docker exec -i` instead of `ssh`. A
//! devcontainer is named by its project folder, so a rebuilt one, with a new
//! ID and name, is still the one picked.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A picked container, as the prefs keep it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    /// A devcontainer's folder on this machine (`devcontainer.local_folder`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// Any other container's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The container Set up last wrote its hooks into, to tell a rebuilt one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_up: Option<String>,
}

impl PartialEq for Target {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

pub const PREFIX: &str = "container:";

impl Target {
    pub fn id(&self) -> String {
        match (&self.folder, &self.name) {
            (Some(folder), _) => format!("folder:{folder}"),
            (None, name) => format!("name:{}", name.as_deref().unwrap_or("")),
        }
    }

    /// The key its sessions and link go by, next to SSH aliases.
    pub fn alias(&self) -> String {
        format!("{PREFIX}{}", self.id())
    }

    pub fn title(&self) -> String {
        title_of(&self.alias())
    }

    pub fn is_valid(&self) -> bool {
        if let Some(folder) = &self.folder {
            return is_absolute(folder) && !folder.contains(['\n', '\r']) && folder.len() < 4096;
        }
        let Some(name) = &self.name else {
            return false;
        };
        !name.is_empty()
            && name.len() <= 255
            && !name.starts_with('-')
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    }

    pub fn matches(&self, info: &Info) -> bool {
        match (&self.folder, &self.name) {
            (Some(folder), _) => info
                .folder
                .as_deref()
                .is_some_and(|f| same_folder(f, folder)),
            (None, Some(name)) => &info.name == name,
            _ => false,
        }
    }
}

fn is_absolute(path: &str) -> bool {
    path.starts_with('/')
        || path.starts_with("\\\\")
        || (path.len() > 2
            && path.as_bytes()[1] == b':'
            && path.as_bytes()[0].is_ascii_alphabetic())
}

/// Windows folders compare without case or slash direction (Docker Desktop
/// writes `c:\Users\…` as VS Code gave it).
fn same_folder(a: &str, b: &str) -> bool {
    if cfg!(windows) {
        let norm = |s: &str| s.replace('/', "\\").trim_end_matches('\\').to_lowercase();
        norm(a) == norm(b)
    } else {
        a == b
    }
}

pub fn is_container(alias: &str) -> bool {
    alias.starts_with(PREFIX)
}

/// What a badge shows for a container's key: the devcontainer's project or
/// the container's name.
pub fn title_of(alias: &str) -> String {
    let id = alias.strip_prefix(PREFIX).unwrap_or(alias);
    if let Some(folder) = id.strip_prefix("folder:") {
        let last = folder
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("");
        return if last.is_empty() {
            folder.to_string()
        } else {
            last.to_string()
        };
    }
    id.strip_prefix("name:").unwrap_or(id).to_string()
}

/// The containers picked (`/sessions/containers`), while the switch
/// (`/sessions/containersEnabled`) is on; once each, valid ones only.
pub fn targets_from_prefs(value: &Value) -> Vec<Target> {
    let enabled = value
        .pointer("/sessions/containersEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !enabled {
        return Vec::new();
    }
    all_targets(value)
}

pub fn all_targets(value: &Value) -> Vec<Target> {
    let mut seen: Vec<Target> = Vec::new();
    for item in value
        .pointer("/sessions/containers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Ok(target) = serde_json::from_value::<Target>(item.clone())
            && target.is_valid()
            && !seen.contains(&target)
        {
            seen.push(target);
        }
    }
    seen
}

/// A running container, as `docker inspect` describes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub id: String,
    pub name: String,
    pub image: String,
    pub folder: Option<String>,
    /// The user VS Code runs as in a devcontainer (`remoteUser`).
    pub user: Option<String>,
    /// Where the project folder is inside.
    pub workspace: Option<String>,
}

impl Info {
    pub fn target(&self) -> Target {
        match &self.folder {
            Some(folder) => Target {
                folder: Some(folder.clone()),
                ..Default::default()
            },
            None => Target {
                name: Some(self.name.clone()),
                ..Default::default()
            },
        }
    }
}

/// `docker inspect` output; Docker's and Podman's share these fields.
pub fn parse(data: &[u8]) -> Vec<Info> {
    let Ok(Value::Array(list)) = serde_json::from_slice::<Value>(data) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|item| {
            let id = item.get("Id")?.as_str()?.to_string();
            if id.is_empty()
                || item.pointer("/State/Running").and_then(Value::as_bool) == Some(false)
            {
                return None;
            }
            let label = |key: &str| {
                item.pointer("/Config/Labels")
                    .and_then(|labels| labels.get(key))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            };
            let name = item
                .get("Name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim_start_matches('/')
                .to_string();
            let mut info = Info {
                name: if name.is_empty() {
                    id.chars().take(12).collect()
                } else {
                    name
                },
                id,
                image: item
                    .pointer("/Config/Image")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                folder: None,
                user: None,
                workspace: None,
            };
            if let Some(folder) = label("devcontainer.local_folder").filter(|f| !f.is_empty()) {
                info.user = remote_user(label("devcontainer.metadata").as_deref());
                info.workspace = item
                    .get("Mounts")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .find(|m| {
                        m.get("Source")
                            .and_then(Value::as_str)
                            .is_some_and(|s| same_folder(s, &folder))
                    })
                    .and_then(|m| m.get("Destination").and_then(Value::as_str))
                    .map(str::to_string);
                info.folder = Some(folder);
            }
            Some(info)
        })
        .collect()
}

/// The `remoteUser` the devcontainer's configuration gave, the last winning.
pub fn remote_user(metadata: Option<&str>) -> Option<String> {
    let Ok(Value::Array(entries)) = serde_json::from_str::<Value>(metadata?) else {
        return None;
    };
    entries
        .iter()
        .filter_map(|e| e.get("remoteUser").and_then(Value::as_str))
        .filter(|u| !u.is_empty() && !u.starts_with('-') && !u.contains(char::is_whitespace))
        .last()
        .map(str::to_string)
}

// MARK: The CLI

/// The Docker command line, or Podman's, which takes the same commands.
#[derive(Clone, Debug, PartialEq)]
pub struct Engine {
    pub binary: PathBuf,
}

impl Engine {
    pub fn name(&self) -> String {
        self.binary
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "docker".into())
    }

    /// The first CLI found: where Docker Desktop, Rancher Desktop and Podman
    /// put theirs, then PATH.
    pub fn find() -> Option<Engine> {
        candidates()
            .into_iter()
            .find(|p| p.is_file())
            .map(|binary| Engine { binary })
    }

    #[cfg_attr(not(windows), allow(unused_mut))]
    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.binary);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: no console flashes for a background command.
            command.creation_flags(0x0800_0000);
        }
        command
    }

    /// `docker exec -i [-u user] -- <container> sh -c <loader>`.
    pub fn exec_args(container: &str, user: Option<&str>) -> Vec<String> {
        let mut args = vec!["exec".to_string(), "-i".to_string()];
        if let Some(user) = user.filter(|u| !u.is_empty()) {
            args.push("-u".into());
            args.push(user.into());
        }
        args.extend(["--", container, "sh", "-c", LOADER].map(String::from));
        args
    }

    /// The running containers, or why they can't be listed.
    pub fn running(&self) -> Result<Vec<Info>, String> {
        let ids = self
            .output(&["ps", "-q", "--no-trunc"], Duration::from_secs(10))
            .ok_or_else(|| format!("{} didn’t answer", self.name()))?;
        if !ids.0 {
            return Err(reason(&ids.2));
        }
        let list: Vec<String> = String::from_utf8_lossy(&ids.1)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_hexdigit()))
            .map(str::to_string)
            .collect();
        if list.is_empty() {
            return Ok(Vec::new());
        }
        let mut args = vec!["inspect", "--type", "container"];
        args.extend(list.iter().map(String::as_str));
        let inspected = self
            .output(&args, Duration::from_secs(10))
            .ok_or_else(|| format!("{} didn’t answer", self.name()))?;
        if !inspected.0 {
            return Err(reason(&inspected.2));
        }
        Ok(parse(&inspected.1))
    }

    /// One short command: (succeeded, stdout, stderr); None when it can't
    /// start or takes too long.
    fn output(&self, args: &[&str], timeout: Duration) -> Option<(bool, Vec<u8>, Vec<u8>)> {
        let mut child = self
            .command()
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .ok()?;
        let mut stdout = child.stdout.take()?;
        let mut stderr = child.stderr.take()?;
        let out = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = std::io::Read::read_to_end(&mut stdout, &mut buf);
            buf
        });
        let err = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = std::io::Read::read_to_end(&mut stderr, &mut buf);
            buf
        });
        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(30))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
            }
        };
        Some((status.success(), out.join().ok()?, err.join().ok()?))
    }
}

/// The last line the CLI printed on standard error, for Settings.
pub fn reason(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .map(|l| l.chars().take(160).collect())
        .unwrap_or_else(|| "The container engine isn’t running".into())
}

/// `docker exec` starts no login shell, so `sh` reads the script straight away.
pub const LOADER: &str = r#"IFS= read -r s; eval "$(printf %s "$s" | base64 -d)""#;

fn candidates() -> Vec<PathBuf> {
    let mut list: Vec<PathBuf> = Vec::new();
    #[cfg(windows)]
    {
        let program_files =
            std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into());
        list.push(Path::new(&program_files).join(r"Docker\Docker\resources\bin\docker.exe"));
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            list.push(
                Path::new(&local)
                    .join(r"Programs\Rancher Desktop\resources\resources\win32\bin\docker.exe"),
            );
        }
        list.push(Path::new(&program_files).join(r"RedHat\Podman\podman.exe"));
        for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
            list.push(dir.join("docker.exe"));
        }
        for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
            list.push(dir.join("podman.exe"));
        }
    }
    #[cfg(not(windows))]
    {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        for dir in ["/usr/bin", "/usr/local/bin", "/opt/homebrew/bin"] {
            list.push(Path::new(dir).join("docker"));
        }
        list.push(home.join(".orbstack/bin/docker"));
        list.push(home.join(".docker/bin/docker"));
        list.push(home.join(".rd/bin/docker"));
        for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
            list.push(dir.join("docker"));
        }
        for dir in ["/usr/bin", "/usr/local/bin", "/opt/homebrew/bin"] {
            list.push(Path::new(dir).join("podman"));
        }
        for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
            list.push(dir.join("podman"));
        }
    }
    list
}

// MARK: Hooks and Open

/// A container's hooks carry their own marker, so a machine reached both
/// ways never takes one kind for the other.
pub fn marker(app: &str) -> String {
    format!("# openbar-container-hook {app}")
}

pub const ANY_MARKER: &str = "# openbar-container-hook ";

/// The hook the relay writes at each connection, next to Openbar's folder.
pub fn hook_path(home: &str, app: &str) -> String {
    let name = super::remote::socket_name(app);
    format!("{home}/.openbar/{}.hook", name.trim_end_matches(".sock"))
}

pub fn hook_command(home: &str, app: &str) -> String {
    format!(
        "sh '{}' 2>/dev/null; exit 0 {}",
        hook_path(home, app).replace('\'', "'\\''"),
        marker(app)
    )
}

/// The devcontainer's VS Code window: `vscode://vscode-remote/` with the
/// Dev Containers authority, which brings an open window forward.
pub fn editor_url(folder: &str, workspace: Option<&str>) -> String {
    let hex: String = folder.bytes().map(|b| format!("{b:02x}")).collect();
    let inside = workspace.map(str::to_string).unwrap_or_else(|| {
        format!(
            "/workspaces/{}",
            title_of(&format!("{PREFIX}folder:{folder}"))
        )
    });
    let inside = if inside.starts_with('/') {
        inside
    } else {
        format!("/{inside}")
    };
    let encoded: String = inside
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    format!("vscode://vscode-remote/dev-container+{hex}{encoded}")
}

/// `docker exec -it` into the container, attached to the pane's tmux; `-u`
/// as `docker exec` carries no UTF-8 locale.
pub fn attach_args(container: &Info, socket: &str, pane: &str) -> Vec<String> {
    let mut args = vec!["exec".to_string(), "-it".to_string()];
    if let Some(user) = &container.user {
        args.push("-u".into());
        args.push(user.clone());
    }
    args.extend(
        [
            container.id.as_str(),
            "tmux",
            "-u",
            "-S",
            socket,
            "attach-session",
            "-t",
            pane,
        ]
        .map(String::from),
    );
    args
}

/// Whether something opens `vscode://` here, so Open never ends in a
/// "pick an app" dialog.
pub fn has_editor() -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::System::Registry::{
            HKEY, HKEY_CLASSES_ROOT, KEY_READ, RegCloseKey, RegOpenKeyExW,
        };
        use windows::core::w;
        let mut key = HKEY::default();
        let ok = unsafe {
            RegOpenKeyExW(
                HKEY_CLASSES_ROOT,
                w!("vscode\\shell\\open\\command"),
                None,
                KEY_READ,
                &mut key,
            )
        }
        .is_ok();
        if ok {
            let _ = unsafe { RegCloseKey(key) };
        }
        ok
    }
    #[cfg(not(windows))]
    {
        Command::new("xdg-mime")
            .args(["query", "default", "x-scheme-handler/vscode"])
            .stderr(Stdio::null())
            .output()
            .ok()
            .is_some_and(|o| {
                o.status.success() && !String::from_utf8_lossy(&o.stdout).trim().is_empty()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn targets_name_containers_across_rebuilds() {
        let dev = Target {
            folder: Some("/home/me/shop".into()),
            ..Default::default()
        };
        let plain = Target {
            name: Some("agent-box".into()),
            ..Default::default()
        };
        assert_eq!(dev.alias(), "container:folder:/home/me/shop");
        assert_eq!(dev.title(), "shop");
        assert_eq!(plain.title(), "agent-box");
        assert_eq!(title_of(r"container:folder:C:\Users\me\shop"), "shop");
        assert!(is_container(&dev.alias()) && !is_container("openbar-linux"));
        assert!(
            !Target {
                name: Some("-it".into()),
                ..Default::default()
            }
            .is_valid()
        );
        assert!(
            !Target {
                name: Some("a b".into()),
                ..Default::default()
            }
            .is_valid()
        );
        assert!(
            !Target {
                folder: Some("rel/x".into()),
                ..Default::default()
            }
            .is_valid()
        );
        assert!(
            Target {
                folder: Some(r"C:\Users\me\x".into()),
                ..Default::default()
            }
            .is_valid()
        );
        assert!(!Target::default().is_valid());
        let rebuilt = Info {
            id: "new".into(),
            name: "musing_goldwasser".into(),
            image: "x".into(),
            folder: Some("/home/me/shop".into()),
            user: None,
            workspace: None,
        };
        assert!(dev.matches(&rebuilt) && !plain.matches(&rebuilt));
        let with_mark = Target {
            set_up: Some("1".into()),
            ..dev.clone()
        };
        assert_eq!(with_mark, dev);
    }

    #[test]
    fn prefs_list_containers_only_with_the_switch_on() {
        let list =
            json!([{"name": "box"}, {"folder": "/p"}, {"name": "box"}, {"name": "-x"}, "junk"]);
        let off = json!({"sessions": {"containers": list}});
        assert!(targets_from_prefs(&off).is_empty());
        let on = json!({"sessions": {"containersEnabled": true, "containers": list}});
        let ids: Vec<String> = targets_from_prefs(&on).iter().map(Target::id).collect();
        assert_eq!(ids, vec!["name:box", "folder:/p"]);
        let stored = serde_json::to_value(Target {
            folder: Some("/p".into()),
            set_up: Some("abc".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(stored, json!({"folder": "/p", "setUp": "abc"}));
    }

    #[test]
    fn inspect_output_of_docker_the_devcontainers_cli_and_podman() {
        let docker = br#"[{"Id":"e9f2","Name":"/musing_goldwasser","State":{"Running":true},
          "Config":{"Image":"img","Labels":{"devcontainer.local_folder":"/home/me/demo-app",
            "devcontainer.metadata":"[{\"remoteUser\":\"root\"},{\"remoteUser\":\"node\"}]"}},
          "Mounts":[{"Source":"/home/me/demo-app","Destination":"/workspaces/demo-app"}]},
          {"Id":"1077","Name":"/ob-plain","State":{"Running":true},"Config":{"Image":"debian","Labels":null}},
          {"Id":"dead","Name":"/stopped","State":{"Running":false},"Config":{}}]"#;
        let found = parse(docker);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].name, "musing_goldwasser");
        assert_eq!(found[0].folder.as_deref(), Some("/home/me/demo-app"));
        assert_eq!(found[0].user.as_deref(), Some("node"));
        assert_eq!(found[0].workspace.as_deref(), Some("/workspaces/demo-app"));
        assert_eq!(found[1].target().id(), "name:ob-plain");
        let podman = br#"[{"Id":"abc","Name":"box","State":{"Running":true},"Config":{"Image":"x","Labels":{}}}]"#;
        assert_eq!(parse(podman)[0].name, "box");
        assert!(parse(b"nope").is_empty());
        assert_eq!(remote_user(Some(r#"[{"remoteUser":"-x"}]"#)), None);
    }

    #[test]
    fn commands_hooks_and_open() {
        assert_eq!(
            Engine::exec_args("e9f2", Some("node")),
            ["exec", "-i", "-u", "node", "--", "e9f2", "sh", "-c", LOADER]
        );
        assert_eq!(
            Engine::exec_args("x", None),
            ["exec", "-i", "--", "x", "sh", "-c", LOADER]
        );
        assert_eq!(
            hook_command("/home/node", "com.openbar.island"),
            "sh '/home/node/.openbar/com.openbar.island.hook' 2>/dev/null; exit 0 # openbar-container-hook com.openbar.island"
        );
        assert_eq!(
            editor_url("/Users/me/a b", Some("/workspaces/a b")),
            "vscode://vscode-remote/dev-container+2f55736572732f6d652f612062/workspaces/a%20b"
        );
        assert!(editor_url("/home/me/shop", None).ends_with("/workspaces/shop"));
        let info = Info {
            id: "1077".into(),
            name: "ob-plain".into(),
            image: "x".into(),
            folder: None,
            user: Some("node".into()),
            workspace: None,
        };
        assert_eq!(
            attach_args(&info, "/tmp/tmux-1000/default", "%0"),
            [
                "exec",
                "-it",
                "-u",
                "node",
                "1077",
                "tmux",
                "-u",
                "-S",
                "/tmp/tmux-1000/default",
                "attach-session",
                "-t",
                "%0"
            ]
        );
        assert_eq!(
            reason(b"\nCannot connect to the Docker daemon\n"),
            "Cannot connect to the Docker daemon"
        );
    }
}
