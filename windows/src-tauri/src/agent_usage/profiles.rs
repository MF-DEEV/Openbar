// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Codex homes of their own, each a `CODEX_HOME` whose account gets a limits
//! card (CodexAccountLimits.swift, CodexProfileLimitsService.swift; spec 03
//! §3.5). They are kept apart from the provider-wide reading of the logs and
//! in memory only. One home is checked at a time: when the AI page shows them
//! and they are due, or when a card or Settings asks, and never while the
//! section, Codex or the Limits card is off.
//!
//! Only metadata is looked at: Codex, not Vorssaint, opens the sign-in.

use super::codex;
use super::model::{Limits, Time};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub const MAXIMUM: usize = 8;
const MAXIMUM_NAME: usize = 40;

/// A reading this recent still stands.
const REFRESH_INTERVAL: f64 = 5.0 * 60.0;
/// After a window renews, the next reading comes sooner.
const RENEWED_INTERVAL: f64 = 60.0;
const RETRY_AFTER: f64 = 5.0 * 60.0;
/// Only the person can install, update or sign in to Codex, or fix a folder.
const SETTLED_RETRY: f64 = 30.0 * 60.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub directory: String,
}

/// The saved homes (`island.agentsCodexProfiles`): at most eight, each with
/// a UUID, a name of 1–40 characters and an absolute folder, none twice.
pub fn decode(value: Option<&Value>) -> Vec<Profile> {
    let Some(list) = value.and_then(Value::as_array) else {
        return vec![];
    };
    let (mut ids, mut paths) = (HashSet::new(), HashSet::new());
    list.iter()
        .take(MAXIMUM)
        .filter_map(|v| serde_json::from_value::<Profile>(v.clone()).ok())
        .filter(|p| {
            is_uuid(&p.id)
                && !p.name.trim().is_empty()
                && p.name.chars().count() <= MAXIMUM_NAME
                && is_absolute(&p.directory)
                && ids.insert(p.id.to_lowercase())
                && paths.insert(path_key(&p.directory))
        })
        .collect()
}

fn is_uuid(id: &str) -> bool {
    let groups: Vec<&str> = id.split('-').collect();
    groups.iter().map(|g| g.len()).eq([8, 4, 4, 4, 12])
        && groups
            .iter()
            .all(|g| g.chars().all(|c| c.is_ascii_hexdigit()))
}

/// `C:\…` or `\\server\…`; tests on other systems also take `/…`.
fn is_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    let drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    drive || path.starts_with("\\\\") || Path::new(path).is_absolute()
}

/// One folder spelled with other slashes or case is the same folder.
fn path_key(path: &str) -> String {
    path.replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

/// `\\?\C:\x` as `C:\x`; a network share is never a private home.
fn plain(path: PathBuf) -> Option<PathBuf> {
    let text = path.to_string_lossy();
    if text.starts_with(r"\\?\UNC\") || (text.starts_with(r"\\") && !text.starts_with(r"\\?\")) {
        return None;
    }
    Some(match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => path,
    })
}

fn same(a: &Path, b: &Path) -> bool {
    path_key(&a.to_string_lossy()) == path_key(&b.to_string_lossy())
}

/// The folder with its links resolved, if it is a private Codex home: not a
/// drive's root or the user's own folder, owned by this user and writable by
/// no one else but the system and administrators, holding a private
/// `auth.json` that is no link, or a `config.toml`, since Codex can keep its
/// sign-in in the credential store.
pub fn canonical(directory: &str) -> Option<PathBuf> {
    if !is_absolute(directory) {
        return None;
    }
    let url = plain(std::fs::canonicalize(directory).ok()?)?;
    let home = super::files::home()
        .and_then(|h| std::fs::canonicalize(h).ok())
        .and_then(plain);
    if url.parent().is_none() || home.is_some_and(|h| same(&h, &url)) || !url.is_dir() {
        return None;
    }
    if !platform::private(&url, platform::Access::Write) {
        return None;
    }
    let auth = url.join("auth.json");
    if let Ok(metadata) = std::fs::symlink_metadata(&auth) {
        // A symbolic or hard link would make two homes one account.
        return (metadata.is_file()
            && platform::single_file(&auth)
            && platform::private(&auth, platform::Access::Any))
        .then_some(url);
    }
    std::fs::symlink_metadata(url.join("config.toml"))
        .is_ok_and(|m| m.is_file())
        .then_some(url)
}

