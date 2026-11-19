// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The bot's token in Windows Credential Manager, never in settings.json.
//! Named after the app's identifier, so a test build under another one
//! neither reads nor replaces the real app's.

use windows::Win32::Foundation::FILETIME;
use windows::Win32::Security::Credentials::{
    CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
    CredReadW, CredWriteW,
};
use windows::core::{HSTRING, PWSTR};

fn target(identifier: &str) -> HSTRING {
    HSTRING::from(format!("Openbar Telegram bot ({identifier})"))
}

pub fn read(identifier: &str) -> Option<String> {
    let target = target(identifier);
    unsafe {
        let mut found: *mut CREDENTIALW = std::ptr::null_mut();
        CredReadW(&target, CRED_TYPE_GENERIC, None, &mut found).ok()?;
        let credential = &*found;
        let bytes = std::slice::from_raw_parts(
            credential.CredentialBlob,
            credential.CredentialBlobSize as usize,
        );
        let token = String::from_utf8(bytes.to_vec()).ok();
        CredFree(found as *const _);
        token
    }
}

pub fn write(identifier: &str, token: &str) -> bool {
    let mut name: Vec<u16> = format!("Openbar Telegram bot ({identifier})")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut user: Vec<u16> = "telegram-bot"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut blob = token.as_bytes().to_vec();
    let credential = CREDENTIALW {
        Flags: CRED_FLAGS(0),
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(name.as_mut_ptr()),
        Comment: PWSTR::null(),
        LastWritten: FILETIME::default(),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        AttributeCount: 0,
        Attributes: std::ptr::null_mut(),
        TargetAlias: PWSTR::null(),
        UserName: PWSTR(user.as_mut_ptr()),
    };
    unsafe { CredWriteW(&credential, 0).is_ok() }
}

pub fn delete(identifier: &str) -> bool {
    let target = target(identifier);
    unsafe { CredDeleteW(&target, CRED_TYPE_GENERIC, None).is_ok() }
}
