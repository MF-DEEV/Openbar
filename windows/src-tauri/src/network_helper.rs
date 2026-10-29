// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Per-app network activity without running the island elevated. On the Mac
//! `nettop` needs no privileges; on Windows only an ETW system logger sees
//! which process sent a packet, and starting one takes an administrator.
//!
//! So an administrator approves it once: Windows asks (UAC), and the
//! elevated `--install-network-helper` copies this exe to Program Files,
//! where the user can't change it, and registers a hidden scheduled task
//! that runs that copy with the user's highest privileges. The user may run
//! the task but not edit or delete it, so it can't be pointed at anything
//! else. From then on the network detail starts the task without a prompt:
//! the copy runs `--network-helper`, owns the ETW session while an island is
//! connected to its pipe and answers it with each process's bytes. It exits
//! after half a minute without an island. Uninstalling removes it.

use super::ProcessStatus;

/// Bumped whenever the pipe messages change, so an island never reads an
/// older helper's answers; an old helper is approved (copied) again.
pub(super) const PROTOCOL: u32 = 1;

pub(super) const HELPER_FLAG: &str = "--network-helper";
pub(super) const INSTALL_FLAG: &str = "--install-network-helper";
pub(super) const REMOVE_FLAG: &str = "--remove-network-helper";

const TASK_PREFIX: &str = "Vorssaint Network Activity ";

fn task_name(sid: &str) -> String {
    format!("{TASK_PREFIX}{sid}")
}

fn pipe_name(sid: &str) -> String {
    format!(r"\\.\pipe\vorssaint-network-{sid}")
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Runs `exe --network-helper` as `sid` with its full (elevated) token,
/// on demand only, without a time limit and on battery too.
fn task_xml(sid: &str, exe: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Lets Vorssaint show which apps use the network.</Description>
  </RegistrationInfo>
  <Principals>
    <Principal id="Author">
      <UserId>{sid}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>Parallel</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <AllowHardTerminate>true</AllowHardTerminate>
    <Hidden>true</Hidden>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>"{exe}"</Command>
      <Arguments>{HELPER_FLAG}</Arguments>
    </Exec>
  </Actions>
</Task>"#,
        sid = xml_escape(sid),
        exe = xml_escape(exe),
    )
}

/// Administrators and SYSTEM own the task; the user may read and run it only.
fn task_sddl(sid: &str) -> String {
    format!("D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FRFX;;;{sid})")
}

/// The copy's folder: administrators and SYSTEM may change it, users may
/// only run what's in it. Protected, so a looser parent can't leak in.
const FOLDER_SDDL: &str = "D:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;BU)";

/// The user may read and write the pipe (not create instances of it), at
/// medium integrity, so the unelevated island can talk to the helper.
fn pipe_sddl(sid: &str) -> String {
    format!("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x12008b;;;{sid})S:(ML;;NW;;;ME)")
}

/// The pipe's messages. The helper greets with its protocol and whether its
/// trace started; each one-byte request gets one reply with the bytes per
/// process since the previous one. All little-endian.
mod wire {
    use super::ProcessStatus;

    pub const SAMPLE: u8 = 1;
    pub const HELLO_LEN: usize = 5;
    pub const REPLY_HEAD: usize = 13;
    pub const ENTRY_LEN: usize = 28;
    /// The collector's own bound on processes per interval, plus one.
    pub const MAX_ENTRIES: usize = 16_385;

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Entry {
        pub pid: u32,
        pub down: u64,
        pub up: u64,
        pub first: i64,
    }

    fn status_code(status: ProcessStatus) -> u8 {
        match status {
            ProcessStatus::Measuring => 0,
            ProcessStatus::Ready => 1,
            ProcessStatus::Unavailable => 2,
            ProcessStatus::PermissionDenied => 3,
            ProcessStatus::Incomplete => 4,
            ProcessStatus::NeedsApproval => 5,
        }
    }

    fn status(code: u8) -> Option<ProcessStatus> {
        Some(match code {
            0 => ProcessStatus::Measuring,
            1 => ProcessStatus::Ready,
            2 => ProcessStatus::Unavailable,
            3 => ProcessStatus::PermissionDenied,
            4 => ProcessStatus::Incomplete,
            5 => ProcessStatus::NeedsApproval,
            _ => return None,
        })
    }

    pub fn hello(protocol: u32, status: ProcessStatus) -> [u8; HELLO_LEN] {
        let mut out = [0; HELLO_LEN];
        out[..4].copy_from_slice(&protocol.to_le_bytes());
        out[4] = status_code(status);
        out
    }

