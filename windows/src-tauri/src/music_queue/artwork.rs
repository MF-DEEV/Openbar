// SPDX-License-Identifier: GPL-3.0-or-later
//! Public cover metadata only: fixed HTTPS host, no login, cookies or redirects.

#[tauri::command]
pub async fn music_queue_artwork(
    title: String,
    artist: String,
) -> Result<serde_json::Value, String> {
    if title.trim().is_empty()
        || artist.trim().is_empty()
        || title.len() > 4096
        || artist.len() > 4096
    {
        return Err("Invalid song metadata".into());
    }
    tauri::async_runtime::spawn_blocking(move || search(&title, &artist))
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Cover metadata unavailable".into())
}

#[tauri::command]
pub async fn music_queue_album(album_id: u64) -> Result<serde_json::Value, String> {
    if album_id == 0 {
        return Err("Invalid release".into());
    }
    tauri::async_runtime::spawn_blocking(move || get(&format!("/album/{album_id}")))
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Release metadata unavailable".into())
}

fn search(title: &str, artist: &str) -> Option<serde_json::Value> {
    let term: String = format!("{} {}", title, artist).chars().take(500).collect();
    let encoded: String = term
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect();
    get(&format!("/search?q={encoded}&limit=12"))
}

#[cfg(windows)]
fn get(path: &str) -> Option<serde_json::Value> {
    use windows::Win32::Networking::WinHttp::*;
    use windows::core::{HSTRING, PCWSTR};
    struct Handle(*mut core::ffi::c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    let _ = WinHttpCloseHandle(self.0);
                }
            }
        }
    }
    let path = HSTRING::from(path);
    unsafe {
        let session = Handle(WinHttpOpen(
            &HSTRING::from(concat!("Openbar/", env!("CARGO_PKG_VERSION"))),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ));
        if session.0.is_null() {
            return None;
        }
        WinHttpSetTimeouts(session.0, 5000, 5000, 5000, 5000).ok()?;
        let connect = Handle(WinHttpConnect(
            session.0,
            &HSTRING::from("api.deezer.com"),
            INTERNET_DEFAULT_HTTPS_PORT,
            0,
        ));
        if connect.0.is_null() {
            return None;
        }
        let request = Handle(WinHttpOpenRequest(
            connect.0,
            &HSTRING::from("GET"),
            &path,
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ));
        if request.0.is_null() {
            return None;
        }
        WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_REDIRECT_POLICY,
            Some(&WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_le_bytes()),
        )
        .ok()?;
        WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_DISABLE_FEATURE,
            Some(&WINHTTP_DISABLE_COOKIES.to_le_bytes()),
        )
        .ok()?;
        WinHttpSendRequest(request.0, None, None, 0, 0, 0).ok()?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).ok()?;
        let mut status = 0_u32;
        let mut size = 4_u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut size,
            std::ptr::null_mut(),
        )
        .ok()?;
        if status != 200 {
            return None;
        }
        let mut body = Vec::new();
        let mut buffer = [0_u8; 8192];
        loop {
            let mut count = 0;
            WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut count,
            )
            .ok()?;
            if count == 0 {
                break;
            }
            body.extend_from_slice(&buffer[..count as usize]);
            if body.len() > 512_000 {
                return None;
            }
        }
        serde_json::from_slice(&body).ok()
    }
}

#[cfg(not(windows))]
fn get(_path: &str) -> Option<serde_json::Value> {
    None
}
