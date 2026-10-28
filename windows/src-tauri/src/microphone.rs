// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Read-only audio metering and Discord accessibility. Never opens a recording
//! stream, changes Discord, reads messages, or assumes WASAPI mute = Discord mute.
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordState {
    muted: bool,
    deafened: bool,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    recording: bool,
    system_muted: bool,
    level: f32,
    discord: Option<DiscordState>,
}

#[derive(Clone, Default)]
pub struct Shared(Arc<Mutex<Snapshot>>);

#[tauri::command]
pub fn microphone_snapshot(state: tauri::State<'_, Shared>) -> Snapshot {
    state.0.lock().unwrap().clone()
}

pub fn probe_main(path: &str) -> ! {
    let mut snapshot = Snapshot::default();
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
        if CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() {
            {
                let mut reader = imp::Reader::default();
                reader.refresh();
                reader.audio(&mut snapshot);
                snapshot.discord = reader.read_discord();
            }
            CoUninitialize();
        }
    }
    let result = serde_json::to_vec_pretty(&snapshot)
        .map_err(std::io::Error::other)
        .and_then(|bytes| std::fs::write(path, bytes));
    std::process::exit(if result.is_ok() { 0 } else { 1 });
}

// Names describe the action. Checked/pressed describes the current state.
// Unknown or incomplete controls must never be interpreted as unmuted.
fn control(name: &str, checked: Option<bool>) -> Option<(bool, bool)> {
    let name = name.to_lowercase();
    let name = name.split(['(', '\n']).next()?.trim();
    let (headphones, muted) = match name {
        "mute" | "wycisz" | "wycisz mikrofon" => (false, false),
        "unmute" | "wyłącz wyciszenie" | "wyłącz wyciszenie mikrofonu" => (false, true),
        "deafen" | "wyłącz dźwięk" | "wycisz dźwięk" | "wycisz słuchawki" => (true, false),
        "undeafen"
        | "włącz dźwięk"
        | "wyłącz wyciszenie dźwięku"
        | "wyłącz wyciszenie słuchawek" => (true, true),
        _ => return None,
    };
    Some((headphones, checked.unwrap_or(muted)))
}