/// The usual home and the `.codex-<name>` homes beside `~/.codex`. The usual
/// one is named `default_name`, so it can have a card beside the others.
pub fn discover(home: &Path, usual: Option<&Path>, default_name: &str) -> Vec<Profile> {
    let mut folders: Vec<PathBuf> = std::fs::read_dir(home)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    let name = folder_name(p).to_lowercase();
                    name == ".codex" || name.starts_with(".codex-")
                })
                .collect()
        })
        .unwrap_or_default();
    folders.sort();
    let usual = usual
        .and_then(|u| std::fs::canonicalize(u).ok())
        .and_then(plain);
    folders.splice(0..0, usual.clone());
    let mut paths = HashSet::new();
    folders
        .into_iter()
        .filter_map(|folder| {
            let canonical = canonical(&folder.to_string_lossy())?;
            if !paths.insert(path_key(&canonical.to_string_lossy())) {
                return None;
            }
            let name = if usual.as_ref().is_some_and(|u| same(u, &canonical)) {
                default_name.to_string()
            } else {
                name_for(&folder, default_name)
            };
            Some(Profile {
                id: codex::uuid(),
                name,
                directory: canonical.to_string_lossy().into_owned(),
            })
        })
        .collect()
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// `.codex-work` is Work; the usual `.codex` takes `default_name`.
pub fn name_for(folder: &Path, default_name: &str) -> String {
    let folder = folder_name(folder);
    let lower = folder.to_lowercase();
    let name = if lower.starts_with(".codex-") {
        &folder[".codex-".len()..]
    } else {
        folder.trim_start_matches('.')
    };
    if name.is_empty() || lower == ".codex" {
        return default_name.to_string();
    }
    capitalized(name).chars().take(MAXIMUM_NAME).collect()
}

/// Each word's first letter upper case and the rest lower (String.capitalized).
fn capitalized(text: &str) -> String {
    let mut start = true;
    text.chars()
        .flat_map(|c| {
            let out: Vec<char> = if start {
                c.to_uppercase().collect()
            } else {
                c.to_lowercase().collect()
            };
            start = !c.is_alphanumeric();
            out
        })
        .collect()
}

// MARK: Checking one home

/// What stopped an account check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The folder is no longer a private Codex home.
    InvalidProfile,
    /// Neither the Codex app nor its command-line tool is on this PC.
    MissingCli,
    /// Signed out, or signed in with a key, which has no plan limits.
    NeedsSignIn,
    /// A Codex from before its limits could be read from outside it.
    Outdated,
    /// No usable answer: it stopped, ran out of time or refused.
    Unavailable,
    /// The home was removed, or the section, Codex or the Limits card turned off.
    Cancelled,
}

