// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Installs the IDE bridge extension (windows/tools/ide-bridge) into the
//! VS Code-family editors on this PC (spec 02-sessions §6.2.2). The extension
//! travels inside the exe; each install zips it into a .vsix and hands that
//! to the editor's own CLI.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

const EXTENSION_ID: &str = "vorssaint.vorssaint-terminal-bridge";
const PACKAGE_JSON: &[u8] = include_bytes!("../../../tools/ide-bridge/extension/package.json");
const VSIX_FILES: [(&str, &[u8]); 4] = [
    (
        "[Content_Types].xml",
        include_bytes!("../../../tools/ide-bridge/[Content_Types].xml"),
    ),
    (
        "extension.vsixmanifest",
        include_bytes!("../../../tools/ide-bridge/extension.vsixmanifest"),
    ),
    ("extension/package.json", PACKAGE_JSON),
    (
        "extension/extension.js",
        include_bytes!("../../../tools/ide-bridge/extension/extension.js"),
    ),
];
const INSTALL_TIMEOUT: Duration = Duration::from_secs(120);
const UNINSTALL_TIMEOUT: Duration = Duration::from_secs(60);

/// One install at a time: Settings and the update at launch may overlap.
static WORK: Mutex<()> = Mutex::new(());

struct Editor {
    id: &'static str,
    name: &'static str,
    /// The install folder, under Programs or Program Files.
    folders: &'static [&'static str],
    clis: &'static [&'static str],
    /// Tells an editor's CLI from a fork's: Cursor also ships a `code.cmd`.
    exes: &'static [&'static str],
    /// Used when product.json can't name the data folder.
    data_folder: &'static str,
}

const EDITORS: [Editor; 5] = [
    Editor {
        id: "vscode",
        name: "VS Code",
        folders: &["Microsoft VS Code"],
        clis: &["code.cmd"],
        exes: &["Code.exe"],
        data_folder: ".vscode",
    },
    Editor {
        id: "vscode-insiders",
        name: "VS Code Insiders",
        folders: &["Microsoft VS Code Insiders"],
        clis: &["code-insiders.cmd"],
        exes: &["Code - Insiders.exe"],
        data_folder: ".vscode-insiders",
    },
    Editor {
        id: "cursor",
        name: "Cursor",
        folders: &["cursor"],
        clis: &["cursor.cmd"],
        exes: &["Cursor.exe"],
        data_folder: ".cursor",
    },
    Editor {
        id: "antigravity",
        name: "Antigravity",
        folders: &["Antigravity IDE", "Antigravity"],
        clis: &["antigravity-ide.cmd", "antigravity.cmd"],
        exes: &["Antigravity IDE.exe", "Antigravity.exe"],
        data_folder: ".antigravity-ide",
    },
    Editor {
        id: "windsurf",
        name: "Windsurf",
        folders: &["Windsurf"],
        clis: &["windsurf.cmd"],
        exes: &["Windsurf.exe"],
        data_folder: ".windsurf",
    },
];

/// Where to look, passed in so tests can build a PC in a temp folder.
struct Places {
    home: PathBuf,
    /// Folders holding per-user and per-machine installs.
    roots: Vec<PathBuf>,
    path: Vec<PathBuf>,
}

impl Places {
    fn of_this_pc() -> Option<Self> {
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)?;
        let mut roots = Vec::new();
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            roots.push(PathBuf::from(local).join("Programs"));
        }
        for variable in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(folder) = std::env::var_os(variable) {
                roots.push(PathBuf::from(folder));
            }
        }
        Some(Self {
            home,
            roots,
            path: crate::agent_usage::search_path(),
        })
    }
}

struct Found<'a> {
    editor: &'a Editor,
    cli: PathBuf,
    extensions: PathBuf,
}

/// The folder above `cli` holding the editor's exe: `bin\code.cmd` in VS Code,
/// `resources\app\bin\cursor.cmd` in Cursor.
fn install_root(cli: &Path, editor: &Editor) -> Option<PathBuf> {
    cli.ancestors()
        .skip(1)
        .take(5)
        .find(|folder| editor.exes.iter().any(|exe| folder.join(exe).is_file()))
        .map(Path::to_path_buf)
}

/// product.json sits in the install folder, or since VS Code 1.144 in a folder
/// named after the build inside it.
fn product_files(root: &Path) -> Vec<PathBuf> {
    let inside = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path());
    std::iter::once(root.to_path_buf())
        .chain(inside)
        .map(|folder| folder.join("resources").join("app").join("product.json"))
        .filter(|file| file.is_file())
        .collect()
}