    pub fn read_hello(bytes: &[u8; HELLO_LEN]) -> (u32, Option<ProcessStatus>) {
        (
            u32::from_le_bytes(bytes[..4].try_into().unwrap()),
            status(bytes[4]),
        )
    }

    pub fn reply(status: ProcessStatus, elapsed: f64, entries: &[Entry]) -> Vec<u8> {
        let entries = &entries[..entries.len().min(MAX_ENTRIES)];
        let mut out = Vec::with_capacity(REPLY_HEAD + entries.len() * ENTRY_LEN);
        out.push(status_code(status));
        out.extend_from_slice(&elapsed.to_le_bytes());
        out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        for entry in entries {
            out.extend_from_slice(&entry.pid.to_le_bytes());
            out.extend_from_slice(&entry.down.to_le_bytes());
            out.extend_from_slice(&entry.up.to_le_bytes());
            out.extend_from_slice(&entry.first.to_le_bytes());
        }
        out
    }

    /// The status, interval and how many entries follow; None for a reply
    /// no helper of this protocol would send.
    pub fn read_head(head: &[u8; REPLY_HEAD]) -> Option<(ProcessStatus, f64, usize)> {
        let status = status(head[0])?;
        let elapsed = f64::from_le_bytes(head[1..9].try_into().unwrap());
        let count = u32::from_le_bytes(head[9..13].try_into().unwrap()) as usize;
        (elapsed.is_finite() && elapsed >= 0.0 && count <= MAX_ENTRIES)
            .then_some((status, elapsed, count))
    }

    pub fn read_entry(bytes: &[u8; ENTRY_LEN]) -> Entry {
        Entry {
            pid: u32::from_le_bytes(bytes[..4].try_into().unwrap()),
            down: u64::from_le_bytes(bytes[4..12].try_into().unwrap()),
            up: u64::from_le_bytes(bytes[12..20].try_into().unwrap()),
            first: i64::from_le_bytes(bytes[20..28].try_into().unwrap()),
        }
    }
}

#[cfg(windows)]
pub(super) use imp::{HelperClient, approve, command_main};

