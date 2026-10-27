// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The daily price-list download (AgentPriceSource.swift; spec 03 §6.1):
//! one HTTPS GET of the same public file the Mac reads, through WinHTTP so
//! no TLS stack ships with the app. No cookies, no cache, 15 s per step,
//! 256 KiB at most, status 200 only, and no redirects.

use super::pricing::PriceList;
use std::path::PathBuf;

pub const HOST: &str = "raw.githubusercontent.com";
pub const PATH: &str = "/vorssaint/vorssaint-utils/main/Resources/agent-prices.json";
/// Once a day; six hours after a failure.
pub const INTERVAL: f64 = 24.0 * 3600.0;
pub const RETRY: f64 = 6.0 * 3600.0;

/// `%APPDATA%\com.vorssaint.island\agent-prices.json`.
pub fn cache_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("com.vorssaint.island").join("agent-prices.json"))
}

pub fn cached() -> Option<PriceList> {
    PriceList::decode(&std::fs::read(cache_path()?).ok()?)
}

/// Downloads, validates and caches the list; None when any step fails.
pub fn download() -> Option<PriceList> {
    let bytes = get()?;
    let list = PriceList::decode(&bytes)?;
    let path = cache_path()?;
    std::fs::create_dir_all(path.parent()?).ok()?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, &bytes).ok()?;
    std::fs::rename(&temporary, &path).ok()?;
    Some(list)
}

#[cfg(windows)]
fn get() -> Option<Vec<u8>> {
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
    let agent = HSTRING::from(format!("Vorssaint/{}", env!("CARGO_PKG_VERSION")));
    unsafe {
        let session = Handle(WinHttpOpen(&agent, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0));
        if session.0.is_null() {
            return None;
        }
        WinHttpSetTimeouts(session.0, 15_000, 15_000, 15_000, 15_000).ok()?;
        let connect = Handle(WinHttpConnect(session.0, &HSTRING::from(HOST), INTERNET_DEFAULT_HTTPS_PORT, 0));
        if connect.0.is_null() {
            return None;
        }
        let request = Handle(WinHttpOpenRequest(connect.0, &HSTRING::from("GET"), &HSTRING::from(PATH), PCWSTR::null(), PCWSTR::null(), std::ptr::null(), WINHTTP_FLAG_SECURE | WINHTTP_FLAG_REFRESH));
        if request.0.is_null() {
            return None;
        }
        let never = WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_le_bytes();
        WinHttpSetOption(Some(request.0), WINHTTP_OPTION_REDIRECT_POLICY, Some(&never)).ok()?;
        let no_cookies = WINHTTP_DISABLE_COOKIES.to_le_bytes();
        WinHttpSetOption(Some(request.0), WINHTTP_OPTION_DISABLE_FEATURE, Some(&no_cookies)).ok()?;
        WinHttpSendRequest(request.0, None, None, 0, 0, 0).ok()?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).ok()?;
        let mut status = 0u32;
        let mut size = 4u32;
        WinHttpQueryHeaders(request.0, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, PCWSTR::null(), Some((&mut status as *mut u32).cast()), &mut size, std::ptr::null_mut()).ok()?;
        if status != 200 {
            return None;
        }
        let mut body = Vec::new();
        let mut buffer = vec![0u8; 16 << 10];
        loop {
            let mut read = 0u32;
            WinHttpReadData(request.0, buffer.as_mut_ptr().cast(), buffer.len() as u32, &mut read).ok()?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&buffer[..read as usize]);
            if body.len() > super::pricing::MAXIMUM_SIZE {
                return None;
            }
        }
        Some(body)
    }
}

#[cfg(not(windows))]
fn get() -> Option<Vec<u8>> {
    None
}