/// `~/<dataFolderName>/extensions`, which is how the editors themselves work it out.
fn extensions_folder(home: &Path, root: &Path, editor: &Editor) -> PathBuf {
    let named = product_files(root)
        .iter()
        .filter_map(|file| std::fs::read(file).ok())
        .filter_map(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .find_map(|product| product["dataFolderName"].as_str().map(str::to_owned))
        // Only ever a folder directly under the home folder.
        .filter(|name| {
            let mut parts = Path::new(name).components();
            matches!(parts.next(), Some(std::path::Component::Normal(_))) && parts.next().is_none()
        });
    home.join(named.as_deref().unwrap_or(editor.data_folder))
        .join("extensions")
}

fn find<'a>(editor: &'a Editor, places: &Places) -> Option<Found<'a>> {
    let installed = places.roots.iter().flat_map(|root| {
        editor.folders.iter().flat_map(move |folder| {
            editor.clis.iter().flat_map(move |cli| {
                let base = root.join(folder);
                [
                    base.join("bin").join(cli),
                    base.join("resources").join("app").join("bin").join(cli),
                ]
            })
        })
    });
    let on_path = places
        .path
        .iter()
        .filter(|dir| dir.is_absolute())
        .flat_map(|dir| editor.clis.iter().map(move |cli| dir.join(cli)));
    installed
        .chain(on_path)
        .filter(|cli| cli.is_file())
        .find_map(|cli| {
            let root = install_root(&cli, editor)?;
            Some(Found {
                editor,
                extensions: extensions_folder(&places.home, &root, editor),
                cli,
            })
        })
}

/// The versions an editor lists as installed. `extensions.json` is its own
/// record: a removed extension's folder can stay until it restarts.
fn listed_versions(extensions: &Path) -> Option<Vec<String>> {
    let bytes = std::fs::read(extensions.join("extensions.json")).ok()?;
    let list: Value = serde_json::from_slice(&bytes).ok()?;
    let versions = list
        .as_array()?
        .iter()
        .filter(|entry| {
            entry["identifier"]["id"]
                .as_str()
                .is_some_and(|id| id.eq_ignore_ascii_case(EXTENSION_ID))
        })
        .filter_map(|entry| entry["version"].as_str().map(str::to_owned))
        .collect();
    Some(versions)
}