pub fn start(app: AppHandle, shared: Shared) {
    #[cfg(windows)]
    std::thread::spawn(move || unsafe {
        use std::time::{Duration, Instant};
        use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
        if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
            return;
        }
        struct ComGuard;
        impl Drop for ComGuard {
            fn drop(&mut self) {
                unsafe {
                    CoUninitialize();
                }
            }
        }
        let _com = ComGuard;
        {
            let mut reader = imp::Reader::default();
            let mut refreshed = Instant::now() - Duration::from_secs(5);
            let mut discord_read = Instant::now() - Duration::from_secs(5);
            loop {
                let prefs = app.state::<crate::prefs::Prefs>().get();
                let enabled = prefs
                    .pointer("/island/microphoneStatus")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true)
                    && prefs
                        .pointer("/island/enabled")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                let mut next = Snapshot::default();
                if enabled {
                    if refreshed.elapsed() >= Duration::from_secs(2) {
                        reader.refresh();
                        refreshed = Instant::now();
                    }
                    reader.audio(&mut next);
                    if discord_read.elapsed() >= Duration::from_millis(200) {
                        reader.discord = reader.read_discord();
                        discord_read = Instant::now();
                    }
                    next.discord = reader.discord;
                } else {
                    reader = imp::Reader::default();
                    refreshed = Instant::now() - Duration::from_secs(5);
                }
                let changed = {
                    let mut current = shared.0.lock().unwrap();
                    if *current == next {
                        false
                    } else {
                        *current = next.clone();
                        true
                    }
                };
                if changed {
                    let _ = app.emit_to(crate::island::LABEL, "microphone://snapshot", &next);
                }
                std::thread::sleep(Duration::from_millis(if enabled { 100 } else { 1000 }));
            }
        }
    });
    #[cfg(not(windows))]
    let _ = (app, shared);
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows::Win32::Media::Audio::{
        AudioSessionStateActive, DEVICE_STATE_ACTIVE,
        Endpoints::{IAudioEndpointVolume, IAudioMeterInformation},
        IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, eCapture,
    };
    use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::Accessibility::*;
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId, IsIconic,
        IsWindow, IsWindowVisible,
    };
    use windows::core::{BOOL, Interface, PWSTR};

    #[derive(Default)]
    pub struct Reader {
        inputs: Vec<IMMDevice>,
        automation: Option<IUIAutomation>,
        storage: crate::discord_storage::Reader,
        bindings: crate::discord_storage::Reader<Vec<crate::discord_keys::Binding>>,
        monitor: crate::discord_keys::Monitor,
        bindings_refreshed: Option<std::time::Instant>,
        profile: Option<&'static str>,
        last_ui: Option<DiscordState>,
        controls: Option<(HWND, IUIAutomationElement, IUIAutomationElement)>,
        pub discord: Option<DiscordState>,
    }

    impl Reader {
        pub unsafe fn refresh(&mut self) {
            unsafe {
                self.inputs.clear();
                if let Ok(enumerator) = CoCreateInstance::<_, IMMDeviceEnumerator>(
                    &MMDeviceEnumerator,
                    None,
                    CLSCTX_ALL,
                ) && let Ok(devices) =
                    enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)
                {
                    for i in 0..devices.GetCount().unwrap_or(0) {
                        if let Ok(device) = devices.Item(i) {
                            self.inputs.push(device);
                        }
                    }
                }
            }
        }

        pub unsafe fn audio(&self, next: &mut Snapshot) {
            unsafe {
                let mut active = 0;
                let mut muted = 0;
                for device in &self.inputs {
                    let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None)
                    else {
                        continue;
                    };
                    let Ok(sessions) = manager.GetSessionEnumerator() else {
                        continue;
                    };
                    let recording = (0..sessions.GetCount().unwrap_or(0)).any(|i| {
                        sessions
                            .GetSession(i)
                            .and_then(|s| s.GetState())
                            .is_ok_and(|s| s == AudioSessionStateActive)
                    });
                    if !recording {
                        continue;
                    }
                    active += 1;
                    let is_muted = device
                        .Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
                        .and_then(|v| v.GetMute())
                        .is_ok_and(|v| v.as_bool());
                    if is_muted {
                        muted += 1;
                        continue;
                    }
                    if let Ok(meter) = device.Activate::<IAudioMeterInformation>(CLSCTX_ALL, None) {
                        let level = meter.GetPeakValue().unwrap_or(0.0);
                        if level.is_finite() {
                            next.level = next.level.max(level.clamp(0.0, 1.0));
                        }
                    }
                }
                next.recording = active > 0;
                next.system_muted = active > 0 && active == muted;
                next.level = (next.level * 100.0).round() / 100.0;
            }
        }

        pub unsafe fn read_discord(&mut self) -> Option<DiscordState> {
            unsafe {
                let mut handles = Vec::<(HWND, &'static str)>::new();
                let _ = EnumWindows(
                    Some(discord_window),
                    LPARAM(&mut handles as *mut _ as isize),
                );
                if handles.is_empty() {
                    self.controls = None;
                    self.monitor.active(false);
                    self.profile = None;
                    self.last_ui = None;
                    return None;
                }
                let profile = handles[0].1;
                if self.profile != Some(profile) {
                    self.monitor.active(false);
                    self.profile = Some(profile);
                    self.bindings_refreshed = None;
                    self.last_ui = None;
                }
                self.monitor.active(true);
                // Live controls have priority. Saved mute/deafen settings can
                // stay unchanged after a click and must NEVER override the UI.
                if let Some(state) = self.read_visible_discord(&handles) {
                    let foreground = GetForegroundWindow();
                    let discord_focused = handles.iter().any(|(window, _)| *window == foreground);
                    if (discord_focused
                        || self.last_ui != Some(state)
                        || self.monitor.state().is_none())
                        && self.monitor.live(crate::discord_storage::VoiceState {
                            muted: state.muted,
                            deafened: state.deafened,
                        })
                    {
                        self.last_ui = Some(state);
                    }
                }
                if let Some(root) = std::env::var_os("APPDATA") {
                    let directory = std::path::PathBuf::from(root)
                        .join(profile)
                        .join("Local Storage")
                        .join("leveldb");
                    if self
                        .bindings_refreshed
                        .is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(2))
                    {
                        self.monitor
                            .bindings(self.bindings.bindings(&directory).unwrap_or_default());
                        self.bindings_refreshed = Some(std::time::Instant::now());
                    }
                    // A startup seed only; a persisted snapshot is not a live
                    // signal and cannot undo a click or a configured shortcut.
                    if self.monitor.state().is_none()
                        && let Some(state) = self.storage.read(&directory)
                    {
                        self.monitor.seed(state);
                    }
                }
                self.monitor.state().map(|state| DiscordState {
                    muted: state.muted,
                    deafened: state.deafened,
                })
            }
        }

        unsafe fn read_visible_discord(
            &mut self,
            handles: &[(HWND, &'static str)],
        ) -> Option<DiscordState> {
            unsafe {
                // Keep provider references while Discord is in the background. Do
                // not retain a state after controls disappear or a process exits.
                if let Some((window, mic, headphones)) = &self.controls
                    && IsWindow(Some(*window)).as_bool()
                    && !IsIconic(*window).as_bool()
                    && IsWindowVisible(*window).as_bool()
                    && let (Some((false, muted)), Some((true, deafened))) =
                        (read_control(mic), read_control(headphones))
                {
                    return Some(DiscordState {
                        muted: muted || deafened,
                        deafened,
                    });
                }
                self.controls = None;
                if self.automation.is_none() {
                    let automation: IUIAutomation =
                        CoCreateInstance(&CUIAutomation8, None, CLSCTX_ALL).ok()?;
                    if let Ok(v2) = automation.cast::<IUIAutomation2>() {
                        let _ = v2.SetConnectionTimeout(300);
                        let _ = v2.SetTransactionTimeout(300);
                    }
                    self.automation = Some(automation);
                }
                let automation = self.automation.as_ref()?;
                // The accessibility fallback is only reliable for visible windows.
                for &(handle, _) in handles {
                    if IsIconic(handle).as_bool() || !IsWindowVisible(handle).as_bool() {
                        continue;
                    }
                    let Ok(root) = automation.ElementFromHandle(handle) else {
                        continue;
                    };
                    let buttons = automation
                        .CreatePropertyCondition(
                            UIA_ControlTypePropertyId,
                            &UIA_ButtonControlTypeId.0.into(),
                        )
                        .ok()?;
                    let checkboxes = automation
                        .CreatePropertyCondition(
                            UIA_ControlTypePropertyId,
                            &UIA_CheckBoxControlTypeId.0.into(),
                        )
                        .ok()?;
                    let condition = automation.CreateOrCondition(&buttons, &checkboxes).ok()?;
                    let Ok(elements) = root.FindAll(TreeScope_Descendants, &condition) else {
                        continue;
                    };
                    let mut mic = None;
                    let mut headphones = None;
                    for i in 0..elements.Length().unwrap_or(0) {
                        let Ok(element) = elements.GetElement(i) else {
                            continue;
                        };
                        let role = element.CurrentControlType().ok();
                        if role != Some(UIA_ButtonControlTypeId)
                            && role != Some(UIA_CheckBoxControlTypeId)
                        {
                            continue;
                        }
                        if let Some((is_headphones, muted)) = read_control(&element) {
                            if is_headphones {
                                headphones = Some((muted, element));
                            } else {
                                mic = Some((muted, element));
                            }
                        }
                    }
                    if let (Some((muted, mic)), Some((deafened, headphones))) = (mic, headphones) {
                        self.controls = Some((handle, mic, headphones));
                        return Some(DiscordState {
                            muted: muted || deafened,
                            deafened,
                        });
                    }
                }
                None
            }
        }
    }

    unsafe fn read_control(element: &IUIAutomationElement) -> Option<(bool, bool)> {
        unsafe {
            let name = element.CurrentName().ok()?.to_string();
            control(&name, None)?;
            let aria = element
                .CurrentAriaProperties()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let checked = aria
                .split(';')
                .find_map(|p| match p.trim() {
                    "checked=true" | "pressed=true" => Some(true),
                    "checked=false" | "pressed=false" => Some(false),
                    _ => None,
                })
                .or_else(|| {
                    element
                        .GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId)
                        .and_then(|p| p.CurrentToggleState())
                        .ok()
                        .and_then(|s| {
                            if s == ToggleState_On {
                                Some(true)
                            } else if s == ToggleState_Off {
                                Some(false)
                            } else {
                                None
                            }
                        })
                });
            control(&name, checked)
        }
    }

    unsafe extern "system" fn discord_window(window: HWND, data: LPARAM) -> BOOL {
        unsafe {
            let mut class = [0u16; 256];
            let length = GetClassNameW(window, &mut class);
            if !String::from_utf16_lossy(&class[..length.max(0) as usize])
                .starts_with("Chrome_WidgetWin_")
            {
                return BOOL(1);
            }
            let mut pid = 0;
            GetWindowThreadProcessId(window, Some(&mut pid));
            if let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                let mut buffer = [0u16; 32768];
                let mut len = buffer.len() as u32;
                let result = QueryFullProcessImageNameW(
                    process,
                    PROCESS_NAME_WIN32,
                    PWSTR(buffer.as_mut_ptr()),
                    &mut len,
                );
                let _ = CloseHandle(process);
                if result.is_ok() {
                    let path = String::from_utf16_lossy(&buffer[..len as usize]);
                    let filename = path.rsplit('\\').next().unwrap_or("").to_ascii_lowercase();
                    let profile = match filename.as_str() {
                        "discord.exe" => Some("discord"),
                        "discordcanary.exe" => Some("discordcanary"),
                        "discordptb.exe" => Some("discordptb"),
                        _ => None,
                    };
                    if let Some(profile) = profile {
                        (*(data.0 as *mut Vec<(HWND, &'static str)>)).push((window, profile));
                    }
                }
            }
            BOOL(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn action_labels_and_explicit_checked_states() {
        assert_eq!(control("Mute", None), Some((false, false)));
        assert_eq!(control("Unmute (Ctrl+Shift+M)", None), Some((false, true)));
        assert_eq!(control("Wyłącz wyciszenie", None), Some((false, true)));
        assert_eq!(
            control("Wyłącz wyciszenie dźwięku", None),
            Some((true, true))
        );
        assert_eq!(control("Wyłącz dźwięk", Some(false)), Some((true, false)));
        assert_eq!(control("Deafen", Some(true)), Some((true, true)));
        assert_eq!(control("Mute", Some(true)), Some((false, true)));
        assert_eq!(control("Unmute", Some(false)), Some((false, false)));
        assert_eq!(control("Mute channel", None), None);
        assert_eq!(control("", None), None);
    }
}
