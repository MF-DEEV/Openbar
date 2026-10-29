// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! How a Claude Code hook reaches the island (spec 02-sessions §2.4, §6.2.4).
//! On Windows a named pipe only this user may open; on a Mac, where the
//! island is only designed, a Unix socket in a private folder. The hook
//! writes the JSON Claude Code gave it, the island answers or closes, and
//! the hook prints the answer. A message is complete once it parses as a
//! JSON object, because a pipe has no half-close to mark the end.

use std::io::{self, Read, Write};
use std::thread;
use std::time::Duration;

use interprocess::local_socket::{GenericFilePath, ListenerOptions, Stream, ToFsName, prelude::*};

/// A hook payload larger than this is not one Claude Code sends.
pub const MAX_MESSAGE: usize = 4 * 1024 * 1024;

/// Where this user's island listens for app `app`.
pub fn endpoint(app: &str) -> String {
    #[cfg(windows)]
    {
        let user = std::env::var("USERNAME").unwrap_or_default();
        let user: String = user.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        format!(r"\\.\pipe\vorssaint-claude-hooks-{app}-{user}")
    }
    #[cfg(not(windows))]
    {
        private_dir()
            .join(format!("{app}.sock"))
            .to_string_lossy()
            .into_owned()
    }
}

#[cfg(not(windows))]
fn private_dir() -> std::path::PathBuf {
    let base = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join(".vorssaint")
}

/// True once `buf` holds one whole JSON object (ClaudeHookMessage.isComplete).
pub fn is_complete(buf: &[u8]) -> bool {
    let last = buf.iter().rposition(|b| !b.is_ascii_whitespace());
    matches!(last, Some(i) if buf[i] == b'}')
        && serde_json::from_slice::<serde_json::Value>(buf).is_ok()
}

/// One hook connected to the island: what it sent, who sent it, and the way
/// back. Dropping it without replying lets Claude Code use its own flow.
pub struct HookConnection {
    pub payload: Vec<u8>,
    /// The hook process, to notice when Claude Code kills it after the
    /// person answered in the terminal.
    pub peer_pid: Option<u32>,
    stream: Stream,
}

impl HookConnection {
    /// Writes the answer and closes, waiting at most two seconds.
    pub fn reply(mut self, answer: &[u8]) -> io::Result<()> {
        self.stream.write_all(answer)?;
        self.stream.flush()
    }
}

/// Listens until the process ends, handing every complete message to `on_message`.
pub fn serve(
    app: &str,
    on_message: impl Fn(HookConnection) + Send + Sync + 'static,
) -> io::Result<()> {
    let path = endpoint(app);
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let dir = private_dir();
        std::fs::create_dir_all(&dir)?;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
        // A socket left behind by a crash would refuse the new one.
        let _ = std::fs::remove_file(&path);
    }
    let name = path.clone().to_fs_name::<GenericFilePath>()?;
    let options = ListenerOptions::new().name(name);
    #[cfg(windows)]
    let options = {
        use interprocess::os::windows::local_socket::ListenerOptionsExt;
        options.security_descriptor(platform::owner_only()?)
    };
    let listener = options.create_sync()?;
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    let on_message = std::sync::Arc::new(on_message);
    thread::Builder::new()
        .name("claude-hooks".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let on_message = on_message.clone();
                thread::spawn(move || {
                    if let Some(connection) = receive(stream) {
                        on_message(connection);
                    }
                });
            }
        })?;
    Ok(())
}

fn receive(mut stream: Stream) -> Option<HookConnection> {
    let peer_pid = platform::peer_pid(&stream);
    if !platform::same_user(&stream) {
        return None;
    }
    let mut buf = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() > MAX_MESSAGE {
                    return None;
                }
                if is_complete(&buf) {
                    break;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return None,
        }
    }
    is_complete(&buf).then_some(HookConnection {
        payload: buf,
        peer_pid,
        stream,
    })
}

/// The `--hook` side: reads stdin, hands it to the island, prints the
/// answer, and always succeeds so a hook never fails a tool call.
pub fn run_hook(app: &str) -> ! {
    let mut input = Vec::new();
    let _ = io::stdin()
        .take(MAX_MESSAGE as u64 + 1)
        .read_to_end(&mut input);
    if let Some(answer) = ask(app, &input) {
        let mut out = io::stdout().lock();
        let _ = out.write_all(&answer);
        let _ = out.flush();
    }
    std::process::exit(0)
}

