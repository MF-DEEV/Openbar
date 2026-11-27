// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! JetBrains IDEs as hosts (#329): IntelliJ IDEA, PyCharm, WebStorm, GoLand,
//! PhpStorm, RubyMine, CLion, Rider, RustRover, Android Studio and the rest
//! of the IntelliJ Platform. Each carries a `product-info.json` beside its
//! `bin` folder that names it, its settings folder and its launcher. The
//! bridge plugin (Tools/JetBrainsBridge) travels inside the app and goes
//! into each IDE's plugins folder; it then answers the island's requests
//! like the VS Code bridge (`bridge.rs`).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// The plugin, as built from Tools/JetBrainsBridge.
pub const PLUGIN: &[u8] =
    include_bytes!("../../../../Tools/JetBrainsBridge/dist/openbar-terminal-bridge.jar");
pub const PLUGIN_FILE: &str = "openbar-terminal-bridge.jar";

/// Products on the platform without a terminal of their own: Gateway, the
/// remote development client, Fleet.
const NOT_IDES: [&str; 4] = ["GW", "JBC", "FL", "CWMGuest"];

/// The IDE a launcher's program name stands for, as the process tree shows
/// it: `pycharm64.exe` on Windows, `pycharm` on Linux.
pub fn ide_name(program: &str) -> Option<&'static str> {
    let lowered = program.to_ascii_lowercase();
    let stem = lowered.strip_suffix(".exe").unwrap_or(&lowered);
    let stem = stem.strip_suffix("64").unwrap_or(stem);
    Some(match stem {
        "idea" => "IntelliJ IDEA",
        "pycharm" => "PyCharm",
        "webstorm" => "WebStorm",
        "goland" => "GoLand",
        "phpstorm" => "PhpStorm",
        "rubymine" => "RubyMine",
        "clion" => "CLion",
        "rider" => "Rider",
        "rustrover" => "RustRover",
        "studio" => "Android Studio",
        "datagrip" => "DataGrip",
        "dataspell" => "DataSpell",
        "aqua" => "Aqua",
        _ => return None,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductInfo {
    name: String,
    version: Option<String>,
    product_code: Option<String>,
    data_directory_name: String,
    product_vendor: Option<String>,
    #[serde(default)]
    launch: Vec<Launch>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Launch {
    os: Option<String>,
    launcher_path: Option<String>,
}

/// One JetBrains IDE install.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ide {
    pub name: String,
    pub version: String,
    /// The folder holding `bin` and `product-info.json`.
    pub home: PathBuf,
    /// Its settings folder's name, like "PyCharm2026.3".
    pub data_dir: String,
    /// "JetBrains", or "Google" for Android Studio.
    pub vendor: String,
    /// The program that opens a project in it.
    pub launcher: Option<PathBuf>,
}

/// A name with no way out of the folder it is joined to.
fn plain(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != ".."
}

impl Ide {
    /// The IDE installed in `home`, or None for any other folder.
    pub fn read(home: &Path) -> Option<Ide> {
        let file = home.join("product-info.json");
        if std::fs::metadata(&file).ok()?.len() > 1 << 20 {
            return None;
        }
        let product: ProductInfo = serde_json::from_slice(&std::fs::read(&file).ok()?).ok()?;
        if NOT_IDES.contains(&product.product_code.as_deref().unwrap_or(""))
            || !plain(&product.data_directory_name)
        {
            return None;
        }
        let vendor = product
            .product_vendor
            .filter(|vendor| plain(vendor))
            .unwrap_or_else(|| "JetBrains".into());
        let os = if cfg!(windows) { "windows" } else { "linux" };
        let launcher = product
            .launch
            .iter()
            .filter(|launch| {
                launch
                    .os
                    .as_deref()
                    .is_none_or(|o| o.eq_ignore_ascii_case(os))
            })
            .find_map(|launch| launch.launcher_path.as_deref())
            .filter(|path| !path.split(['/', '\\']).any(|part| part == ".."))
            .map(|path| home.join(path));
        Some(Ide {
            name: product.name,
            version: product.version.unwrap_or_default(),
            home: home.to_path_buf(),
            data_dir: product.data_directory_name,
            vendor,
            launcher,
        })
    }

    /// The IDE a running program belongs to, from its image: the launcher
    /// in `bin`, or the Java runtime in `jbr/bin` of older launchers.
    pub fn of_image(image: &Path) -> Option<Ide> {
        image.ancestors().skip(1).take(3).find_map(Ide::read)
    }

    pub fn id(&self) -> String {
        format!("{}/{}", self.vendor, self.data_dir)
    }

    /// Where the IDE loads plugins from at start.
    pub fn plugins(&self) -> Option<PathBuf> {
        plugins_folder(&self.vendor, &self.data_dir)
    }

    /// The IDE runs now, so a change to its plugins waits for its restart.
    pub fn running(&self) -> bool {
        let Some(name) = self.launcher.as_deref().and_then(Path::file_name) else {
            return false;
        };
        super::process::pids_named(&name.to_string_lossy())
            .into_iter()
            .filter_map(super::process::image_path)
            .any(|image| image.starts_with(&self.home))
    }
}

#[cfg(windows)]
fn plugins_folder(vendor: &str, data_dir: &str) -> Option<PathBuf> {
    let roaming = std::env::var_os("APPDATA").map(PathBuf::from)?;
    Some(roaming.join(vendor).join(data_dir).join("plugins"))
}

/// On Linux the plugins sit right in the IDE's data folder.
#[cfg(not(windows))]
fn plugins_folder(vendor: &str, data_dir: &str) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    let data = crate::linux::paths::data()?;
    #[cfg(not(target_os = "linux"))]
    let data = PathBuf::from(std::env::var_os("HOME")?).join(".local/share");
    Some(data.join(vendor).join(data_dir))
}

/// Every JetBrains IDE found, one per settings folder: two copies of the
/// same version share their settings and plugins.
pub fn installed() -> Vec<Ide> {
    let mut found: Vec<Ide> = Vec::new();
    for candidate in candidates() {
        if let Some(ide) = Ide::read(&candidate)
            && !found.iter().any(|known| known.id() == ide.id())
        {
            found.push(ide);
        }
    }
    found.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
    found
}

/// The folders an IDE may be installed in: standalone installs and the
/// Toolbox App's, which kept them three levels down before Toolbox 2.
fn candidates() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut toolbox: Option<PathBuf> = None;
    #[cfg(windows)]
    {
        for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(base) = std::env::var_os(variable).map(PathBuf::from) {
                roots.push(base.join("JetBrains"));
                roots.push(base.join("Android"));
            }
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            roots.push(local.join("Programs"));
            toolbox = Some(local.join("JetBrains").join("Toolbox").join("apps"));
        }
    }
    #[cfg(not(windows))]
    {
        roots.push(PathBuf::from("/opt"));
        roots.push(PathBuf::from("/usr/local"));
        roots.push(PathBuf::from("/usr/share"));
        roots.push(PathBuf::from("/snap"));
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            roots.push(home.join("Applications"));
            roots.push(home.join(".local").join("opt"));
            roots.push(home.join("opt"));
            toolbox = Some(home.join(".local/share/JetBrains/Toolbox/apps"));
        }
    }
    let mut candidates = Vec::new();
    for root in roots {
        for child in folders(&root) {
            // A snap keeps its files in `current`.
            candidates.push(child.join("current"));
            candidates.extend(folders(&child));
            candidates.push(child);
        }
    }
    if let Some(toolbox) = toolbox {
        for product in folders(&toolbox) {
            for channel in folders(&product) {
                candidates.extend(folders(&channel));
                candidates.push(channel);
            }
            candidates.push(product);
        }
    }
    candidates
}

