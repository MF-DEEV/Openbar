// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The ping's one POST on Windows, through WinHTTP like the price list
//! (`agent_usage/prices.rs`): no cookies, no cache, no redirects, 15 s per step.

use super::Target;

pub fn post(target: &Target, agent: &str, body: &[u8]) -> Option<u16> {
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
    let length = u32::try_from(body.len()).ok()?;
    let host = target.host.trim_start_matches('[').trim_end_matches(']');
    unsafe {
        let session = Handle(WinHttpOpen(
            &HSTRING::from(agent),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ));
        if session.0.is_null() {
            return None;
        }
        WinHttpSetTimeouts(session.0, 15_000, 15_000, 15_000, 15_000).ok()?;
        let connect = Handle(WinHttpConnect(
            session.0,
            &HSTRING::from(host),
            target.port,
            0,
        ));
        if connect.0.is_null() {
            return None;
        }
        let secure = if target.secure {
            WINHTTP_FLAG_SECURE
        } else {
            WINHTTP_OPEN_REQUEST_FLAGS(0)
        };
        let request = Handle(WinHttpOpenRequest(
            connect.0,
            &HSTRING::from("POST"),
            &HSTRING::from(target.path.as_str()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            secure | WINHTTP_FLAG_REFRESH,
        ));
        if request.0.is_null() {
            return None;
        }
        let never = WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_le_bytes();
        WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_REDIRECT_POLICY,
            Some(&never),
        )
        .ok()?;
        let no_cookies = WINHTTP_DISABLE_COOKIES.to_le_bytes();
        WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_DISABLE_FEATURE,
            Some(&no_cookies),
        )
        .ok()?;
        let headers: Vec<u16> = "Content-Type: application/json\r\nCache-Control: no-store"
            .encode_utf16()
            .collect();
        WinHttpSendRequest(
            request.0,
            Some(&headers),
            Some(body.as_ptr().cast()),
            length,
            length,
            0,
        )
        .ok()?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).ok()?;
        let mut status = 0u32;
        let mut size = 4u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut size,
            std::ptr::null_mut(),
        )
        .ok()?;
        u16::try_from(status).ok()
    }
}