/// Without that record, the folders, less those listed in `.obsolete`.
fn folder_versions(extensions: &Path) -> Vec<String> {
    let obsolete: Value = std::fs::read(extensions.join(".obsolete"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(Value::Null);
    let prefix = format!("{EXTENSION_ID}-");
    let Ok(entries) = std::fs::read_dir(extensions) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| obsolete.get(name).is_none())
        .filter_map(|name| {
            let version = name.to_ascii_lowercase().strip_prefix(&prefix)?.to_owned();
            Some(version)
        })
        .collect()
}

fn installed_versions(extensions: &Path) -> Vec<String> {
    listed_versions(extensions).unwrap_or_else(|| folder_versions(extensions))
}

/// The version this build carries, from the extension's own manifest.
fn bundled_version() -> Option<String> {
    let manifest: Value = serde_json::from_slice(PACKAGE_JSON).ok()?;
    manifest["version"].as_str().map(str::to_owned)
}

/// A folder may carry a platform after the version: `1.1.0-universal`.
fn is_version(installed: &str, bundled: &str) -> bool {
    installed == bundled || installed.starts_with(&format!("{bundled}-"))
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    Missing,
    Outdated,
    Current,
}

fn state_of(extensions: &Path, bundled: &str) -> State {
    let versions = installed_versions(extensions);
    if versions.is_empty() {
        State::Missing
    } else if versions.iter().any(|v| is_version(v, bundled)) {
        State::Current
    } else {
        State::Outdated
    }
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorStatus {
    id: &'static str,
    name: &'static str,
    state: State,
}

fn statuses(places: &Places, bundled: &str) -> Vec<EditorStatus> {
    EDITORS
        .iter()
        .filter_map(|editor| find(editor, places))
        .map(|found| EditorStatus {
            id: found.editor.id,
            name: found.editor.name,
            state: state_of(&found.extensions, bundled),
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    NotFound,
    Packaging,
    Refused,
    TimedOut,
    Other(String),
}

impl Failure {
    /// What Settings gets: a word it has a sentence for, or the system's own message.
    fn into_reason(self) -> String {
        match self {
            Failure::NotFound => "notFound".into(),
            Failure::Packaging => "packaging".into(),
            Failure::Refused => "refused".into(),
            Failure::TimedOut => "timedOut".into(),
            Failure::Other(reason) => reason,
        }
    }
}

fn write_vsix(path: &Path) -> Result<(), Failure> {
    let write = || -> Result<(), Box<dyn std::error::Error>> {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path)?);
        for (name, bytes) in VSIX_FILES {
            zip.start_file(name, zip::write::SimpleFileOptions::default())?;
            zip.write_all(bytes)?;
        }
        zip.finish()?.sync_all()?;
        Ok(())
    };
    write().map_err(|_| Failure::Packaging)
}

/// A folder of our own in temp, gone when the install is over.
struct WorkFolder(PathBuf);

impl WorkFolder {
    fn new() -> Result<Self, Failure> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.subsec_nanos());
        let folder =
            std::env::temp_dir().join(format!("vorssaint-bridge-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&folder).map_err(|_| Failure::Packaging)?;
        Ok(Self(folder))
    }
}

impl Drop for WorkFolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs an editor's CLI hidden and gives up after `timeout`, so a CLI that
/// hangs never holds the button. Output goes nowhere: a pipe nobody reads
/// could block it, and a grandchild could keep one open past the exit.
fn run_cli(
    cli: &Path,
    args: &[&std::ffi::OsStr],
    folder: &Path,
    timeout: Duration,
) -> Result<(), Failure> {
    let mut command = Command::new(cli);
    command
        .args(args)
        .current_dir(folder)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console flashes for the .cmd.
        command.creation_flags(0x0800_0000);
    }
    let mut child = command
        .spawn()
        .map_err(|error| Failure::Other(error.to_string()))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => return Err(Failure::Refused),
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Ok(None) => {
                end_tree(&mut child);
                return Err(Failure::TimedOut);
            }
            Err(error) => {
                end_tree(&mut child);
                return Err(Failure::Other(error.to_string()));
            }
        }
    }
}

fn end_tree(child: &mut std::process::Child) {
    // A .cmd shim's editor process outlives cmd.exe; the whole tree goes.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn install_found(found: &Found, bundled: &str, timeout: Duration) -> Result<(), Failure> {
    let work = WorkFolder::new()?;
    let vsix = work.0.join("vorssaint-terminal-bridge.vsix");
    write_vsix(&vsix)?;
    run_cli(
        &found.cli,
        &[
            "--install-extension".as_ref(),
            vsix.as_os_str(),
            "--force".as_ref(),
        ],
        &work.0,
        timeout,
    )?;
    // Some CLIs exit 0 after only printing what went wrong.
    if state_of(&found.extensions, bundled) == State::Current {
        Ok(())
    } else {
        Err(Failure::Refused)
    }
}

fn install(id: &str, places: &Places, bundled: &str) -> Result<(), Failure> {
    let _one_at_a_time = WORK.lock().unwrap_or_else(|poison| poison.into_inner());
    let found = EDITORS
        .iter()
        .find(|editor| editor.id == id)
        .and_then(|editor| find(editor, places))
        .ok_or(Failure::NotFound)?;
    install_found(&found, bundled, INSTALL_TIMEOUT)
}

/// An extension the person once added is kept current by the app, so a newer
/// one never waits on a click in Settings.
pub fn update_outdated() {
    let (Some(places), Some(bundled)) = (Places::of_this_pc(), bundled_version()) else {
        return;
    };
    let _one_at_a_time = WORK.lock().unwrap_or_else(|poison| poison.into_inner());
    for found in EDITORS.iter().filter_map(|editor| find(editor, &places)) {
        if state_of(&found.extensions, &bundled) == State::Outdated {
            let _ = install_found(&found, &bundled, INSTALL_TIMEOUT);
        }
    }
}

/// What the uninstaller takes away: the extension in every editor that has
/// it, and the folder its windows register in.
pub fn remove_everywhere() {
    let Some(places) = Places::of_this_pc() else {
        return;
    };
    for found in EDITORS.iter().filter_map(|editor| find(editor, &places)) {
        if !installed_versions(&found.extensions).is_empty() {
            let _ = run_cli(
                &found.cli,
                &["--uninstall-extension".as_ref(), EXTENSION_ID.as_ref()],
                &places.home,
                UNINSTALL_TIMEOUT,
            );
        }
    }
    if let Some(folder) = super::bridge::folder() {
        let _ = std::fs::remove_dir_all(folder);
    }
}