fn folders(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    found.sort();
    found
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    Missing,
    Current,
    Outdated,
}

/// Where the plugin stands in an IDE: the jar is built the same each time
/// from the same sources, so a different one is another version.
pub fn state(ide: &Ide) -> State {
    state_in(ide.plugins().as_deref(), PLUGIN)
}

fn state_in(plugins: Option<&Path>, ours: &[u8]) -> State {
    let Some(file) = plugins.map(|p| p.join(PLUGIN_FILE)) else {
        return State::Missing;
    };
    match std::fs::read(&file) {
        Ok(bytes) if bytes == ours => State::Current,
        Ok(_) => State::Outdated,
        Err(_) if file.exists() => State::Outdated,
        Err(_) => State::Missing,
    }
}

/// Why a change to an IDE's plugins didn't happen.
#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    /// No settings folder could be named for it.
    NoFolder,
    /// The IDE holds the jar open (Windows): it has to close first.
    InUse,
    Other(String),
}

/// Puts the plugin into the IDE's plugins folder, which it loads at its
/// next start: written beside, then moved into place, so the IDE never
/// finds half a jar.
pub fn install(ide: &Ide) -> Result<(), Failure> {
    install_in(&ide.plugins().ok_or(Failure::NoFolder)?, PLUGIN)
}