fn ask(app: &str, input: &[u8]) -> Option<Vec<u8>> {
    if input.is_empty() || input.len() > MAX_MESSAGE {
        return None;
    }
    let name = endpoint(app).to_fs_name::<GenericFilePath>().ok()?;
    // No island: connect fails at once and Claude Code goes on without us.
    let mut stream = connect(name)?;
    stream.write_all(input).ok()?;
    stream.flush().ok()?;
    let mut answer = Vec::new();
    let _ = stream.read_to_end(&mut answer);
    (!answer.iter().all(u8::is_ascii_whitespace)).then_some(answer)
}

/// A busy pipe (all instances taken) is retried briefly; a missing one is not.
fn connect(name: interprocess::local_socket::Name<'_>) -> Option<Stream> {
    for _ in 0..10 {
        match Stream::connect(name.borrow()) {
            Ok(s) => return Some(s),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock || e.raw_os_error() == Some(231) => {
                thread::sleep(Duration::from_millis(20));
            }
            Err(_) => return None,
        }
    }
    None
}

#[cfg(windows)]
pub use platform::current_user_sid;

#[cfg(windows)]
mod platform {
    use interprocess::local_socket::Stream;
    use interprocess::os::windows::security_descriptor::SecurityDescriptor;
    use std::io;
    use widestring::U16CString;
    use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree};
    use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows::core::PWSTR;

    /// Full access for this user and SYSTEM only; no one else may even connect.
    pub fn owner_only() -> io::Result<SecurityDescriptor> {
        let sid = current_user_sid()?;
        let sddl = U16CString::from_str(format!("D:P(A;;GA;;;{sid})(A;;GA;;;SY)"))
            .map_err(io::Error::other)?;
        SecurityDescriptor::deserialize(&sddl)
    }

    /// "S-1-5-21-…", the account this process runs as.
    pub fn current_user_sid() -> io::Result<String> {
        unsafe {
            let mut token = HANDLE::default();
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
                .map_err(io::Error::other)?;
            let mut len = 0u32;
            let _ = GetTokenInformation(token, TokenUser, None, 0, &mut len);
            let mut buf = vec![0u8; len as usize];
            let result = GetTokenInformation(
                token,
                TokenUser,
                Some(buf.as_mut_ptr().cast()),
                len,
                &mut len,
            );
            let _ = CloseHandle(token);
            result.map_err(io::Error::other)?;
            let user = &*(buf.as_ptr() as *const TOKEN_USER);
            let mut text = PWSTR::null();
            ConvertSidToStringSidW(user.User.Sid, &mut text).map_err(io::Error::other)?;
            let sid = text.to_string().map_err(io::Error::other);
            let _ = LocalFree(Some(HLOCAL(text.0.cast())));
            sid
        }
    }

    pub fn peer_pid(stream: &Stream) -> Option<u32> {
        use interprocess::local_socket::traits::StreamCommon;
        stream.peer_creds().ok().and_then(|c| c.pid())
    }

    /// The pipe's DACL already admits only this user and SYSTEM.
    pub fn same_user(_stream: &Stream) -> bool {
        true
    }
}

#[cfg(not(windows))]
mod platform {
    use interprocess::local_socket::Stream;

    pub fn peer_pid(stream: &Stream) -> Option<u32> {
        use interprocess::local_socket::traits::StreamCommon;
        stream
            .peer_creds()
            .ok()
            .and_then(|c| c.pid())
            .and_then(|p| u32::try_from(p).ok())
    }

    /// The socket is 0600 in a 0700 folder; only this user can reach it.
    pub fn same_user(_stream: &Stream) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completeness_waits_for_a_whole_object() {
        assert!(!is_complete(b""));
        assert!(!is_complete(b"{\"a\":"));
        assert!(!is_complete(b"{\"a\":\"}\""));
        assert!(is_complete(b"{\"a\":1}\n"));
        assert!(!is_complete(b"[1]"));
    }

    #[test]
    fn a_hook_round_trips_through_the_island() {
        let app = format!("test-{}", std::process::id());
        serve(&app, |c| {
            let value: serde_json::Value = serde_json::from_slice(&c.payload).unwrap();
            if value["hook_event_name"] == "PermissionRequest" {
                let _ = c.reply(br#"{"hookSpecificOutput":{"decision":{"behavior":"allow"}}}"#);
            }
        })
        .unwrap();
        let answer = ask(
            &app,
            br#"{"hook_event_name":"PermissionRequest","session_id":"s"}"#,
        )
        .unwrap();
        assert!(String::from_utf8(answer).unwrap().contains("allow"));
        assert_eq!(
            ask(&app, br#"{"hook_event_name":"Stop","session_id":"s"}"#),
            None
        );
        assert_eq!(
            ask("no-island-here", br#"{"hook_event_name":"Stop"}"#),
            None
        );
    }
}