#[cfg(windows)]
mod imp {
    use super::super::network_process::{Bytes, NetworkTrace};
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::{
        CloseHandle, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, HANDLE, HLOCAL, LocalFree,
        WAIT_OBJECT_0,
    };
    use windows::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        SDDL_REVISION_1,
    };
    use windows::Win32::Security::{
        DACL_SECURITY_INFORMATION, GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
        SetFileSecurityW, TOKEN_ELEVATION, TOKEN_ELEVATION_TYPE, TOKEN_QUERY, TOKEN_USER,
        TokenElevation, TokenElevationType, TokenElevationTypeLimited, TokenUser,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, FILE_GENERIC_READ,
        FILE_SHARE_NONE, FILE_WRITE_DATA, MOVEFILE_DELAY_UNTIL_REBOOT, MoveFileExW, OPEN_EXISTING,
        PIPE_ACCESS_DUPLEX, ReadFile, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT, WriteFile,
    };
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
        CoTaskMemFree, CoUninitialize,
    };
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeServerProcessId,
        PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT, PeekNamedPipe,
    };
    use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
    use windows::Win32::System::TaskScheduler::{
        IRegisteredTask, ITaskFolder, ITaskService, TASK_CREATE_OR_UPDATE, TASK_ENUM_HIDDEN,
        TASK_LOGON_INTERACTIVE_TOKEN, TaskScheduler,
    };
    use windows::Win32::System::Threading::{
        CreateEventW, GetCurrentProcess, OpenProcess, OpenProcessToken,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, ResetEvent,
        TerminateProcess, WaitForSingleObject,
    };
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Shell::{FOLDERID_ProgramFiles, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
    use windows::core::{BSTR, HSTRING, PCWSTR};

    /// How long a helper waits for an island, and for its next request.
    const IDLE: Duration = Duration::from_secs(30);
    /// How long an island waits for the helper it started to open its pipe.
    const START: Duration = Duration::from_secs(10);
    /// How long an island waits for the greeting and for each reply.
    const REPLY: Duration = Duration::from_secs(2);

    struct Handle(HANDLE);

    impl Drop for Handle {
        fn drop(&mut self) {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }

    /// COM for this call, on threads that may or may not have it already.
    struct Com(bool);

    impl Com {
        fn init() -> Self {
            Com(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok())
        }
    }

    impl Drop for Com {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    fn token_info<T: Copy>(class: windows::Win32::Security::TOKEN_INFORMATION_CLASS) -> Option<T> {
        unsafe {
            let mut token = HANDLE::default();
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).ok()?;
            let token = Handle(token);
            let mut value = std::mem::zeroed::<T>();
            let mut len = 0;
            GetTokenInformation(
                token.0,
                class,
                Some((&mut value as *mut T).cast()),
                std::mem::size_of::<T>() as u32,
                &mut len,
            )
            .ok()?;
            Some(value)
        }
    }

    /// "S-1-5-21-…", the account this process runs as (elevated or not).
    fn user_sid() -> Option<String> {
        process_sid(unsafe { GetCurrentProcess() })
    }

    fn process_sid(process: HANDLE) -> Option<String> {
        unsafe {
            let mut token = HANDLE::default();
            OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
            let token = Handle(token);
            let mut len = 0;
            let _ = GetTokenInformation(token.0, TokenUser, None, 0, &mut len);
            // u64s keep the SID that follows TOKEN_USER aligned.
            let mut buffer = vec![0u64; (len as usize).div_ceil(8)];
            GetTokenInformation(
                token.0,
                TokenUser,
                Some(buffer.as_mut_ptr().cast()),
                len,
                &mut len,
            )
            .ok()?;
            let user = &*(buffer.as_ptr() as *const TOKEN_USER);
            let mut text = windows::core::PWSTR::null();
            ConvertSidToStringSidW(user.User.Sid, &mut text).ok()?;
            let sid = text.to_string().ok();
            let _ = LocalFree(Some(HLOCAL(text.0.cast())));
            sid
        }
    }

    fn elevated() -> bool {
        token_info::<TOKEN_ELEVATION>(TokenElevation).is_some_and(|e| e.TokenIsElevated != 0)
    }

    /// An administrator running with UAC's limited token: Windows can ask
    /// them. Standard accounts and UAC turned off get no prompt to offer.
    fn can_approve() -> bool {
        token_info::<TOKEN_ELEVATION_TYPE>(TokenElevationType)
            .is_some_and(|t| t == TokenElevationTypeLimited)
    }

    /// `%ProgramFiles%\Vorssaint\Vorssaint Network.exe`, writable by administrators only.
    fn helper_path() -> Option<PathBuf> {
        unsafe {
            let path = SHGetKnownFolderPath(&FOLDERID_ProgramFiles, KF_FLAG_DEFAULT, None).ok()?;
            let text = path.to_string().ok();
            CoTaskMemFree(Some(path.0.cast()));
            Some(
                PathBuf::from(text?)
                    .join("Vorssaint")
                    .join("Vorssaint Network.exe"),
            )
        }
    }

    fn root_folder() -> Option<ITaskFolder> {
        unsafe {
            let service: ITaskService =
                CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER).ok()?;
            let empty = VARIANT::default();
            service.Connect(&empty, &empty, &empty, &empty).ok()?;
            service.GetFolder(&BSTR::from(r"\")).ok()
        }
    }

    fn registered_task(sid: &str) -> Option<IRegisteredTask> {
        unsafe { root_folder()?.GetTask(&BSTR::from(task_name(sid))).ok() }
    }

    // MARK: Approval (the island and the elevated copy of itself)

    /// Asks Windows to run this exe with `arg` elevated and waits for it;
    /// true when it ran and succeeded, false when declined or failed.
    ///
    /// PowerShell asks, not this process: with the sparse package's identity
    /// (docs/notifications.md) Windows won't start an elevated child of
    /// Vorssaint unless the package allows elevation, and says so only after
    /// the user said yes ("cannot access the specified device, path, or
    /// file"). PowerShell has no package identity.
    fn run_elevated(arg: &str) -> bool {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let Ok(exe) = std::env::current_exe() else {
            return false;
        };
        let mut system = [0u16; 260];
        let len = unsafe { GetSystemDirectoryW(Some(&mut system)) } as usize;
        if len == 0 || len >= system.len() {
            return false;
        }
        let powershell = PathBuf::from(String::from_utf16_lossy(&system[..len]))
            .join(r"WindowsPowerShell\v1.0\powershell.exe");
        // Through the environment, so no path needs quoting for PowerShell.
        std::process::Command::new(powershell)
            .args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg(
                // Stop: a declined prompt fails the command (exit 1) instead
                // of leaving $p null, which would exit 0.
                "$ErrorActionPreference = 'Stop'; \
                 $p = Start-Process -FilePath $env:VORSSAINT_EXE -ArgumentList $env:VORSSAINT_ARG \
                 -Verb RunAs -WindowStyle Hidden -Wait -PassThru; exit $p.ExitCode",
            )
            .env("VORSSAINT_EXE", exe)
            .env("VORSSAINT_ARG", arg)
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .is_ok_and(|status| status.success())
    }

    /// The network detail's button: Windows asks once, then the helper is
    /// there for every launch. False when declined or when it failed.
    pub fn approve() -> bool {
        run_elevated(INSTALL_FLAG)
    }

    /// Copies this exe where only administrators can change it and registers
    /// the task that runs that copy. Runs elevated.
    fn install() -> Option<()> {
        if !elevated() {
            return None;
        }
        let sid = user_sid()?;
        let source = std::env::current_exe().ok()?;
        let target = helper_path()?;
        let folder = target.parent()?;
        std::fs::create_dir_all(folder).ok()?;
        // Whoever made the folder, only administrators may change what the
        // task runs; the copy inherits this.
        protect(folder)?;
        // An older copy of this account's still waiting for an island would
        // answer the next one; its island never connects to a stale copy, so
        // it holds no trace.
        end_waiting_helpers(&target, &sid);
        // One that couldn't be ended keeps its image locked, but Windows lets
        // a running image be renamed out of the way.
        let old = target.with_extension("old");
        let _ = std::fs::remove_file(&old);
        if target.exists() {
            let _ = std::fs::rename(&target, &old);
        }
        std::fs::copy(&source, &target).ok()?;
        let _ = std::fs::remove_file(&old);
        let _com = Com::init();
        let target = target.to_str()?;
        unsafe {
            root_folder()?
                .RegisterTask(
                    &BSTR::from(task_name(&sid)),
                    &BSTR::from(task_xml(&sid, target)),
                    TASK_CREATE_OR_UPDATE.0,
                    &VARIANT::default(),
                    &VARIANT::default(),
                    TASK_LOGON_INTERACTIVE_TOKEN,
                    &VARIANT::from(task_sddl(&sid).as_str()),
                )
                .ok()?;
        }
        Some(())
    }

    fn protect(folder: &std::path::Path) -> Option<()> {
        let path = HSTRING::from(folder.as_os_str());
        let sddl = HSTRING::from(FOLDER_SDDL);
        unsafe {
            let mut descriptor = PSECURITY_DESCRIPTOR::default();
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl.as_ptr()),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
            .ok()?;
            let set =
                SetFileSecurityW(PCWSTR(path.as_ptr()), DACL_SECURITY_INFORMATION, descriptor);
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
            set.as_bool().then_some(())
        }
    }

    /// Only this account's: another account's helper may be serving its
    /// island, and ending it would leave its trace running until a restart.
    fn end_waiting_helpers(target: &std::path::Path, sid: &str) {
        let Some(name) = target.file_name().and_then(|n| n.to_str()) else {
            return;
        };
        let target = target.to_string_lossy();
        unsafe {
            let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return;
            };
            let snapshot = Handle(snapshot);
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut more = Process32FirstW(snapshot.0, &mut entry).is_ok();
            while more {
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
                let pid = entry.th32ProcessID;
                if String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case(name)
                    && crate::shell::process_path(pid)
                        .is_some_and(|path| path.eq_ignore_ascii_case(&target))
                    && let Ok(process) = OpenProcess(
                        PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE,
                        false,
                        pid,
                    )
                {
                    let process = Handle(process);
                    if process_sid(process.0).as_deref() == Some(sid) {
                        let _ = TerminateProcess(process.0, 1);
                        let _ = WaitForSingleObject(process.0, 2_000);
                    }
                }
                more = Process32NextW(snapshot.0, &mut entry).is_ok();
            }
        }
    }

    /// Whether the approved copy is this exe. After an update it isn't, and
    /// approving again copies the new one, so fixes reach the helper too.
    fn copy_is_current() -> bool {
        let (Ok(exe), Some(copy)) = (std::env::current_exe(), helper_path()) else {
            return false;
        };
        let same_size = matches!(
            (std::fs::metadata(&exe), std::fs::metadata(&copy)),
            (Ok(a), Ok(b)) if a.len() == b.len()
        );
        same_size && matches!((std::fs::read(&exe), std::fs::read(&copy)), (Ok(a), Ok(b)) if a == b)
    }

    /// The uninstaller's step. Only an administrator can delete the task and
    /// the copy, so an unelevated call with a task to remove asks Windows to
    /// run it elevated; with nothing installed it asks nothing.
    fn remove() -> Option<()> {
        let _com = Com::init();
        let sid = user_sid()?;
        if !elevated() {
            registered_task(&sid)?;
            return run_elevated(REMOVE_FLAG).then_some(());
        }
        let root = root_folder()?;
        unsafe {
            let _ = root.DeleteTask(&BSTR::from(task_name(&sid)), 0);
        }
        // Another account on this PC may still use the copy.
        let others = unsafe {
            let tasks = root.GetTasks(TASK_ENUM_HIDDEN.0).ok()?;
            let count = tasks.Count().ok()?;
            (1..=count).any(|i| {
                tasks
                    .get_Item(&VARIANT::from(i))
                    .and_then(|task| task.Name())
                    .is_ok_and(|name| name.to_string().starts_with(TASK_PREFIX))
            })
        };
        if !others && let Some(target) = helper_path() {
            for path in [target.with_extension("old"), target.clone()] {
                remove_now_or_at_restart(&path);
            }
            if let Some(folder) = target.parent() {
                remove_now_or_at_restart(folder); // Only once it's empty.
            }
        }
        Some(())
    }

    /// A helper still waiting for an island keeps its image; it exits on its
    /// own, and Windows deletes what's left when it restarts.
    fn remove_now_or_at_restart(path: &std::path::Path) {
        let removed = if path.is_dir() {
            std::fs::remove_dir(path)
        } else {
            std::fs::remove_file(path)
        };
        if removed.is_err() && path.exists() {
            let path = HSTRING::from(path.as_os_str());
            let _ =
                unsafe { MoveFileExW(PCWSTR(path.as_ptr()), None, MOVEFILE_DELAY_UNTIL_REBOOT) };
        }
    }

    /// `Vorssaint.exe --network-helper | --install-network-helper |
    /// --remove-network-helper`; returns for any other argument.
    pub fn command_main(arg: &str) {
        let ok = match arg {
            HELPER_FLAG => {
                serve();
                true
            }
            INSTALL_FLAG => install().is_some(),
            REMOVE_FLAG => remove().is_some(),
            _ => return,
        };
        std::process::exit(if ok { 0 } else { 1 })
    }

    // MARK: The helper (elevated, started by the task)

    fn create_pipe(sid: &str) -> Option<Handle> {
        let name = HSTRING::from(pipe_name(sid));
        let sddl = HSTRING::from(pipe_sddl(sid));
        unsafe {
            let mut descriptor = PSECURITY_DESCRIPTOR::default();
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl.as_ptr()),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
            .ok()?;
            let attributes = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor.0,
                bInheritHandle: false.into(),
            };
            let pipe = CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                64 * 1024,
                64 * 1024,
                0,
                Some(&attributes),
            );
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
            (!pipe.is_invalid()).then_some(Handle(pipe))
        }
    }

    /// Waits for an overlapped operation on the helper's pipe, cancelling it
    /// after `timeout`. The bytes moved, or None.
    fn finish(
        pipe: &Handle,
        overlapped: &mut OVERLAPPED,
        started: windows::core::Result<()>,
        timeout: Duration,
    ) -> Option<u32> {
        unsafe {
            if let Err(error) = started {
                if error.code() == ERROR_PIPE_CONNECTED.to_hresult() {
                    return Some(0);
                }
                if error.code() != ERROR_IO_PENDING.to_hresult() {
                    return None;
                }
                if WaitForSingleObject(overlapped.hEvent, timeout.as_millis() as u32)
                    != WAIT_OBJECT_0
                {
                    let _ = CancelIoEx(pipe.0, Some(overlapped));
                    let mut moved = 0;
                    let _ = GetOverlappedResult(pipe.0, overlapped, &mut moved, true);
                    return None;
                }
            }
            let mut moved = 0;
            GetOverlappedResult(pipe.0, overlapped, &mut moved, false).ok()?;
            Some(moved)
        }
    }

    fn overlapped(event: &Handle) -> OVERLAPPED {
        let _ = unsafe { ResetEvent(event.0) };
        OVERLAPPED {
            hEvent: event.0,
            ..Default::default()
        }
    }

    fn serve_read(pipe: &Handle, event: &Handle, buffer: &mut [u8], timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut filled = 0;
        while filled < buffer.len() {
            let left = deadline.saturating_duration_since(Instant::now());
            let mut io = overlapped(event);
            let started =
                unsafe { ReadFile(pipe.0, Some(&mut buffer[filled..]), None, Some(&mut io)) };
            match finish(pipe, &mut io, started, left) {
                Some(0) | None => return false,
                Some(read) => filled += read as usize,
            }
        }
        true
    }

    fn serve_write(pipe: &Handle, event: &Handle, mut bytes: &[u8]) -> bool {
        while !bytes.is_empty() {
            let mut io = overlapped(event);
            let started = unsafe { WriteFile(pipe.0, Some(bytes), None, Some(&mut io)) };
            match finish(pipe, &mut io, started, REPLY) {
                Some(0) | None => return false,
                Some(written) => bytes = &bytes[written as usize..],
            }
        }
        true
    }

    /// Serves islands one at a time, each with its own trace, until none has
    /// connected for `IDLE`.
    fn serve() {
        let Some(sid) = user_sid() else { return };
        // A helper that is just exiting may still hold the pipe.
        let started = Instant::now();
        let pipe = loop {
            if let Some(pipe) = create_pipe(&sid) {
                break pipe;
            }
            if started.elapsed() > Duration::from_secs(3) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        let Ok(event) = (unsafe { CreateEventW(None, true, false, None) }) else {
            return;
        };
        let event = Handle(event);
        loop {
            let mut io = overlapped(&event);
            let started = unsafe { ConnectNamedPipe(pipe.0, Some(&mut io)) };
            if finish(&pipe, &mut io, started, IDLE).is_none() {
                return;
            }
            session(&pipe, &event);
            let _ = unsafe { DisconnectNamedPipe(pipe.0) };
        }
    }

    /// One island: the trace runs from its greeting until it hangs up (or
    /// stops asking), and dropping it stops the ETW session.
    fn session(pipe: &Handle, event: &Handle) {
        let (status, trace) = match NetworkTrace::start() {
            Ok(trace) => (ProcessStatus::Ready, Some(trace)),
            Err(status) => (status, None),
        };
        if !serve_write(pipe, event, &wire::hello(PROTOCOL, status)) {
            return;
        }
        let Some(mut trace) = trace else { return };
        loop {
            let mut request = [0u8; 1];
            if !serve_read(pipe, event, &mut request, IDLE) || request[0] != wire::SAMPLE {
                return;
            }
            let (status, bytes, elapsed) = trace.sample();
            let entries: Vec<wire::Entry> = bytes
                .into_iter()
                .map(|(pid, bytes)| wire::Entry {
                    pid,
                    down: bytes.down,
                    up: bytes.up,
                    first: bytes.first,
                })
                .collect();
            if !serve_write(pipe, event, &wire::reply(status, elapsed, &entries)) {
                return;
            }
        }
    }

    // MARK: The island's side

    /// The network detail's link to the helper, standing in for a
    /// `NetworkTrace` the island isn't allowed to start itself.
    pub struct HelperClient {
        sid: String,
        pipe: Option<Handle>,
        started: Instant,
        failed: Option<ProcessStatus>,
    }

    impl HelperClient {
        /// Connects to a helper still waiting from before, or starts one.
        /// `NeedsApproval` when there is none yet and Windows can ask for it.
        pub fn start() -> Result<Self, ProcessStatus> {
            let _com = Com::init();
            let sid = user_sid().ok_or(ProcessStatus::Unavailable)?;
            let mut client = HelperClient {
                sid,
                pipe: None,
                started: Instant::now(),
                failed: None,
            };
            // A copy from an older Vorssaint, or none: an uninstall that found
            // it running deletes it at the next restart, even if it was
            // approved again since. Approving copies this exe again.
            let current = copy_is_current();
            if current && client.connect() {
                return Ok(client);
            }
            let Some(task) = registered_task(&client.sid).filter(|_| current) else {
                return Err(if can_approve() {
                    ProcessStatus::NeedsApproval
                } else {
                    ProcessStatus::PermissionDenied
                });
            };
            unsafe { task.Run(&VARIANT::default()) }.map_err(|_| ProcessStatus::Unavailable)?;
            Ok(client)
        }

        /// True once connected, or once the helper turned out unusable
        /// (`failed`); false while its pipe isn't there yet.
        fn connect(&mut self) -> bool {
            let name = HSTRING::from(pipe_name(&self.sid));
            let pipe = unsafe {
                CreateFileW(
                    PCWSTR(name.as_ptr()),
                    // Exactly what the pipe grants: GENERIC_WRITE would also
                    // ask to create pipe instances, which the user may not.
                    FILE_GENERIC_READ.0 | FILE_WRITE_DATA.0,
                    FILE_SHARE_NONE,
                    None,
                    OPEN_EXISTING,
                    // The helper learns who connected but can't act as them.
                    SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                    None,
                )
            };
            let Ok(pipe) = pipe else { return false };
            let pipe = Handle(pipe);
            // Only the approved copy may answer: anything else could have
            // opened a pipe of the same name first.
            let mut server = 0;
            let genuine = unsafe { GetNamedPipeServerProcessId(pipe.0, &mut server) }.is_ok()
                && crate::shell::process_path(server)
                    .zip(helper_path())
                    .is_some_and(|(running, approved)| {
                        running.eq_ignore_ascii_case(&approved.to_string_lossy())
                    });
            if !genuine {
                self.failed = Some(ProcessStatus::Unavailable);
                return true;
            }
            let mut hello = [0u8; wire::HELLO_LEN];
            if !client_read(&pipe, &mut hello) {
                self.failed = Some(ProcessStatus::Unavailable);
                return true;
            }
            match wire::read_hello(&hello) {
                (PROTOCOL, Some(ProcessStatus::Ready)) => self.pipe = Some(pipe),
                (PROTOCOL, Some(status)) => self.failed = Some(status),
                // A copy from an older Vorssaint: approving copies this one.
                _ => {
                    self.failed = Some(if can_approve() {
                        ProcessStatus::NeedsApproval
                    } else {
                        ProcessStatus::Unavailable
                    })
                }
            }
            true
        }

        /// The same answer `NetworkTrace::sample` gives, read from the helper.
        pub fn sample(&mut self) -> (ProcessStatus, HashMap<u32, Bytes>, f64) {
            let nothing = |status| (status, HashMap::new(), 0.0);
            if let Some(status) = self.failed {
                return nothing(status);
            }
            let Some(pipe) = &self.pipe else {
                if self.connect() {
                    // A failed helper, or a trace only just started.
                    return nothing(self.failed.unwrap_or(ProcessStatus::Measuring));
                }
                return nothing(if self.started.elapsed() < START {
                    ProcessStatus::Measuring
                } else {
                    ProcessStatus::Unavailable
                });
            };
            match request(pipe) {
                Some(answer) => answer,
                None => {
                    // The helper went away; reopening the detail starts another.
                    self.pipe = None;
                    self.failed = Some(ProcessStatus::Unavailable);
                    nothing(ProcessStatus::Unavailable)
                }
            }
        }
    }

    fn request(pipe: &Handle) -> Option<(ProcessStatus, HashMap<u32, Bytes>, f64)> {
        unsafe { WriteFile(pipe.0, Some(&[wire::SAMPLE]), None, None) }.ok()?;
        let mut head = [0u8; wire::REPLY_HEAD];
        if !client_read(pipe, &mut head) {
            return None;
        }
        let (status, elapsed, count) = wire::read_head(&head)?;
        let mut body = vec![0u8; count * wire::ENTRY_LEN];
        if !client_read(pipe, &mut body) {
            return None;
        }
        let bytes = body
            .as_chunks::<{ wire::ENTRY_LEN }>()
            .0
            .iter()
            .map(wire::read_entry)
            .map(|e| {
                (
                    e.pid,
                    Bytes {
                        down: e.down,
                        up: e.up,
                        first: e.first,
                    },
                )
            })
            .collect();
        Some((status, bytes, elapsed))
    }

    /// Fills `buffer` from the island's (blocking) end of the pipe, giving
    /// up after `REPLY` rather than hanging the sampler on a stuck helper.
    fn client_read(pipe: &Handle, buffer: &mut [u8]) -> bool {
        let deadline = Instant::now() + REPLY;
        let mut filled = 0;
        while filled < buffer.len() {
            let mut available = 0u32;
            if unsafe { PeekNamedPipe(pipe.0, None, 0, None, Some(&mut available), None) }.is_err()
            {
                return false;
            }
            if available == 0 {
                if Instant::now() >= deadline {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            let end = buffer.len().min(filled + available as usize);
            let mut read = 0;
            if unsafe {
                ReadFile(
                    pipe.0,
                    Some(&mut buffer[filled..end]),
                    Some(&mut read),
                    None,
                )
            }
            .is_err()
                || read == 0
            {
                return false;
            }
            filled += read as usize;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_round_trip_and_reject_what_no_helper_sends() {
        let entries = [
            wire::Entry {
                pid: 42,
                down: 1 << 40,
                up: 7,
                first: -3,
            },
            wire::Entry {
                pid: 7,
                down: 0,
                up: u64::MAX,
                first: i64::MAX,
            },
        ];
        let bytes = wire::reply(ProcessStatus::Ready, 1.5, &entries);
        assert_eq!(bytes.len(), wire::REPLY_HEAD + 2 * wire::ENTRY_LEN);
        let head: [u8; wire::REPLY_HEAD] = bytes[..wire::REPLY_HEAD].try_into().unwrap();
        assert_eq!(wire::read_head(&head), Some((ProcessStatus::Ready, 1.5, 2)));
        let read: Vec<_> = bytes[wire::REPLY_HEAD..]
            .as_chunks::<{ wire::ENTRY_LEN }>()
            .0
            .iter()
            .map(wire::read_entry)
            .collect();
        assert_eq!(read, entries);

        let mut bad = head;
        bad[0] = 9;
        assert_eq!(wire::read_head(&bad), None);
        let mut huge = head;
        huge[9..13].copy_from_slice(&(wire::MAX_ENTRIES as u32 + 1).to_le_bytes());
        assert_eq!(wire::read_head(&huge), None);
        // The helper sends no more than the island accepts.
        let many = vec![entries[0]; wire::MAX_ENTRIES + 5];
        let bytes = wire::reply(ProcessStatus::Ready, 1.0, &many);
        assert_eq!(
            bytes.len(),
            wire::REPLY_HEAD + wire::MAX_ENTRIES * wire::ENTRY_LEN
        );
        let head: [u8; wire::REPLY_HEAD] = bytes[..wire::REPLY_HEAD].try_into().unwrap();
        assert_eq!(wire::read_head(&head).unwrap().2, wire::MAX_ENTRIES);
        let nan = wire::reply(ProcessStatus::Ready, f64::NAN, &[]);
        assert_eq!(wire::read_head(&nan[..].try_into().unwrap()), None);
    }

    #[test]
    fn every_status_survives_the_pipe() {
        for status in [
            ProcessStatus::Measuring,
            ProcessStatus::Ready,
            ProcessStatus::Unavailable,
            ProcessStatus::PermissionDenied,
            ProcessStatus::Incomplete,
            ProcessStatus::NeedsApproval,
        ] {
            assert_eq!(
                wire::read_hello(&wire::hello(PROTOCOL, status)),
                (PROTOCOL, Some(status))
            );
        }
        assert_eq!(wire::read_hello(&[1, 0, 0, 0, 200]), (1, None));
    }

    #[test]
    fn the_task_runs_only_the_approved_copy_elevated_and_the_user_cannot_edit_it() {
        let xml = task_xml("S-1-5-21-1", r"C:\Program Files\Vorssaint\A&B.exe");
        assert!(xml.contains("<UserId>S-1-5-21-1</UserId>"));
        assert!(xml.contains("<RunLevel>HighestAvailable</RunLevel>"));
        assert!(xml.contains("<LogonType>InteractiveToken</LogonType>"));
        assert!(xml.contains(r#"<Command>"C:\Program Files\Vorssaint\A&amp;B.exe"</Command>"#));
        assert!(xml.contains("<Arguments>--network-helper</Arguments>"));
        // Read and execute for the user; full control for administrators only.
        assert_eq!(
            task_sddl("S-1-5-21-1"),
            "D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FRFX;;;S-1-5-21-1)"
        );
        assert!(pipe_sddl("S-1-5-21-1").contains("(A;;0x12008b;;;S-1-5-21-1)"));
        assert!(pipe_sddl("S-1-5-21-1").ends_with("S:(ML;;NW;;;ME)"));
        assert_eq!(xml_escape(r#"<a>"&"#), "&lt;a&gt;&quot;&amp;");
        // Users may run the copy's folder, never write to it.
        assert!(FOLDER_SDDL.starts_with("D:P"));
        assert!(FOLDER_SDDL.contains("(A;OICI;0x1200a9;;;BU)"));
        assert_eq!(
            pipe_name("S-1-5-21-1"),
            r"\\.\pipe\vorssaint-network-S-1-5-21-1"
        );
    }
}