fn install_in(folder: &Path, plugin: &[u8]) -> Result<(), Failure> {
    std::fs::create_dir_all(folder).map_err(|e| Failure::Other(e.to_string()))?;
    let target = folder.join(PLUGIN_FILE);
    let partial = folder.join(format!(".{PLUGIN_FILE}.{}", std::process::id()));
    std::fs::write(&partial, plugin).map_err(|e| Failure::Other(e.to_string()))?;
    match std::fs::rename(&partial, &target) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(&partial);
            Err(failure(error))
        }
    }
}

/// Takes the plugin out; a running IDE keeps it until it restarts, and on
/// Windows holds the file until it closes.
pub fn remove(ide: &Ide) -> Result<(), Failure> {
    remove_in(&ide.plugins().ok_or(Failure::NoFolder)?)
}

fn remove_in(folder: &Path) -> Result<(), Failure> {
    match std::fs::remove_file(folder.join(PLUGIN_FILE)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(failure(error)),
    }
}

fn failure(error: std::io::Error) -> Failure {
    // ERROR_SHARING_VIOLATION and ERROR_ACCESS_DENIED on a file the IDE has open.
    if cfg!(windows) && matches!(error.raw_os_error(), Some(32) | Some(5)) {
        Failure::InUse
    } else {
        Failure::Other(error.to_string())
    }
}

/// Updates the plugin in every IDE holding an older one, at launch.
pub fn update_installed() {
    for ide in installed() {
        if state(&ide) == State::Outdated {
            let _ = install(&ide);
        }
    }
}

/// Takes the plugin out of every IDE, when Openbar goes.
pub fn remove_everywhere() {
    for ide in installed() {
        if state(&ide) != State::Missing {
            let _ = remove(&ide);
        }
    }
}

/// What the island takes from a lock file Claude Code's JetBrains plugin
/// keeps for each IDE (`<config>/ide/<port>.lock`). The file also holds the
/// token of Claude's own connection to the IDE; this type has no field for
/// it, so it is never kept, passed on or used.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeLock {
    pub pid: Option<u32>,
    #[serde(default)]
    pub workspace_folders: Vec<String>,
}

impl ClaudeLock {
    pub fn parse(bytes: &[u8]) -> Option<ClaudeLock> {
        (bytes.len() < 1 << 16)
            .then(|| serde_json::from_slice(bytes).ok())
            .flatten()
    }
}

/// Every lock file in these Claude Code config folders.
pub fn locks(homes: &[PathBuf]) -> Vec<ClaudeLock> {
    homes
        .iter()
        .filter_map(|home| std::fs::read_dir(home.join("ide")).ok())
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "lock"))
        .filter_map(|e| ClaudeLock::parse(&std::fs::read(e.path()).ok()?))
        .collect()
}