#[tauri::command]
pub async fn ide_bridge_status() -> Vec<EditorStatus> {
    tauri::async_runtime::spawn_blocking(|| {
        let (Some(places), Some(bundled)) = (Places::of_this_pc(), bundled_version()) else {
            return Vec::new();
        };
        statuses(&places, &bundled)
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
pub async fn ide_bridge_install(id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let places = Places::of_this_pc().ok_or(Failure::NotFound)?;
        let bundled = bundled_version().ok_or(Failure::Packaging)?;
        install(&id, &places, &bundled)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(Failure::into_reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Read;

    /// A throwaway PC with a home folder and one Programs folder.
    struct Pc {
        base: PathBuf,
    }

    impl Pc {
        fn new(name: &str) -> Self {
            let base = std::env::temp_dir().join(format!(
                "vorssaint-bridge-test-{name}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&base);
            fs::create_dir_all(base.join("home")).unwrap();
            fs::create_dir_all(base.join("Programs")).unwrap();
            Self { base }
        }

        fn places(&self) -> Places {
            Places {
                home: self.base.join("home"),
                roots: vec![self.base.join("Programs")],
                path: Vec::new(),
            }
        }

        /// Files for an editor at `Programs/<folder>`; returns the install root.
        fn editor(&self, folder: &str, exe: &str, cli: &str) -> PathBuf {
            let root = self.base.join("Programs").join(folder);
            touch(&root.join(exe));
            touch(&root.join(cli));
            root
        }

        fn extension(&self, data_folder: &str, folder: &str) -> PathBuf {
            let path = self
                .base
                .join("home")
                .join(data_folder)
                .join("extensions")
                .join(folder);
            fs::create_dir_all(&path).unwrap();
            path
        }
    }

    impl Drop for Pc {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "").unwrap();
    }

    fn editor(id: &str) -> &'static Editor {
        EDITORS.iter().find(|e| e.id == id).unwrap()
    }

    #[test]
    fn manifests_agree_on_the_version() {
        let bundled = bundled_version().unwrap();
        let manifest = std::str::from_utf8(VSIX_FILES[1].1).unwrap();
        assert!(
            manifest.contains(&format!("Version=\"{bundled}\" Publisher=\"vorssaint\"")),
            "extension.vsixmanifest and package.json name different versions"
        );
    }

    #[test]
    fn the_vsix_has_the_layout_editors_read() {
        let pc = Pc::new("vsix");
        let path = pc.base.join("x.vsix");
        write_vsix(&path).unwrap();
        let mut archive = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
        let mut names: Vec<String> = archive.file_names().map(str::to_owned).collect();
        names.sort();
        assert_eq!(
            names,
            [
                "[Content_Types].xml",
                "extension.vsixmanifest",
                "extension/extension.js",
                "extension/package.json",
            ]
        );
        let mut package = String::new();
        archive
            .by_name("extension/package.json")
            .unwrap()
            .read_to_string(&mut package)
            .unwrap();
        assert!(package.contains("vorssaint-terminal-bridge"));
    }

    #[test]
    fn nothing_listed_when_no_editor_is_here() {
        let pc = Pc::new("empty");
        // Data left behind by an editor since removed isn't an editor.
        pc.extension(".vscode", "vorssaint.vorssaint-terminal-bridge-1.0.0");
        assert!(statuses(&pc.places(), "1.1.0").is_empty());
    }

    #[test]
    fn finds_each_layout_and_its_data_folder() {
        let pc = Pc::new("layouts");
        pc.editor("Microsoft VS Code", "Code.exe", "bin/code.cmd");
        pc.editor("cursor", "Cursor.exe", "resources/app/bin/cursor.cmd");
        let ag = pc.editor(
            "Antigravity IDE",
            "Antigravity IDE.exe",
            "bin/antigravity-ide.cmd",
        );
        fs::create_dir_all(ag.join("resources/app")).unwrap();
        fs::write(
            ag.join("resources/app/product.json"),
            r#"{"dataFolderName":".antigravity-custom"}"#,
        )
        .unwrap();
        let places = pc.places();
        let code = find(editor("vscode"), &places).unwrap();
        assert_eq!(code.extensions, places.home.join(".vscode/extensions"));
        let cursor = find(editor("cursor"), &places).unwrap();
        assert_eq!(cursor.extensions, places.home.join(".cursor/extensions"));
        let antigravity = find(editor("antigravity"), &places).unwrap();
        assert_eq!(
            antigravity.extensions,
            places.home.join(".antigravity-custom/extensions")
        );
        assert!(find(editor("windsurf"), &places).is_none());
    }

    #[test]
    fn reads_the_data_folder_from_a_build_folder() {
        let pc = Pc::new("build-folder");
        let root = pc.editor("Windsurf", "Windsurf.exe", "bin/windsurf.cmd");
        fs::create_dir_all(root.join("2c33b50b16/resources/app")).unwrap();
        fs::write(
            root.join("2c33b50b16/resources/app/product.json"),
            r#"{"dataFolderName":".windsurf-next"}"#,
        )
        .unwrap();
        let places = pc.places();
        let found = find(editor("windsurf"), &places).unwrap();
        assert_eq!(
            found.extensions,
            places.home.join(".windsurf-next/extensions")
        );
    }

    #[test]
    fn a_data_folder_cannot_leave_the_home_folder() {
        let pc = Pc::new("escape");
        for name in ["../elsewhere", "a/b", "/abs", ""] {
            let root = pc.editor("Windsurf", "Windsurf.exe", "bin/windsurf.cmd");
            fs::create_dir_all(root.join("resources/app")).unwrap();
            fs::write(
                root.join("resources/app/product.json"),
                serde_json::json!({ "dataFolderName": name }).to_string(),
            )
            .unwrap();
            let places = pc.places();
            let found = find(editor("windsurf"), &places).unwrap();
            assert_eq!(found.extensions, places.home.join(".windsurf/extensions"));
        }
    }

    #[test]
    fn a_forks_code_cmd_on_path_is_not_vs_code() {
        let pc = Pc::new("fork");
        let cursor = pc.editor("cursor", "Cursor.exe", "resources/app/bin/code.cmd");
        let mut places = pc.places();
        places.path = vec![cursor.join("resources/app/bin")];
        assert!(find(editor("vscode"), &places).is_none());
    }

    #[test]
    fn finds_a_custom_install_through_path() {
        let pc = Pc::new("path");
        let root = pc.base.join("Tools").join("VS Code");
        touch(&root.join("Code.exe"));
        touch(&root.join("bin/code.cmd"));
        let mut places = pc.places();
        places.path = vec![PathBuf::from("relative"), root.join("bin")];
        let found = find(editor("vscode"), &places).unwrap();
        assert_eq!(found.cli, root.join("bin/code.cmd"));
    }

    #[test]
    fn states_follow_the_folders() {
        let pc = Pc::new("states");
        let extensions = pc.base.join("home/.vscode/extensions");
        assert_eq!(state_of(&extensions, "1.1.0"), State::Missing);
        pc.extension(".vscode", "other.extension-1.1.0");
        assert_eq!(state_of(&extensions, "1.1.0"), State::Missing);
        pc.extension(".vscode", "vorssaint.vorssaint-terminal-bridge-1.0.3");
        assert_eq!(state_of(&extensions, "1.1.0"), State::Outdated);
        // 1.1.0 isn't 1.1.01.
        pc.extension(".vscode", "vorssaint.vorssaint-terminal-bridge-1.1.01");
        assert_eq!(state_of(&extensions, "1.1.0"), State::Outdated);
        pc.extension(
            ".vscode",
            "vorssaint.vorssaint-terminal-bridge-1.1.0-universal",
        );
        assert_eq!(state_of(&extensions, "1.1.0"), State::Current);
    }

    #[test]
    fn a_folder_marked_obsolete_is_not_installed() {
        let pc = Pc::new("obsolete");
        let extensions = pc.base.join("home/.vscode/extensions");
        pc.extension(".vscode", "vorssaint.vorssaint-terminal-bridge-1.1.0");
        fs::write(
            extensions.join(".obsolete"),
            r#"{"vorssaint.vorssaint-terminal-bridge-1.1.0":true}"#,
        )
        .unwrap();
        assert_eq!(state_of(&extensions, "1.1.0"), State::Missing);
        fs::write(extensions.join(".obsolete"), "not json").unwrap();
        assert_eq!(state_of(&extensions, "1.1.0"), State::Current);
    }

    #[test]
    fn the_editors_own_list_outranks_the_folders() {
        let pc = Pc::new("listed");
        let extensions = pc.base.join("home/.vscode/extensions");
        // As VS Code leaves it: uninstalled, the folder still there, not in .obsolete.
        pc.extension(".vscode", "vorssaint.vorssaint-terminal-bridge-1.1.0");
        fs::write(extensions.join("extensions.json"), "[]").unwrap();
        assert_eq!(state_of(&extensions, "1.1.0"), State::Missing);

        fs::write(
            extensions.join("extensions.json"),
            r#"[{"identifier":{"id":"Vorssaint.Vorssaint-Terminal-Bridge"},"version":"1.0.3"},
                {"identifier":{"id":"other.extension"},"version":"1.1.0"}]"#,
        )
        .unwrap();
        assert_eq!(state_of(&extensions, "1.1.0"), State::Outdated);

        fs::write(
            extensions.join("extensions.json"),
            r#"[{"identifier":{"id":"vorssaint.vorssaint-terminal-bridge"},"version":"1.1.0"}]"#,
        )
        .unwrap();
        assert_eq!(state_of(&extensions, "1.1.0"), State::Current);

        // A list it can't read says nothing, so the folders decide.
        fs::write(extensions.join("extensions.json"), "{ broken").unwrap();
        assert_eq!(state_of(&extensions, "1.1.0"), State::Current);
    }

    #[test]
    fn an_unknown_editor_is_not_found() {
        let pc = Pc::new("unknown");
        assert_eq!(
            install("notepad", &pc.places(), "1.1.0"),
            Err(Failure::NotFound)
        );
        assert_eq!(
            install("vscode", &pc.places(), "1.1.0"),
            Err(Failure::NotFound)
        );
    }

    // The CLI paths run real scripts, so these are for the platforms the tests run on.
    #[cfg(unix)]
    mod cli {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        fn script(pc: &Pc, body: &str) -> PathBuf {
            let root = pc.editor("Microsoft VS Code", "Code.exe", "bin/code.cmd");
            let cli = root.join("bin/code.cmd");
            fs::write(&cli, format!("#!/bin/sh\n{body}\n")).unwrap();
            fs::set_permissions(&cli, fs::Permissions::from_mode(0o755)).unwrap();
            cli
        }

        #[test]
        fn install_reports_each_way_it_can_end() {
            let pc = Pc::new("cli");
            let extensions = pc.base.join("home/.vscode/extensions");

            script(&pc, "exit 3");
            assert_eq!(
                install("vscode", &pc.places(), "1.1.0"),
                Err(Failure::Refused)
            );

            // Exit 0 with nothing to show for it.
            script(&pc, "exit 0");
            assert_eq!(
                install("vscode", &pc.places(), "1.1.0"),
                Err(Failure::Refused)
            );

            // Gets the args and a .vsix that is really there.
            let done = extensions.join("vorssaint.vorssaint-terminal-bridge-1.1.0");
            script(
                &pc,
                &format!(
                    "[ \"$1\" = --install-extension ] && [ -s \"$2\" ] && [ \"$3\" = --force ] && mkdir -p '{}'",
                    done.display()
                ),
            );
            assert_eq!(install("vscode", &pc.places(), "1.1.0"), Ok(()));
            assert!(done.is_dir());
        }

        #[test]
        fn a_cli_that_hangs_is_given_up_on() {
            let pc = Pc::new("hang");
            let cli = script(&pc, "exec sleep 30");
            let started = Instant::now();
            let result = run_cli(&cli, &[], &pc.base, Duration::from_millis(200));
            assert_eq!(result, Err(Failure::TimedOut));
            assert!(started.elapsed() < Duration::from_secs(5));
        }

        #[test]
        fn a_cli_that_will_not_start_says_why() {
            let pc = Pc::new("nostart");
            let missing = pc.base.join("nothing.cmd");
            let result = run_cli(&missing, &[], &pc.base, Duration::from_secs(1));
            assert!(matches!(result, Err(Failure::Other(_))));
        }
    }

    #[test]
    fn reasons_are_words_settings_knows() {
        assert_eq!(Failure::NotFound.into_reason(), "notFound");
        assert_eq!(Failure::TimedOut.into_reason(), "timedOut");
        assert_eq!(
            Failure::Other("disk full".into()).into_reason(),
            "disk full"
        );
    }
}
