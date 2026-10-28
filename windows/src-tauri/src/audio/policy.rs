// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Switching the default device. Windows has no documented way, so this uses
//! the undocumented `IPolicyConfig` the Sound control panel itself calls, as
//! EarTrumpet and SoundSwitch do (spec 05 §1.3): CLSID_PolicyConfigClient
//! {870af99c-171d-4f9e-af0d-e63df40c2bc9}, IPolicyConfig
//! {f8679f50-850a-41cf-9c72-430f290290c8}, the Windows 7+ layout, unchanged
//! through Windows 11. The engine probes it once; when creating it fails the
//! island hides the device menus.

// The vtable keeps Windows' method names.
#![allow(non_snake_case)]

use windows::Win32::Media::Audio::{ERole, eCommunications, eConsole, eMultimedia};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::core::{GUID, HRESULT, IUnknown, IUnknown_Vtbl, PCWSTR, interface};

const CLSID_POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

/// Only `SetDefaultEndpoint` is called; the slots before it keep the vtable's
/// order, with pointer arguments left untyped.
#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: IUnknown {
    fn GetMixFormat(&self, device: PCWSTR, format: *mut *mut core::ffi::c_void) -> HRESULT;
    fn GetDeviceFormat(
        &self,
        device: PCWSTR,
        default: i32,
        format: *mut *mut core::ffi::c_void,
    ) -> HRESULT;
    fn ResetDeviceFormat(&self, device: PCWSTR) -> HRESULT;
    fn SetDeviceFormat(
        &self,
        device: PCWSTR,
        endpoint: *mut core::ffi::c_void,
        mix: *mut core::ffi::c_void,
    ) -> HRESULT;
    fn GetProcessingPeriod(
        &self,
        device: PCWSTR,
        default: i32,
        period: *mut i64,
        minimum: *mut i64,
    ) -> HRESULT;
    fn SetProcessingPeriod(&self, device: PCWSTR, period: *mut i64) -> HRESULT;
    fn GetShareMode(&self, device: PCWSTR, mode: *mut core::ffi::c_void) -> HRESULT;
    fn SetShareMode(&self, device: PCWSTR, mode: *mut core::ffi::c_void) -> HRESULT;
    fn GetPropertyValue(
        &self,
        device: PCWSTR,
        key: *const core::ffi::c_void,
        value: *mut core::ffi::c_void,
    ) -> HRESULT;
    fn SetPropertyValue(
        &self,
        device: PCWSTR,
        key: *const core::ffi::c_void,
        value: *mut core::ffi::c_void,
    ) -> HRESULT;
    fn SetDefaultEndpoint(&self, device: PCWSTR, role: ERole) -> HRESULT;
    fn SetEndpointVisibility(&self, device: PCWSTR, visible: i32) -> HRESULT;
}

pub struct Policy(IPolicyConfig);

impl Policy {
    /// None when the class or the interface is missing (a future Windows).
    pub fn probe() -> Option<Self> {
        unsafe {
            CoCreateInstance::<_, IPolicyConfig>(&CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)
        }
        .ok()
        .map(Policy)
    }

    /// Makes `id` (an `IMMDevice::GetId` string) the default for every role.
    pub fn set_default(&self, id: &str) -> Result<(), String> {
        let wide: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
        for role in [eConsole, eMultimedia, eCommunications] {
            unsafe { self.0.SetDefaultEndpoint(PCWSTR(wide.as_ptr()), role) }
                .ok()
                .map_err(|e| e.message())?;
        }
        Ok(())
    }
}