/// A folder as this system writes it. Claude Code's JetBrains plugin has
/// been seen writing WSL paths on Windows (`/mnt/c/work/app`), and Git
/// Bash's are like it (`/c/work/app`).
pub fn native_folder(folder: &str) -> String {
    if cfg!(windows) {
        let rest = folder
            .strip_prefix("/mnt/")
            .or_else(|| folder.strip_prefix('/'))
            .filter(|rest| {
                let mut chars = rest.chars();
                chars.next().is_some_and(|c| c.is_ascii_alphabetic())
                    && matches!(chars.next(), None | Some('/'))
            });
        if let Some(rest) = rest {
            let drive = rest[..1].to_ascii_uppercase();
            let path = rest[1..].trim_start_matches('/').replace('/', "\\");
            return format!("{drive}:\\{path}");
        }
        return folder.replace('/', "\\");
    }
    folder.to_owned()
}

/// The deepest of `folders` that holds `cwd`, as written there.
pub fn open_folder(cwd: &str, folders: &[String]) -> Option<String> {
    let key = |path: &str| {
        let native = native_folder(path);
        let trimmed = native.trim_end_matches(['/', '\\']).to_owned();
        if cfg!(windows) {
            trimmed.to_lowercase()
        } else {
            trimmed
        }
    };
    let wanted = key(cwd);
    let separator = if cfg!(windows) { '\\' } else { '/' };
    folders
        .iter()
        .map(|folder| (native_folder(folder), key(folder)))
        .filter(|(_, folder)| !folder.is_empty())
        .filter(|(_, folder)| {
            wanted == *folder || wanted.starts_with(&format!("{folder}{separator}"))
        })
        .max_by_key(|(_, folder)| folder.len())
        .map(|(native, _)| native)
}

/// The project open in the IDE process `ide_pid` that holds the session's
/// folder, by the lock file Claude Code's JetBrains plugin keeps for it.
pub fn open_project(cwd: &str, ide_pid: u32, homes: &[PathBuf]) -> Option<String> {
    let folders: Vec<String> = locks(homes)
        .into_iter()
        .filter(|lock| lock.pid == Some(ide_pid))
        .flat_map(|lock| lock.workspace_folders)
        .collect();
    open_folder(cwd, &folders)
}

/// Asks the IDE to open a project it already has open, which brings that
/// project's window forward and opens nothing new. The launcher hands the
/// folder to the running IDE and exits.
pub fn show_project(ide: &Ide, folder: &str) -> bool {
    let Some(launcher) = ide.launcher.as_deref().filter(|l| l.is_file()) else {
        return false;
    };
    if !Path::new(folder).is_dir() {
        return false;
    }
    let mut command = std::process::Command::new(launcher);
    command
        .arg(folder)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let Ok(mut child) = command.spawn() else {
        return false;
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            // Still running: the IDE took it; the launcher lingers on its own.
            Ok(None) => return true,
            Err(_) => return false,
        }
    }
}

/// The bridge plugin runs in the IDE process `ide_pid`: it registered itself
/// in the island's folder under that process.
pub fn listening(ide_pid: u32) -> bool {
    super::bridge::folder().is_some_and(|folder| folder.join(format!("{ide_pid}.json")).is_file())
}

/// One IDE in Settings › Sessions.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeStatus {
    pub id: String,
    pub name: String,
    pub version: String,
    pub state: State,
    /// It runs now: a change waits for it to restart.
    pub running: bool,
}