impl Failure {
    pub fn key(self) -> &'static str {
        match self {
            Failure::InvalidProfile => "invalidProfile",
            Failure::MissingCli => "missing",
            Failure::NeedsSignIn => "needsSignIn",
            Failure::Outdated => "outdated",
            Failure::Unavailable => "unavailable",
            Failure::Cancelled => "cancelled",
        }
    }

    fn from_codex(failure: codex::Failure) -> Failure {
        match failure {
            codex::Failure::Missing => Failure::MissingCli,
            codex::Failure::NeedsSignIn => Failure::NeedsSignIn,
            codex::Failure::Outdated => Failure::Outdated,
            codex::Failure::Unreachable | codex::Failure::Refused => Failure::Unavailable,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    pub limits: Limits,
    pub plan: Option<String>,
}

/// How long after the last check the account is asked again.
pub fn interval(failure: Option<Failure>, renewed: bool) -> f64 {
    match failure {
        None if renewed => RENEWED_INTERVAL,
        None => REFRESH_INTERVAL,
        Some(Failure::Unavailable | Failure::Cancelled) => RETRY_AFTER,
        Some(
            Failure::InvalidProfile
            | Failure::MissingCli
            | Failure::NeedsSignIn
            | Failure::Outdated,
        ) => SETTLED_RETRY,
    }
}

/// Variables the check may keep, in any case: the system's own, the proxy
/// and certificate settings, and the telemetry opt-outs. Neither a sign-in,
/// a provider nor a shared state folder the app was started with may point
/// a check at another account.
const ALLOWED: [&str; 35] = [
    "HOME",
    "USER",
    "LOGNAME",
    "TMPDIR",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "NODE_EXTRA_CA_CERTS",
    "DO_NOT_TRACK",
    "DISABLE_TELEMETRY",
    // Node and Rust programs fail without the Windows ones.
    "USERPROFILE",
    "USERNAME",
    "USERDOMAIN",
    "HOMEDRIVE",
    "HOMEPATH",
    "APPDATA",
    "LOCALAPPDATA",
    "SystemRoot",
    "SystemDrive",
    "windir",
    "ComSpec",
    "PATHEXT",
    "TEMP",
    "TMP",
    "ProgramData",
    "ProgramFiles",
    "ProgramFiles(x86)",
    "ProgramW6432",
    "CommonProgramFiles",
    "PROCESSOR_ARCHITECTURE",
    "NUMBER_OF_PROCESSORS",
    "OS",
];

pub fn environment(
    directory: &Path,
    path: String,
    base: impl IntoIterator<Item = (String, String)>,
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = base
        .into_iter()
        .filter(|(key, _)| ALLOWED.iter().any(|a| a.eq_ignore_ascii_case(key)))
        .collect();
    out.push(("PATH".into(), path));
    out.push((
        "CODEX_HOME".into(),
        directory.to_string_lossy().into_owned(),
    ));
    out.push(("LANG".into(), "en_US.UTF-8".into()));
    out
}

/// Asks only for the account and its limits, never refreshing its sign-in on
/// purpose and never using a reset. A Codex from before resets still reports
/// limits, so none are required.
pub fn read(
    profile: &Profile,
    cancelled: Arc<AtomicBool>,
    now: &dyn Fn() -> Time,
) -> Result<Reading, Failure> {
    let directory = canonical(&profile.directory).ok_or(Failure::InvalidProfile)?;
    if cancelled.load(Ordering::Relaxed) {
        return Err(Failure::Cancelled);
    }
    let answer = codex::check_home(
        |executable| {
            environment(
                &directory,
                super::cli::child_path(executable),
                std::env::vars(),
            )
        },
        cancelled.clone(),
    );
    if cancelled.load(Ordering::Relaxed) {
        return Err(Failure::Cancelled);
    }
    let (account, result) = answer.map_err(Failure::from_codex)?;
    let limits = codex::limits(&result, now()).ok_or(Failure::Unavailable)?;
    Ok(Reading {
        limits,
        plan: plan(&account),
    })
}

/// The plan's name, for the plans Codex accounts can have.
fn plan(account: &Value) -> Option<String> {
    let plan = account.pointer("/account/planType")?.as_str()?;
    [
        "free",
        "go",
        "plus",
        "pro",
        "team",
        "business",
        "enterprise",
        "edu",
    ]
    .contains(&plan)
    .then(|| super::pricing::codex_plan(Some(plan), &super::pricing::list()))
    .flatten()
    .map(|p| p.name)
}

// MARK: The queue

#[derive(Clone, Debug, PartialEq)]
pub struct Account {
    pub profile: Profile,
    pub limits: Option<Limits>,
    pub plan: Option<String>,
    /// Waiting for its turn or being checked.
    pub checking: bool,
    pub failure: Option<Failure>,
    /// When the last check began; None until one has.
    pub attempted: Option<Time>,
}

impl Account {
    fn new(profile: Profile) -> Account {
        Account {
            profile,
            limits: None,
            plan: None,
            checking: false,
            failure: None,
            attempted: None,
        }
    }

    /// An expired window is unknown, not a freshly replenished allowance.
    pub fn current_limits(&self, now: Time) -> Option<Limits> {
        let mut limits = self.limits.clone()?;
        limits
            .windows
            .retain(|w| !w.resets_at.is_some_and(|r| r <= now));
        (!limits.windows.is_empty()).then_some(limits)
    }

    pub fn is_due(&self, now: Time) -> bool {
        let Some(attempted) = self.attempted else {
            return true;
        };
        // Only a window that renewed after the reading asks sooner, so a
        // Codex that answers with a past renewal is not asked every minute.
        let renewed = self.limits.as_ref().is_some_and(|reading| {
            reading.windows.iter().any(|w| {
                w.resets_at
                    .is_some_and(|r| r > reading.observed_at && r <= now)
            })
        });
        now - attempted >= interval(self.failure, renewed)
    }
}

/// A check under way: what it asks and how to call it off.
#[derive(Clone, Debug)]
pub struct Check {
    pub profile: Profile,
    pub cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct Accounts {
    pub list: Vec<Account>,
    running: Option<(String, Arc<AtomicBool>)>,
    /// Checks waiting their turn. Those the person asked for still run after
    /// the page closes; the page's own wait for it to show again.
    waiting: Vec<(String, bool)>,
}

impl Accounts {
    /// Follows the saved homes; true when they changed.
    pub fn synchronize(&mut self, wanted: &[Profile]) -> bool {
        if self.list.iter().map(|a| &a.profile).eq(wanted.iter()) {
            return false;
        }
        let previous = std::mem::take(&mut self.list);
        self.list = wanted
            .iter()
            .map(|profile| {
                // A renamed home keeps its reading and its check.
                match previous.iter().find(|a| {
                    a.profile.id == profile.id && a.profile.directory == profile.directory
                }) {
                    Some(account) => Account {
                        profile: profile.clone(),
                        ..account.clone()
                    },
                    None => Account::new(profile.clone()),
                }
            })
            .collect();
        let kept: HashSet<String> = self
            .list
            .iter()
            .filter(|a| a.checking)
            .map(|a| a.profile.id.clone())
            .collect();
        self.waiting.retain(|(id, _)| kept.contains(id));
        if let Some((id, cancelled)) = &self.running
            && !kept.contains(id)
        {
            cancelled.store(true, Ordering::Relaxed);
            self.running = None;
        }
        true
    }

    /// Queues the homes that are due, or, when the person asks, `id` or
    /// every home. One already waiting or under way is not asked twice.
    pub fn refresh(&mut self, id: Option<&str>, force: bool, now: Time) -> bool {
        let mut changed = false;
        for account in self
            .list
            .iter_mut()
            .filter(|a| id.is_none_or(|id| a.profile.id == id))
        {
            if account.checking {
                if force
                    && let Some(place) = self
                        .waiting
                        .iter_mut()
                        .find(|(w, _)| *w == account.profile.id)
                {
                    place.1 = true;
                }
                continue;
            }
            if !force && !account.is_due(now) {
                continue;
            }
            account.checking = true;
            self.waiting.push((account.profile.id.clone(), force));
            changed = true;
        }
        changed
    }

    /// The next check to start, once the one under way has finished.
    pub fn next(&mut self, now: Time) -> Option<Check> {
        if self.running.is_some() {
            return None;
        }
        while !self.waiting.is_empty() {
            let (id, _) = self.waiting.remove(0);
            let Some(account) = self.list.iter_mut().find(|a| a.profile.id == id) else {
                continue;
            };
            account.attempted = Some(now);
            let cancelled = Arc::new(AtomicBool::new(false));
            self.running = Some((id, cancelled.clone()));
            return Some(Check {
                profile: account.profile.clone(),
                cancelled,
            });
        }
        None
    }

    /// A check's answer; one called off meanwhile is set aside.
    pub fn finish(&mut self, check: &Check, result: Result<Reading, Failure>) -> bool {
        match &self.running {
            Some((_, cancelled)) if Arc::ptr_eq(cancelled, &check.cancelled) => self.running = None,
            _ => return false,
        }
        let Some(account) = self.list.iter_mut().find(|a| {
            a.profile.id == check.profile.id && a.profile.directory == check.profile.directory
        }) else {
            return true;
        };
        account.checking = false;
        match result {
            Ok(reading) => {
                account.limits = Some(reading.limits);
                account.plan = reading.plan;
                account.failure = None;
            }
            Err(Failure::Cancelled) => account.attempted = None,
            Err(failure) => {
                // Another sign-in may be behind the home now. A check that
                // could not finish leaves the last reading standing.
                if matches!(failure, Failure::NeedsSignIn | Failure::InvalidProfile) {
                    account.limits = None;
                    account.plan = None;
                }
                account.failure = Some(failure);
            }
        }
        true
    }

    /// The page closed: a check under way finishes and keeps its time, so
    /// looking at the page again does not start it over.
    pub fn page_hidden(&mut self) -> bool {
        let dropped: HashSet<String> = self
            .waiting
            .iter()
            .filter(|(_, asked)| !asked)
            .map(|(id, _)| id.clone())
            .collect();
        if dropped.is_empty() {
            return false;
        }
        self.waiting.retain(|(_, asked)| *asked);
        for account in self
            .list
            .iter_mut()
            .filter(|a| dropped.contains(&a.profile.id))
        {
            account.checking = false;
        }
        true
    }

    /// The section, Codex or the Limits card is off: nothing runs.
    pub fn cancel(&mut self) -> bool {
        if self.running.is_none() && self.waiting.is_empty() {
            return false;
        }
        if let Some((id, cancelled)) = self.running.take() {
            cancelled.store(true, Ordering::Relaxed);
            // Asked again once checks can run.
            if let Some(account) = self.list.iter_mut().find(|a| a.profile.id == id) {
                account.attempted = None;
            }
        }
        self.waiting.clear();
        for account in &mut self.list {
            account.checking = false;
        }
        true
    }
}

// MARK: Who may touch a folder

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::path::Path;
    use windows::Win32::Foundation::{HANDLE, HLOCAL, LocalFree};
    use windows::Win32::Security::Authorization::{
        ConvertSidToStringSidW, GetNamedSecurityInfoW, SE_FILE_OBJECT,
    };
    use windows::Win32::Security::{
        ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
        DACL_SECURITY_INFORMATION, GetAce, GetAclInformation, OWNER_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR, PSID,
    };
    use windows::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    use windows::core::{HSTRING, PWSTR};

    pub enum Access {
        /// Creating, changing, deleting or taking over (mode & 0o022 on the Mac).
        Write,
        /// Reading too (mode & 0o077).
        Any,
    }

    // FILE_WRITE_DATA, FILE_APPEND_DATA, FILE_WRITE_EA, FILE_DELETE_CHILD,
    // FILE_WRITE_ATTRIBUTES, DELETE, WRITE_DAC, WRITE_OWNER, GENERIC_ALL, GENERIC_WRITE.
    const WRITE: u32 = 0x2
        | 0x4
        | 0x10
        | 0x40
        | 0x100
        | 0x1_0000
        | 0x4_0000
        | 0x8_0000
        | 0x1000_0000
        | 0x4000_0000;
    // FILE_READ_DATA, FILE_READ_EA, GENERIC_READ.
    const READ: u32 = 0x1 | 0x8 | 0x8000_0000;
    const INHERIT_ONLY: u8 = 0x08;
    /// SYSTEM, Administrators and OWNER RIGHTS, who stand where root does on the Mac.
    const TRUSTED: [&str; 3] = ["S-1-5-18", "S-1-5-32-544", "S-1-3-4"];

    fn sid_string(sid: PSID) -> Option<String> {
        unsafe {
            let mut text = PWSTR::null();
            ConvertSidToStringSidW(sid, &mut text).ok()?;
            let out = text.to_string().ok();
            let _ = LocalFree(Some(HLOCAL(text.0.cast())));
            out
        }
    }

    /// Owned by this user, and no one else but the system and the
    /// administrators may write it (or, for `Any`, read it).
    pub fn private(path: &Path, access: Access) -> bool {
        let Ok(user) = crate::hooks::transport::current_user_sid() else {
            return false;
        };
        let rights = match access {
            Access::Write => WRITE,
            Access::Any => WRITE | READ,
        };
        unsafe {
            let mut owner = PSID::default();
            let mut dacl: *mut ACL = std::ptr::null_mut();
            let mut descriptor = PSECURITY_DESCRIPTOR::default();
            let status = GetNamedSecurityInfoW(
                &HSTRING::from(path),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                Some(&mut owner),
                None,
                Some(&mut dacl),
                None,
                &mut descriptor,
            );
            if status.is_err() {
                return false;
            }
            // No DACL at all lets everyone in.
            let allowed = !dacl.is_null()
                && sid_string(owner).as_deref() == Some(user.as_str())
                && only_trusted(dacl, rights, &user);
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
            allowed
        }
    }

    /// Every entry that grants any of `rights` to the object itself names
    /// the user or a trusted account. Deny entries only take away.
    unsafe fn only_trusted(dacl: *const ACL, rights: u32, user: &str) -> bool {
        unsafe {
            let mut info = ACL_SIZE_INFORMATION::default();
            if GetAclInformation(
                dacl,
                (&mut info as *mut ACL_SIZE_INFORMATION).cast(),
                std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
                AclSizeInformation,
            )
            .is_err()
            {
                return false;
            }
            for index in 0..info.AceCount {
                let mut ace: *mut c_void = std::ptr::null_mut();
                if GetAce(dacl, index, &mut ace).is_err() || ace.is_null() {
                    return false;
                }
                let header = &*(ace as *const ACE_HEADER);
                if header.AceFlags & INHERIT_ONLY != 0 {
                    continue;
                }
                match header.AceType {
                    // ACCESS_DENIED, ACCESS_DENIED_CALLBACK.
                    1 | 10 => continue,
                    // ACCESS_ALLOWED, ACCESS_ALLOWED_CALLBACK: same layout up to the SID.
                    0 | 9 => {
                        let entry = &*(ace as *const ACCESS_ALLOWED_ACE);
                        if entry.Mask & rights == 0 {
                            continue;
                        }
                        let sid = PSID(&entry.SidStart as *const u32 as *mut c_void);
                        match sid_string(sid) {
                            Some(s) if s == user || TRUSTED.contains(&s.as_str()) => {}
                            _ => return false,
                        }
                    }
                    // Kinds a file's DACL does not use are not trusted.
                    _ => return false,
                }
            }
            true
        }
    }

    /// A plain file with one name: not a link of either kind.
    pub fn single_file(path: &Path) -> bool {
        use std::os::windows::fs::OpenOptionsExt;
        use std::os::windows::io::AsRawHandle;
        // FILE_READ_ATTRIBUTES, and the link itself rather than its target.
        let Ok(file) = std::fs::OpenOptions::new()
            .access_mode(0x80)
            .custom_flags(0x0020_0000)
            .open(path)
        else {
            return false;
        };
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info) }.is_ok()
            && info.nNumberOfLinks == 1
            // FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT.
            && info.dwFileAttributes & (0x10 | 0x400) == 0
    }
}

/// Mode bits stand in for the ACL where the tests run on another system.
#[cfg(not(windows))]
mod platform {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::Path;

    pub enum Access {
        Write,
        Any,
    }

    pub fn private(path: &Path, access: Access) -> bool {
        let mask = match access {
            Access::Write => 0o022,
            Access::Any => 0o077,
        };
        std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & mask == 0)
    }

    pub fn single_file(path: &Path) -> bool {
        std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && m.nlink() == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_usage::model::{LimitSource, LimitWindow, Provider, WindowKind};
    use serde_json::json;

    const A: &str = "11111111-2222-4333-8444-555555555555";
    const B: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";

    fn profile(id: &str, name: &str, directory: &str) -> Profile {
        Profile {
            id: id.into(),
            name: name.into(),
            directory: directory.into(),
        }
    }

    fn limits(observed: Time, resets: Option<Time>) -> Limits {
        Limits {
            provider: Provider::Codex,
            windows: vec![LimitWindow {
                id: "codex.300".into(),
                kind: WindowKind::Session,
                minutes: Some(300),
                scope: None,
                used_percent: 40.0,
                resets_at: resets,
                estimated_resets_at: None,
            }],
            observed_at: observed,
            source: LimitSource::Account,
        }
    }

    #[test]
    fn saved_homes_are_kept_only_when_whole_and_unique() {
        let long = "x".repeat(41);
        let value = json!([
            {"id": A, "name": "Work", "directory": r"C:\Users\me\.codex-work"},
            {"id": A, "name": "Same id", "directory": r"C:\Users\me\.codex-other"},
            {"id": B, "name": "Same folder", "directory": "c:/users/me/.codex-work/"},
            {"id": "not-a-uuid", "name": "Bad", "directory": r"C:\x"},
            {"id": "22222222-2222-4222-8222-222222222222", "name": "  ", "directory": r"C:\y"},
            {"id": "33333333-3333-4333-8333-333333333333", "name": long, "directory": r"C:\z"},
            {"id": "44444444-4444-4444-8444-444444444444", "name": "Relative", "directory": r"codex"},
            {"id": "55555555-5555-4555-8555-555555555555", "name": "Personal", "directory": r"D:\codex"},
            "nonsense"
        ]);
        let decoded = decode(Some(&value));
        assert_eq!(
            decoded.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["Work", "Personal"]
        );
        assert!(decode(Some(&json!("[]"))).is_empty());
        assert!(decode(None).is_empty());
        let many: Vec<Value> = (0..10)
            .map(|i| json!({"id": format!("{i:08}-0000-4000-8000-000000000000"), "name": "N", "directory": format!(r"C:\h{i}")}))
            .collect();
        assert_eq!(decode(Some(&Value::from(many))).len(), MAXIMUM);
    }

    #[test]
    fn folders_are_named_as_on_the_mac() {
        assert_eq!(name_for(Path::new(r"/h/.codex"), "Default"), "Default");
        assert_eq!(name_for(Path::new(r"/h/.codex-work"), "Default"), "Work");
        assert_eq!(
            name_for(Path::new(r"/h/.codex-my-TEAM"), "Default"),
            "My-Team"
        );
        assert_eq!(name_for(Path::new(r"/h/.codex-"), "Default"), "Default");
        assert_eq!(name_for(Path::new(r"/h/personal"), "Default"), "Personal");
        assert_eq!(
            name_for(Path::new(&format!("/h/.codex-{}", "a".repeat(60))), "D")
                .chars()
                .count(),
            40
        );
    }

    #[test]
    fn verbatim_paths_read_plainly_and_shares_are_refused() {
        assert_eq!(
            plain(PathBuf::from(r"\\?\C:\Users\me\.codex")),
            Some(PathBuf::from(r"C:\Users\me\.codex"))
        );
        assert_eq!(plain(PathBuf::from(r"\\?\UNC\server\share")), None);
        assert_eq!(plain(PathBuf::from(r"\\server\share")), None);
    }

    #[test]
    fn the_check_keeps_only_allowed_variables() {
        let base = [
            ("SystemRoot", r"C:\Windows"),
            ("https_proxy", "http://proxy:8080"),
            ("CODEX_HOME", r"C:\elsewhere"),
            ("OPENAI_API_KEY", "sk-secret"),
            ("CODEX_API_KEY", "secret"),
            ("Path", r"C:\inherited"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()));
        let env = environment(
            Path::new(r"C:\Users\me\.codex-work"),
            r"C:\bin".into(),
            base,
        );
        let get = |k: &str| {
            env.iter()
                .filter(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(get("SystemRoot"), [r"C:\Windows"]);
        assert_eq!(get("https_proxy"), ["http://proxy:8080"]);
        assert_eq!(get("CODEX_HOME"), [r"C:\Users\me\.codex-work"]);
        assert!(get("OPENAI_API_KEY").is_empty() && get("CODEX_API_KEY").is_empty());
        assert!(get("Path").is_empty());
        assert_eq!(get("PATH"), [r"C:\bin"]);
        assert_eq!(get("LANG"), ["en_US.UTF-8"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_home_must_be_private_and_hold_a_sign_in_or_a_config() {
        use std::os::unix::fs::PermissionsExt;
        let set = |p: &Path, mode| {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)).unwrap()
        };
        let root = std::env::temp_dir().join(format!("vorssaint-profiles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        let work = home.join(".codex-work");
        let empty = home.join(".codex-empty");
        let usual = home.join(".codex");
        for d in [&work, &empty, &usual] {
            std::fs::create_dir_all(d).unwrap();
            set(d, 0o700);
        }
        let path = |p: &Path| p.to_string_lossy().into_owned();
        assert_eq!(
            canonical(&path(&empty)),
            None,
            "neither auth.json nor config.toml"
        );
        std::fs::write(usual.join("config.toml"), "").unwrap();
        let auth = work.join("auth.json");
        std::fs::write(&auth, "{}").unwrap();
        set(&auth, 0o600);
        let work_canonical = std::fs::canonicalize(&work).unwrap();
        assert_eq!(canonical(&path(&work)), Some(work_canonical.clone()));
        set(&auth, 0o644);
        assert_eq!(
            canonical(&path(&work)),
            None,
            "auth.json readable by others"
        );
        set(&auth, 0o600);
        std::fs::hard_link(&auth, root.join("copy.json")).unwrap();
        assert_eq!(canonical(&path(&work)), None, "auth.json has a second name");
        std::fs::remove_file(root.join("copy.json")).unwrap();
        set(&work, 0o777);
        assert_eq!(canonical(&path(&work)), None, "folder writable by others");
        set(&work, 0o700);
        assert_eq!(canonical("relative"), None);

        let found = discover(&home, Some(&usual), "Default");
        assert_eq!(
            found.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["Default", "Work"],
            "the usual home first, then the others; the empty one is left out"
        );
        assert!(found.iter().all(|p| is_uuid(&p.id)));
        assert_eq!(found[1].directory, work_canonical.to_string_lossy());
        std::fs::remove_dir_all(&root).unwrap();
    }

    fn reading(observed: Time) -> Reading {
        Reading {
            limits: limits(observed, Some(observed + 3600.0)),
            plan: Some("Plus".into()),
        }
    }

    #[test]
    fn homes_are_checked_one_at_a_time_and_only_when_due() {
        let mut accounts = Accounts::default();
        let list = [profile(A, "Work", r"C:\w"), profile(B, "Home", r"C:\h")];
        assert!(accounts.synchronize(&list));
        assert!(!accounts.synchronize(&list));
        assert!(accounts.refresh(None, false, 0.0));
        assert!(accounts.list.iter().all(|a| a.checking));
        let first = accounts.next(0.0).unwrap();
        assert_eq!(first.profile.id, A);
        assert!(accounts.next(0.0).is_none(), "one at a time");
        assert!(accounts.finish(&first, Ok(reading(0.0))));
        let second = accounts.next(1.0).unwrap();
        assert_eq!(second.profile.id, B);
        assert!(accounts.finish(&second, Err(Failure::NeedsSignIn)));
        assert_eq!(accounts.list[0].plan.as_deref(), Some("Plus"));
        assert_eq!(accounts.list[1].failure, Some(Failure::NeedsSignIn));

        // Read a minute ago: not due; a sign-in failure waits half an hour.
        assert!(!accounts.refresh(None, false, 60.0));
        assert!(accounts.refresh(None, false, REFRESH_INTERVAL));
        assert_eq!(accounts.list.iter().filter(|a| a.checking).count(), 1);
        // Asked for, it runs at once.
        assert!(accounts.refresh(Some(B), true, REFRESH_INTERVAL));
        assert!(accounts.list.iter().all(|a| a.checking));
    }

    #[test]
    fn a_renewed_window_asks_sooner_and_reads_as_unknown() {
        let mut account = Account::new(profile(A, "Work", r"C:\w"));
        account.limits = Some(limits(0.0, Some(100.0)));
        account.attempted = Some(0.0);
        assert!(!account.is_due(99.0));
        assert!(account.current_limits(99.0).is_some());
        assert!(
            account.current_limits(100.0).is_none(),
            "expired, not replenished"
        );
        assert!(!account.is_due(RENEWED_INTERVAL - 1.0));
        assert!(account.is_due(RENEWED_INTERVAL.max(100.0)));
        // A renewal from before the reading does not.
        account.limits = Some(limits(200.0, Some(150.0)));
        assert!(!account.is_due(RENEWED_INTERVAL.max(200.0)));
        assert_eq!(interval(Some(Failure::Unavailable), false), RETRY_AFTER);
        assert_eq!(interval(Some(Failure::Outdated), false), SETTLED_RETRY);
    }

    #[test]
    fn leaving_the_page_keeps_asked_checks_and_the_one_under_way() {
        let mut accounts = Accounts::default();
        accounts.synchronize(&[profile(A, "Work", r"C:\w"), profile(B, "Home", r"C:\h")]);
        accounts.refresh(None, false, 0.0);
        let running = accounts.next(0.0).unwrap();
        assert!(accounts.page_hidden());
        assert!(accounts.list[0].checking && !accounts.list[1].checking);
        accounts.refresh(Some(B), true, 0.0);
        assert!(!accounts.page_hidden(), "asked for, it stays");
        accounts.finish(&running, Ok(reading(0.0)));
        assert_eq!(accounts.next(1.0).unwrap().profile.id, B);
    }

    #[test]
    fn removing_or_turning_off_calls_the_check_off() {
        let mut accounts = Accounts::default();
        accounts.synchronize(&[profile(A, "Work", r"C:\w"), profile(B, "Home", r"C:\h")]);
        accounts.refresh(None, true, 0.0);
        let running = accounts.next(0.0).unwrap();
        // Renamed: the same home keeps its check.
        accounts.synchronize(&[profile(A, "Job", r"C:\w"), profile(B, "Home", r"C:\h")]);
        assert!(!running.cancelled.load(Ordering::Relaxed));
        // Removed: called off, and its answer is set aside.
        accounts.synchronize(&[profile(B, "Home", r"C:\h")]);
        assert!(running.cancelled.load(Ordering::Relaxed));
        assert!(!accounts.finish(&running, Ok(reading(0.0))));
        let next = accounts.next(1.0).unwrap();
        assert_eq!(next.profile.id, B);
        assert!(accounts.cancel());
        assert!(next.cancelled.load(Ordering::Relaxed));
        assert_eq!(
            accounts.list[0].attempted, None,
            "asked again once checks can run"
        );
        assert!(!accounts.list[0].checking);
        assert!(!accounts.cancel());
    }

    #[test]
    fn a_failed_check_keeps_the_last_reading_unless_the_sign_in_changed() {
        let mut accounts = Accounts::default();
        accounts.synchronize(&[profile(A, "Work", r"C:\w")]);
        accounts.refresh(None, true, 0.0);
        let check = accounts.next(0.0).unwrap();
        accounts.finish(&check, Ok(reading(0.0)));
        accounts.refresh(None, true, 1.0);
        let check = accounts.next(1.0).unwrap();
        accounts.finish(&check, Err(Failure::Unavailable));
        assert!(accounts.list[0].limits.is_some());
        accounts.refresh(None, true, 2.0);
        let check = accounts.next(2.0).unwrap();
        accounts.finish(&check, Err(Failure::Cancelled));
        assert_eq!(accounts.list[0].attempted, None);
        accounts.refresh(None, true, 3.0);
        let check = accounts.next(3.0).unwrap();
        accounts.finish(&check, Err(Failure::InvalidProfile));
        assert!(accounts.list[0].limits.is_none() && accounts.list[0].plan.is_none());
    }
}