/// The JetBrains IDEs found and the bridge plugin in each.
#[tauri::command]
pub async fn jetbrains_bridge_status() -> Vec<IdeStatus> {
    tauri::async_runtime::spawn_blocking(|| {
        installed()
            .into_iter()
            .map(|ide| IdeStatus {
                id: ide.id(),
                state: state(&ide),
                running: ide.running(),
                name: ide.name,
                version: ide.version,
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// Puts the plugin into the IDE `id` names, or takes it out. The error is a
/// reason the page words: "inUse", "noFolder", "notFound", or the system's.
#[tauri::command]
pub async fn jetbrains_bridge_change(id: String, install: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let ide = installed()
            .into_iter()
            .find(|ide| ide.id() == id)
            .ok_or("notFound".to_owned())?;
        let done = if install {
            self::install(&ide)
        } else {
            remove(&ide)
        };
        done.map_err(|failure| match failure {
            Failure::InUse => "inUse".to_owned(),
            Failure::NoFolder => "noFolder".to_owned(),
            Failure::Other(reason) => reason,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Opens Settings on the Sessions section, from a JetBrains session's reply field.
#[tauri::command]
pub fn sessions_open_bridge_settings<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    crate::settings::open_section(&app, "content", "sessions");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ob-jb-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn launchers_name_their_ide() {
        assert_eq!(ide_name("pycharm64.exe"), Some("PyCharm"));
        assert_eq!(ide_name("IDEA64.EXE"), Some("IntelliJ IDEA"));
        assert_eq!(ide_name("webstorm"), Some("WebStorm"));
        assert_eq!(ide_name("studio64.exe"), Some("Android Studio"));
        assert_eq!(ide_name("rider64.exe"), Some("Rider"));
        assert_eq!(ide_name("java"), None);
        assert_eq!(ide_name("code.exe"), None);
    }

    #[test]
    fn an_install_is_read_from_its_product_info() {
        let home = temp("read");
        let launcher = if cfg!(windows) {
            "bin/pycharm64.exe"
        } else {
            "bin/pycharm"
        };
        std::fs::write(
            home.join("product-info.json"),
            format!(
                r#"{{"name":"PyCharm","version":"2026.3","productCode":"PY","dataDirectoryName":"PyCharm2026.3",
                "productVendor":"JetBrains","launch":[{{"os":"macOS","launcherPath":"../MacOS/pycharm"}},
                {{"os":"{}","launcherPath":"{launcher}"}}]}}"#,
                if cfg!(windows) { "Windows" } else { "Linux" }
            ),
        )
        .unwrap();
        let ide = Ide::read(&home).unwrap();
        assert_eq!(ide.name, "PyCharm");
        assert_eq!(ide.data_dir, "PyCharm2026.3");
        assert_eq!(ide.vendor, "JetBrains");
        assert_eq!(ide.launcher, Some(home.join(launcher)));
        assert_eq!(ide.id(), "JetBrains/PyCharm2026.3");
        std::fs::create_dir_all(home.join("bin")).unwrap();
        assert_eq!(
            Ide::of_image(&home.join("bin").join("pycharm64.exe"))
                .unwrap()
                .home,
            home
        );
        assert_eq!(
            Ide::of_image(&home.join("jbr/bin/java")).unwrap().home,
            home
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn gateway_and_odd_folders_are_no_ides() {
        let home = temp("odd");
        std::fs::write(
            home.join("product-info.json"),
            r#"{"name":"Gateway","productCode":"GW","dataDirectoryName":"Gateway2026.3"}"#,
        )
        .unwrap();
        assert_eq!(Ide::read(&home), None);
        std::fs::write(
            home.join("product-info.json"),
            r#"{"name":"X","dataDirectoryName":"../../etc"}"#,
        )
        .unwrap();
        assert_eq!(Ide::read(&home), None);
        std::fs::write(
            home.join("product-info.json"),
            r#"{"name":"X","dataDirectoryName":"X1","launch":[{"launcherPath":"../../evil"}]}"#,
        )
        .unwrap();
        assert_eq!(Ide::read(&home).unwrap().launcher, None);
        std::fs::write(home.join("product-info.json"), "not json").unwrap();
        assert_eq!(Ide::read(&home), None);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn the_plugin_installs_updates_and_goes() {
        let folder = temp("plugins").join("PyCharm2026.3");
        assert_eq!(state_in(Some(&folder), b"v1"), State::Missing);
        install_in(&folder, b"v1").unwrap();
        assert_eq!(state_in(Some(&folder), b"v1"), State::Current);
        assert_eq!(state_in(Some(&folder), b"v2"), State::Outdated);
        install_in(&folder, b"v2").unwrap();
        assert_eq!(std::fs::read(folder.join(PLUGIN_FILE)).unwrap(), b"v2");
        let names: Vec<_> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(
            names,
            vec![std::ffi::OsString::from(PLUGIN_FILE)],
            "nothing half written is left beside it"
        );
        remove_in(&folder).unwrap();
        assert_eq!(state_in(Some(&folder), b"v2"), State::Missing);
        remove_in(&folder).unwrap();
        assert_eq!(state_in(None, b"v2"), State::Missing);
        let _ = std::fs::remove_dir_all(folder.parent().unwrap());
    }

    #[test]
    fn the_bundled_plugin_is_a_jar_with_its_descriptor() {
        assert!(PLUGIN.starts_with(b"PK"), "a zip");
        let text = String::from_utf8_lossy(PLUGIN);
        assert!(text.contains("META-INF/plugin.xml"));
    }

    #[test]
    fn the_lock_file_is_read_without_the_token() {
        let lock = ClaudeLock::parse(
            br#"{"pid":4321,"workspaceFolders":["/w/alpha","/w/beta"],"ideName":"PyCharm","transport":"ws","authToken":"secret-123"}"#,
        )
        .unwrap();
        assert_eq!(lock.pid, Some(4321));
        assert_eq!(lock.workspace_folders, vec!["/w/alpha", "/w/beta"]);
        assert!(
            !format!("{lock:?}").contains("secret-123"),
            "the token is nowhere in what was read"
        );
        assert_eq!(ClaudeLock::parse(b"nope"), None);
        assert_eq!(ClaudeLock::parse(&vec![b' '; 1 << 17]), None);

        let home = temp("locks");
        std::fs::create_dir_all(home.join("ide")).unwrap();
        let alpha = home.join("alpha");
        let beta = home.join("beta");
        std::fs::create_dir_all(beta.join("src")).unwrap();
        let folders =
            serde_json::to_string(&[alpha.to_string_lossy(), beta.to_string_lossy()]).unwrap();
        std::fs::write(
            home.join("ide/1.lock"),
            format!(r#"{{"pid":77,"workspaceFolders":{folders},"authToken":"t"}}"#),
        )
        .unwrap();
        std::fs::write(home.join("ide/2.lock"), "junk").unwrap();
        let cwd = beta.join("src");
        assert_eq!(
            open_project(&cwd.to_string_lossy(), 77, std::slice::from_ref(&home)),
            Some(beta.to_string_lossy().into_owned())
        );
        assert_eq!(
            open_project(&cwd.to_string_lossy(), 78, std::slice::from_ref(&home)),
            None,
            "another IDE's projects never count"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn the_deepest_open_folder_holds_the_session() {
        let sep = if cfg!(windows) { "\\" } else { "/" };
        let root = if cfg!(windows) { "C:\\w" } else { "/w" };
        let folders = vec![root.to_string(), format!("{root}{sep}app")];
        assert_eq!(
            open_folder(&format!("{root}{sep}app{sep}src"), &folders),
            Some(format!("{root}{sep}app"))
        );
        assert_eq!(
            open_folder(&format!("{root}{sep}apple"), &folders),
            Some(root.to_string())
        );
        assert_eq!(
            open_folder(&format!("{root}x"), &folders),
            None,
            "a name is matched whole"
        );
    }

    #[cfg(windows)]
    #[test]
    fn wsl_paths_become_windows_paths() {
        assert_eq!(native_folder("/mnt/c/Users/me/app"), "C:\\Users\\me\\app");
        assert_eq!(native_folder("/d/work"), "D:\\work");
        assert_eq!(native_folder("C:/Users/me"), "C:\\Users\\me");
        assert_eq!(
            open_folder("c:\\users\\me\\app\\src", &["/mnt/c/Users/me/app".into()]),
            Some("C:\\Users\\me\\app".into())
        );
    }
}
