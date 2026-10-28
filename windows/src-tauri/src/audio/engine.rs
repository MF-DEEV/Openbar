// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The Windows side of `audio`: every interface lives on the audio worker
//! (an MTA thread) inside `ENGINE`. Device, endpoint-volume and new-session
//! callbacks arrive on the system's threads and only queue a reload there.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, FILETIME, PROPERTYKEY};
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
    IAudioMeterInformation,
};
use windows::Win32::Media::Audio::{
    AUDIO_VOLUME_NOTIFICATION_DATA, AudioSessionStateActive, AudioSessionStateExpired,
    DEVICE_STATE, DEVICE_STATE_ACTIVE, EDataFlow, ERole, IAudioSessionControl,
    IAudioSessionControl2, IAudioSessionManager2, IAudioSessionNotification,
    IAudioSessionNotification_Impl, IMMDevice, IMMDeviceEnumerator, IMMNotificationClient,
    IMMNotificationClient_Impl, ISimpleAudioVolume, MMDeviceEnumerator, eCapture, eConsole,
    eMultimedia, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, STGM_READ};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::core::{GUID, Interface, PCWSTR, PWSTR, Ref, implement};

use super::guard::{self, Guard};
use super::mic::MicMute;
use super::policy::Policy;
use super::priority::{self, Saved, Tracker};
use super::roller::{self, Direction};
use super::switcher;
use super::tree::{Proc, root};
use super::{App, Device, Level, Priority, Ranked, Reading, State};
use crate::com::Worker;

/// Marks the island's own level changes, so the endpoint callback can tell
/// them from the volume keys and other apps (spec 05 §1.3).
const OWN_CHANGE: GUID = GUID::from_u128(0x6f1c9b52_4a0e_4c39_9b5d_2c7a1e8d0f41);
/// Level changes from the finer volume keys: not the island's own, so its
/// volume notice shows them as it shows the keys' usual steps.
const ROLLER_CHANGE: GUID = GUID::from_u128(0x2b7e5d1a_93c4_4f6e_8a21_5c0d9e3f7b64);
const FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
    pid: 14,
};
/// PKEY_Device_EnumeratorName: the bus ("HDAUDIO", "USB", "BTHENUM", "ROOT"…).
const ENUMERATOR_NAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
    pid: 24,
};
/// PKEY_AudioEndpoint_FormFactor (EndpointFormFactor: 3 headphones, 5 headset…).
const FORM_FACTOR: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x1da5d803_d492_4edd_8c23_e0c0ffee7f0e),
    pid: 0,
};
/// A peak this long ago still counts as playing, so the dot doesn't blink between notes.
const PLAYING_HOLD: Duration = Duration::from_millis(1500);
const SYSTEM_KEY: &str = "system";
const MIC_FILE: &str = "mic-mute.json";
const PRIORITY_FILE: &str = "audio-priority.json";

fn worker() -> &'static Worker {
    static W: OnceLock<Worker> = OnceLock::new();
    W.get_or_init(|| Worker::spawn("vorssaint-audio", false))
}

thread_local! {
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
}

static DEVICES_DIRTY: AtomicBool = AtomicBool::new(false);
static SESSIONS_DIRTY: AtomicBool = AtomicBool::new(false);
static WATCHING: AtomicBool = AtomicBool::new(false);
static POLL_QUEUED: AtomicBool = AtomicBool::new(false);
static TICKER: OnceLock<std::thread::Thread> = OnceLock::new();

/// Runs `job` on the audio thread, then publishes what changed.
pub fn with_engine<T: Send + 'static>(
    job: impl FnOnce(&mut Engine) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    worker()
        .run(move || with(job))
        .unwrap_or_else(|| Err("The audio thread stopped".into()))
}

fn with<T>(job: impl FnOnce(&mut Engine) -> Result<T, String>) -> Result<T, String> {
    ENGINE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(Engine::new()?);
        }
        let engine = slot.as_mut().unwrap();
        if DEVICES_DIRTY.swap(false, Ordering::SeqCst) {
            engine.reload_devices();
        }
        if SESSIONS_DIRTY.swap(false, Ordering::SeqCst) {
            engine.reload_sessions();
        }
        let result = job(engine);
        super::publish(engine.state());
        result
    })
}

/// Runs `job` on the audio thread only if the engine already exists, and
/// waits; for quitting, which must not start one.
pub fn with_existing(job: impl FnOnce(&mut Engine) + Send + 'static) {
    let _ = worker().run(move || {
        ENGINE.with(|cell| {
            if let Some(engine) = cell.borrow_mut().as_mut() {
                job(engine);
            }
        })
    });
}

/// Runs `job` on the audio thread without waiting, then publishes.
pub fn post(job: impl FnOnce(&mut Engine) + Send + 'static) {
    worker().post(move || {
        let _ = with(|e| {
            job(e);
            Ok(())
        });
    });
}

/// Runs `job` on the audio thread after `ms`, for the priority passes.
fn after(ms: u64, job: impl FnOnce(&mut Engine) + Send + 'static) {
    let _ = std::thread::Builder::new()
        .name("vorssaint-audio-later".into())
        .spawn(move || {
            std::thread::sleep(Duration::from_millis(ms));
            post(job);
        });
}

/// Queues a reload from a callback thread.
fn request(devices: bool) {
    let flag = if devices {
        &DEVICES_DIRTY
    } else {
        &SESSIONS_DIRTY
    };
    if !flag.swap(true, Ordering::SeqCst) {
        worker().post(|| {
            let _ = with(|_| Ok(()));
        });
    }
}

pub fn start() {
    worker().post(|| {
        if let Err(e) = with(|_| Ok(())) {
            eprintln!("audio: {e}");
        }
    });
    // Four readings a second while the mixer page shows; parked otherwise.
    let ticker = std::thread::Builder::new()
        .name("vorssaint-audio-meter".into())
        .spawn(|| {
            loop {
                if !WATCHING.load(Ordering::SeqCst) {
                    std::thread::park();
                    continue;
                }
                if !POLL_QUEUED.swap(true, Ordering::SeqCst) {
                    worker().post(|| {
                        POLL_QUEUED.store(false, Ordering::SeqCst);
                        let _ = with(|e| {
                            e.poll();
                            Ok(())
                        });
                    });
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        });
    if let Ok(handle) = ticker {
        let _ = TICKER.set(handle.thread().clone());
    }
}

// MARK: Callbacks

#[implement(IMMNotificationClient)]
struct DeviceEvents;

impl IMMNotificationClient_Impl for DeviceEvents_Impl {
    fn OnDeviceStateChanged(
        &self,
        _id: &PCWSTR,
        _state: DEVICE_STATE,
    ) -> windows::core::Result<()> {
        request(true);
        Ok(())
    }
    fn OnDeviceAdded(&self, _id: &PCWSTR) -> windows::core::Result<()> {
        request(true);
        Ok(())
    }
    fn OnDeviceRemoved(&self, _id: &PCWSTR) -> windows::core::Result<()> {
        request(true);
        Ok(())
    }
    fn OnDefaultDeviceChanged(
        &self,
        _flow: EDataFlow,
        _role: ERole,
        _id: &PCWSTR,
    ) -> windows::core::Result<()> {
        request(true);
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _id: &PCWSTR, key: &PROPERTYKEY) -> windows::core::Result<()> {
        // Renaming a device in Sound settings; the rest changes too often to follow.
        if *key == FRIENDLY_NAME {
            request(true);
        }
        Ok(())
    }
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeEvents {
    output: bool,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeEvents_Impl {
    fn OnNotify(&self, data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        if let Some(data) = unsafe { data.as_ref() }
            && self.output
            && data.guidEventContext != OWN_CHANGE
        {
            super::level_from_outside(data.fMasterVolume, data.bMuted.as_bool());
        }
        worker().post(|| {
            let _ = with(|_| Ok(()));
        });
        Ok(())
    }
}

#[implement(IAudioSessionNotification)]
struct SessionEvents;

impl IAudioSessionNotification_Impl for SessionEvents_Impl {
    fn OnSessionCreated(&self, _session: Ref<IAudioSessionControl>) -> windows::core::Result<()> {
        request(false);
        Ok(())
    }
}

// MARK: Engine

/// The default device's level, with its change callback registered.
struct Endpoint {
    volume: IAudioEndpointVolume,
    callback: IAudioEndpointVolumeCallback,
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        let _ = unsafe { self.volume.UnregisterControlChangeNotify(&self.callback) };
    }
}

/// One output's session list, told about new sessions.
struct Manager {
    manager: IAudioSessionManager2,
    events: IAudioSessionNotification,
}

impl Drop for Manager {
    fn drop(&mut self) {
        let _ = unsafe { self.manager.UnregisterSessionNotification(&self.events) };
    }
}

struct Session {
    control: IAudioSessionControl2,
    simple: ISimpleAudioVolume,
    meter: Option<IAudioMeterInformation>,
    owner: Owner,
    /// The session's own exe name without ".exe", for the Music page's player match.
    exe_stem: String,
}

/// The app a process's sound belongs to, cached by PID and creation time.
#[derive(Clone)]
struct Owner {
    key: String,
    path: Option<String>,
    name: String,
    system: bool,
}

pub struct Engine {
    enumerator: IMMDeviceEnumerator,
    client: IMMNotificationClient,
    policy: Option<Policy>,
    output: Option<Endpoint>,
    input: Option<Endpoint>,
    outputs: Vec<Device>,
    inputs: Vec<Device>,
    managers: HashMap<String, Manager>,
    sessions: Vec<Session>,
    owners: HashMap<u32, (u64, Owner)>,
    names: HashMap<String, String>,
    sounding: HashMap<String, Instant>,
    mic: MicMute,
    watching: bool,
    priority: Saved,
    output_tracker: Tracker,
    input_tracker: Tracker,
    /// The newest scheduled pass of each kind; older ones find it moved on.
    enforce_generation: u64,
    placement_generation: u64,
    guard: Guard,
    switch_failed: bool,
    /// Why the last automatic switch of each list failed (the Mac's
    /// outputSwitchError and the microphone's lastError).
    priority_error: Option<String>,
    input_priority_error: Option<String>,
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = unsafe {
            self.enumerator
                .UnregisterEndpointNotificationCallback(&self.client)
        };
    }
}

impl Engine {
    fn new() -> Result<Self, String> {
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
                .map_err(|e| e.message())?;
        let client: IMMNotificationClient = DeviceEvents.into();
        unsafe { enumerator.RegisterEndpointNotificationCallback(&client) }
            .map_err(|e| e.message())?;
        let mut engine = Engine {
            enumerator,
            client,
            policy: Policy::probe(),
            output: None,
            input: None,
            outputs: Vec::new(),
            inputs: Vec::new(),
            managers: HashMap::new(),
            sessions: Vec::new(),
            owners: HashMap::new(),
            names: HashMap::new(),
            sounding: HashMap::new(),
            mic: load_mic(),
            watching: false,
            priority: load_priority(),
            output_tracker: Tracker::default(),
            input_tracker: Tracker::default(),
            enforce_generation: 0,
            placement_generation: 0,
            guard: Guard::default(),
            switch_failed: false,
            priority_error: None,
            input_priority_error: None,
        };
        // Restored while muted: the microphones present now were present
        // before, so one unmuted by hand meanwhile stays as it is.
        if engine.mic.muted {
            engine.mic.seen = engine
                .devices(eCapture, eConsole)
                .into_iter()
                .map(|d| d.id)
                .collect();
        }
        engine.reload_devices();
        super::switching_probed(engine.policy.is_some());
        Ok(engine)
    }

    /// Devices, defaults and their callbacks, then the sessions on them.
    fn reload_devices(&mut self) {
        self.outputs = self.devices(eRender, eMultimedia);
        self.inputs = self.devices(eCapture, eConsole);
        let output = self
            .outputs
            .iter()
            .find(|d| d.is_default)
            .map(|d| d.id.clone());
        let input = self
            .inputs
            .iter()
            .find(|d| d.is_default)
            .map(|d| d.id.clone());
        // Rebuilt on every device event: a device unplugged and back under the
        // same id leaves the old interfaces dead.
        self.output = None;
        self.input = None;
        self.managers.clear();
        self.output = output.and_then(|id| self.endpoint(&id, true));
        self.input = input.and_then(|id| self.endpoint(&id, false));
        // A microphone plugged in while Mic Mute is on is muted too; one
        // unplugged while it was turned off is unmuted on its return.
        let arrivals = self.mic.arrivals(&self.input_mutes());
        for id in &arrivals {
            let _ = self.set_device_mute(id, true);
        }
        let present: Vec<String> = self.inputs.iter().map(|d| d.id.clone()).collect();
        let owed = self.mic.owed(&present);
        for id in &owed {
            if self.set_device_mute(id, false).is_ok() {
                self.mic.ours.remove(id);
            }
        }
        if !arrivals.is_empty() || !owed.is_empty() {
            save_mic(&self.mic);
        }
        self.guard_headphones();
        self.observe_priority();
        self.reload_sessions();
    }

    fn devices(&self, flow: EDataFlow, role: ERole) -> Vec<Device> {
        let default = unsafe { self.enumerator.GetDefaultAudioEndpoint(flow, role) }
            .ok()
            .and_then(|d| device_id(&d));
        let Ok(collection) = (unsafe {
            self.enumerator
                .EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)
        }) else {
            return Vec::new();
        };
        let count = unsafe { collection.GetCount() }.unwrap_or(0);
        (0..count)
            .filter_map(|i| unsafe { collection.Item(i) }.ok())
            .filter_map(|device| {
                let id = device_id(&device)?;
                let name = friendly_name(&device).unwrap_or_else(|| id.clone());
                let enumerator = string_property(&device, &ENUMERATOR_NAME).unwrap_or_default();
                Some(Device {
                    headphones: flow == eRender
                        && guard::looks_like_headphones(&name, form_factor(&device)),
                    tier: Some(priority::tier(&enumerator, &name)),
                    is_default: Some(&id) == default.as_ref(),
                    name,
                    id,
                })
            })
            .collect()
    }

    fn endpoint(&self, id: &str, output: bool) -> Option<Endpoint> {
        let volume = self.endpoint_volume(id)?;
        let callback: IAudioEndpointVolumeCallback = VolumeEvents { output }.into();
        unsafe { volume.RegisterControlChangeNotify(&callback) }.ok()?;
        Some(Endpoint { volume, callback })
    }

    fn endpoint_volume(&self, id: &str) -> Option<IAudioEndpointVolume> {
        let wide: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
        let device = unsafe { self.enumerator.GetDevice(PCWSTR(wide.as_ptr())) }.ok()?;
        unsafe { device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) }.ok()
    }

    /// Every active microphone and whether it is muted now.
    fn input_mutes(&self) -> Vec<(String, bool)> {
        self.inputs
            .iter()
            .map(|d| {
                let muted = self
                    .endpoint_volume(&d.id)
                    .and_then(|v| unsafe { v.GetMute() }.ok())
                    .is_some_and(|m| m.as_bool());
                (d.id.clone(), muted)
            })
            .collect()
    }

    fn set_device_mute(&self, id: &str, muted: bool) -> Result<(), String> {
        let volume = self.endpoint_volume(id).ok_or("Microphone unavailable")?;
        unsafe { volume.SetMute(muted, &OWN_CHANGE) }.map_err(|e| e.message())
    }

    /// Session lists on every active output. GetSessionEnumerator and
    /// GetCount come before registering, or Windows may never call back
    /// (Microsoft Learn's documented quirk; spec 05 §2.4).
    fn reload_sessions(&mut self) {
        let ids: Vec<String> = self.outputs.iter().map(|d| d.id.clone()).collect();
        self.managers.retain(|id, _| ids.contains(id));
        let mut sessions = Vec::new();
        let mut snapshot: Option<HashMap<u32, (u32, String)>> = None;
        for id in &ids {
            if !self.managers.contains_key(id)
                && let Some(manager) = self.manager(id)
            {
                self.managers.insert(id.clone(), manager);
            }
            let Some(manager) = self.managers.get(id) else {
                continue;
            };
            let Ok(list) = (unsafe { manager.manager.GetSessionEnumerator() }) else {
                continue;
            };
            for i in 0..unsafe { list.GetCount() }.unwrap_or(0) {
                let Ok(control) = (unsafe { list.GetSession(i) }) else {
                    continue;
                };
                if unsafe { control.GetState() }.is_ok_and(|s| s == AudioSessionStateExpired) {
                    continue;
                }
                let Ok(control) = control.cast::<IAudioSessionControl2>() else {
                    continue;
                };
                let Ok(simple) = control.cast::<ISimpleAudioVolume>() else {
                    continue;
                };
                let meter = control.cast::<IAudioMeterInformation>().ok();
                let pid = unsafe { control.GetProcessId() }.unwrap_or(0);
                let system = unsafe { control.IsSystemSoundsSession() }.0 == 0;
                let snapshot = snapshot.get_or_insert_with(process_snapshot);
                let owner = self.owner(pid, system, snapshot);
                let exe_stem = snapshot
                    .get(&pid)
                    .map(|(_, exe)| stem(exe))
                    .unwrap_or_default();
                sessions.push(Session {
                    control,
                    simple,
                    meter,
                    owner,
                    exe_stem,
                });
            }
        }
        self.sessions = sessions;
    }

    fn manager(&self, id: &str) -> Option<Manager> {
        let wide: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
        let device = unsafe { self.enumerator.GetDevice(PCWSTR(wide.as_ptr())) }.ok()?;
        let manager = unsafe { device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) }.ok()?;
        let list = unsafe { manager.GetSessionEnumerator() }.ok()?;
        let _ = unsafe { list.GetCount() };
        let events: IAudioSessionNotification = SessionEvents.into();
        unsafe { manager.RegisterSessionNotification(&events) }.ok()?;
        Some(Manager { manager, events })
    }

    /// The app a session belongs to, after the process-tree roll-up.
    fn owner(&mut self, pid: u32, system: bool, snapshot: &HashMap<u32, (u32, String)>) -> Owner {
        if system || pid == 0 {
            return Owner {
                key: SYSTEM_KEY.into(),
                path: None,
                name: "System sounds".into(),
                system: true,
            };
        }
        let created = creation_time(pid);
        if let Some((at, owner)) = self.owners.get(&pid)
            && *at == created
        {
            return owner.clone();
        }
        // The chain of ancestors, each with its path and start time.
        let mut procs = HashMap::new();
        let mut current = pid;
        for _ in 0..16 {
            let Some((parent, _)) = snapshot.get(&current) else {
                break;
            };
            let exe = crate::shell::process_path(current).unwrap_or_default();
            procs.insert(
                current,
                Proc {
                    parent: *parent,
                    exe,
                    created: creation_time(current),
                },
            );
            if procs.contains_key(parent) {
                break;
            }
            current = *parent;
        }
        let windows_dir = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let top = root(pid, &procs, &windows_dir);
        let path = procs
            .get(&top)
            .map(|p| p.exe.clone())
            .filter(|p| !p.is_empty());
        let owner = match path {
            Some(path) => {
                let name = self
                    .names
                    .entry(path.clone())
                    .or_insert_with(|| crate::shell::app_name(&path))
                    .clone();
                Owner {
                    key: path.to_lowercase(),
                    path: Some(path),
                    name,
                    system: false,
                }
            }
            // An elevated process hides its path; its exe name still shows.
            None => {
                let exe = snapshot
                    .get(&pid)
                    .map(|(_, e)| e.clone())
                    .unwrap_or_else(|| format!("{pid}"));
                Owner {
                    key: exe.to_lowercase(),
                    path: None,
                    name: stem(&exe),
                    system: false,
                }
            }
        };
        // A process that can't be opened has no start time to tell a reused PID by.
        if created == 0 {
            return owner;
        }
        if self.owners.len() > 512 {
            self.owners.clear();
        }
        self.owners.insert(pid, (created, owner.clone()));
        owner
    }

    /// The mixer's readings: state and level of every session, and its peak
    /// while the page shows.
    fn readings(&mut self) -> Vec<Reading> {
        let now = Instant::now();
        let mut readings = Vec::with_capacity(self.sessions.len());
        for s in &self.sessions {
            let Ok(state) = (unsafe { s.control.GetState() }) else {
                continue;
            };
            if state == AudioSessionStateExpired {
                continue;
            }
            let active = state == AudioSessionStateActive;
            let playing = if self.watching {
                let peak = s
                    .meter
                    .as_ref()
                    .and_then(|m| unsafe { m.GetPeakValue() }.ok())
                    .unwrap_or(0.0);
                if active && peak > 0.0001 {
                    self.sounding.insert(s.owner.key.clone(), now);
                }
                active
                    && self
                        .sounding
                        .get(&s.owner.key)
                        .is_some_and(|t| now.duration_since(*t) < PLAYING_HOLD)
            } else {
                active
            };
            readings.push(Reading {
                key: s.owner.key.clone(),
                path: s.owner.path.clone(),
                name: s.owner.name.clone(),
                system: s.owner.system,
                volume: unsafe { s.simple.GetMasterVolume() }.unwrap_or(1.0),
                muted: unsafe { s.simple.GetMute() }.is_ok_and(|m| m.as_bool()),
                playing,
            });
        }
        readings
    }

    fn level(endpoint: Option<&Endpoint>) -> Level {
        let Some(e) = endpoint else {
            return Level::default();
        };
        Level {
            volume: unsafe { e.volume.GetMasterVolumeLevelScalar() }
                .ok()
                .filter(|v| v.is_finite()),
            muted: unsafe { e.volume.GetMute() }.ok().map(|m| m.as_bool()),
        }
    }

    fn state(&mut self) -> State {
        let apps: Vec<App> = super::group(&self.readings());
        State {
            supported: true,
            output: Self::level(self.output.as_ref()),
            input: Self::level(self.input.as_ref()),
            outputs: self.outputs.clone(),
            inputs: self.inputs.clone(),
            can_switch: self.policy.is_some(),
            mic_muted: self.mic.muted,
            apps,
            priority: self.priority_view(),
            switcher_refused: super::SWITCHER_REFUSED.load(Ordering::SeqCst),
            switch_failed: self.switch_failed,
            priority_error: self.priority_error.clone(),
            input_priority_error: self.input_priority_error.clone(),
            roller_failed: super::ROLLER_FAILED.load(Ordering::SeqCst),
        }
    }

    // MARK: Headphone guard

    /// Lowers the speakers Windows fell back to when headphones left, or
    /// gives them their level back when headphones are the output again.
    fn guard_headphones(&mut self) {
        let options = super::options();
        let outputs: Vec<guard::Output> = self
            .outputs
            .iter()
            .map(|d| guard::Output {
                id: &d.id,
                headphones: d.headphones,
            })
            .collect();
        let default = self
            .outputs
            .iter()
            .find(|d| d.is_default)
            .map(|d| d.id.clone());
        let action = self.guard.observe(
            &outputs,
            default.as_deref(),
            options.lower_on_disconnect,
            options.lower_percent,
        );
        match action {
            guard::Action::None => {}
            guard::Action::Lower { id, volume, mute } => {
                let Some(endpoint) = self.endpoint_volume(&id) else {
                    return;
                };
                let previous = unsafe { endpoint.GetMasterVolumeLevelScalar() }.ok();
                // Not the island's own context: its volume notice shows the lowered level.
                if unsafe { endpoint.SetMasterVolumeLevelScalar(volume, &GUID::zeroed()) }.is_err()
                {
                    return;
                }
                // Only a mute the output didn't have is this guard's to undo.
                let muted = mute
                    && unsafe { endpoint.GetMute() }.is_ok_and(|m| !m.as_bool())
                    && unsafe { endpoint.SetMute(true, &GUID::zeroed()) }.is_ok();
                self.guard.lowered(&id, previous, volume, muted);
            }
            guard::Action::Restore(lowered) => self.restore(&lowered),
        }
    }

    // MARK: Audio priority

    fn current_id(devices: &[Device]) -> Option<String> {
        devices.iter().find(|d| d.is_default).map(|d| d.id.clone())
    }

    fn tier_of(devices: &[Device], id: &str) -> priority::Tier {
        devices
            .iter()
            .find(|d| d.id == id)
            .and_then(|d| d.tier)
            .unwrap_or(priority::Tier::Hardware)
    }

    /// A fresh inventory: newcomers wait for their place, a device that
    /// came or went owes a pass, and the names of listed ones are kept.
    fn observe_priority(&mut self) {
        let outputs: Vec<String> = self.outputs.iter().map(|d| d.id.clone()).collect();
        let inputs: Vec<String> = self.inputs.iter().map(|d| d.id.clone()).collect();
        let changed_out = self
            .output_tracker
            .observe(&outputs.iter().cloned().collect());
        let changed_in = self
            .input_tracker
            .observe(&inputs.iter().cloned().collect());
        let before = self.priority.clone();
        let (current_out, current_in) = (
            Self::current_id(&self.outputs),
            Self::current_id(&self.inputs),
        );
        let devices = self.outputs.clone();
        let waiting_out = priority::merge(
            &mut self.priority.outputs,
            &mut self.output_tracker,
            &outputs,
            current_out.as_deref(),
            |id| Self::tier_of(&devices, id),
        );
        let devices = self.inputs.clone();
        let waiting_in = priority::merge(
            &mut self.priority.inputs,
            &mut self.input_tracker,
            &inputs,
            current_in.as_deref(),
            |id| Self::tier_of(&devices, id),
        );
        self.remember_names();
        if self.priority != before {
            save_priority(&self.priority);
        }
        if waiting_out || waiting_in {
            self.placement_generation += 1;
            let generation = self.placement_generation;
            after(priority::PLACEMENT_DELAY_MS, move |e| {
                if e.placement_generation == generation {
                    e.place_new_devices();
                }
            });
        }
        if changed_out || changed_in {
            self.schedule_enforcement();
        }
    }

    fn remember_names(&mut self) {
        let listed: std::collections::HashSet<&String> = self
            .priority
            .outputs
            .iter()
            .chain(&self.priority.inputs)
            .collect();
        for d in self.outputs.iter().chain(&self.inputs) {
            if listed.contains(&d.id) {
                self.priority.names.insert(d.id.clone(), d.name.clone());
            }
        }
        self.priority.names.retain(|id, _| listed.contains(id));
    }

    fn place_new_devices(&mut self) {
        let before = self.priority.clone();
        let outputs: Vec<String> = self.outputs.iter().map(|d| d.id.clone()).collect();
        let inputs: Vec<String> = self.inputs.iter().map(|d| d.id.clone()).collect();
        let (current_out, current_in) = (
            Self::current_id(&self.outputs),
            Self::current_id(&self.inputs),
        );
        let devices = self.outputs.clone();
        priority::place(
            &mut self.priority.outputs,
            &mut self.output_tracker,
            &outputs,
            current_out.as_deref(),
            |id| Self::tier_of(&devices, id),
        );
        let devices = self.inputs.clone();
        priority::place(
            &mut self.priority.inputs,
            &mut self.input_tracker,
            &inputs,
            current_in.as_deref(),
            |id| Self::tier_of(&devices, id),
        );
        self.remember_names();
        if self.priority != before {
            save_priority(&self.priority);
        }
    }

    fn schedule_enforcement(&mut self) {
        self.enforce_generation += 1;
        let generation = self.enforce_generation;
        after(priority::ENFORCE_DELAY_MS, move |e| {
            if e.enforce_generation == generation {
                e.enforce_priority();
            }
        });
    }

    /// The owed passes: the best connected device of a list that is on
    /// becomes the default.
    fn enforce_priority(&mut self) {
        let mut switched = false;
        for output in [true, false] {
            let (tracker, devices, list, enabled) = if output {
                (
                    &mut self.output_tracker,
                    &self.outputs,
                    &self.priority.outputs,
                    self.priority.output_enabled,
                )
            } else {
                (
                    &mut self.input_tracker,
                    &self.inputs,
                    &self.priority.inputs,
                    self.priority.input_enabled,
                )
            };
            if !tracker.settle() || !enabled {
                continue;
            }
            let available = devices.iter().map(|d| d.id.clone()).collect();
            let current = Self::current_id(devices);
            let Some(target) = priority::target(list, tracker, &available, current.as_deref())
            else {
                continue;
            };
            // A failed switch says why, as the Mac's outputSwitchError does.
            let result = self.policy.as_ref().map(|p| p.set_default(target));
            let error = if output {
                &mut self.priority_error
            } else {
                &mut self.input_priority_error
            };
            match result {
                Some(Ok(())) => {
                    switched = true;
                    *error = None;
                }
                Some(Err(e)) => *error = Some(e),
                None => {}
            }
        }
        if switched {
            self.reload_devices();
        }
    }

    fn priority_view(&self) -> Priority {
        let ranked = |list: &[String], devices: &[Device]| -> Vec<Ranked> {
            list.iter()
                .map(|id| {
                    let device = devices.iter().find(|d| &d.id == id);
                    Ranked {
                        id: id.clone(),
                        name: device
                            .map(|d| d.name.clone())
                            .or_else(|| self.priority.names.get(id).cloned())
                            .unwrap_or_else(|| id.clone()),
                        available: device.is_some(),
                    }
                })
                .collect()
        };
        Priority {
            output_enabled: self.priority.output_enabled,
            input_enabled: self.priority.input_enabled,
            outputs: ranked(&self.priority.outputs, &self.outputs),
            inputs: ranked(&self.priority.inputs, &self.inputs),
        }
    }

    /// A switch turned on, or a new order: a pass follows at once.
    pub fn set_priority(&mut self, output: bool, enabled: Option<bool>, ids: Option<Vec<String>>) {
        let (list, on, tracker) = if output {
            (
                &mut self.priority.outputs,
                &mut self.priority.output_enabled,
                &mut self.output_tracker,
            )
        } else {
            (
                &mut self.priority.inputs,
                &mut self.priority.input_enabled,
                &mut self.input_tracker,
            )
        };
        let mut due = false;
        if let Some(enabled) = enabled {
            due |= enabled && !*on;
            *on = enabled;
            if !enabled {
                if output {
                    self.priority_error = None;
                } else {
                    self.input_priority_error = None;
                }
            }
        }
        if let Some(ids) = ids {
            let ids = priority::sanitized_list(&ids);
            if ids != *list {
                *list = ids;
                due |= *on;
            }
        }
        tracker.due |= due;
        self.remember_names();
        save_priority(&self.priority);
        if due {
            self.schedule_enforcement();
        }
    }

    // MARK: Output switcher

    pub fn switch_to_next(&mut self, cycle: &[String]) {
        let available = self.outputs.iter().map(|d| d.id.clone()).collect();
        let current = Self::current_id(&self.outputs);
        let ok = match switcher::next(current.as_deref(), cycle, &available) {
            Some(next) => self
                .policy
                .as_ref()
                .is_some_and(|p| p.set_default(next).is_ok()),
            None => switcher::nothing_to_do_is_fine(cycle, &available),
        };
        self.switch_failed = !ok;
        if ok {
            self.priority_error = None;
            self.reload_devices();
        }
    }

    pub fn clear_switch_failure(&mut self) {
        self.switch_failed = false;
    }

    /// The app is quitting: speakers the guard lowered get their level back
    /// while it is still the one the guard set (AppVolumeMixer.stopAll).
    pub fn restore_lowered(&mut self) {
        let Some(lowered) = self.guard.lowered.take() else {
            return;
        };
        self.restore(&lowered);
    }

    fn restore(&self, lowered: &guard::Lowered) {
        let Some(endpoint) = self.endpoint_volume(&lowered.id) else {
            return;
        };
        let current = unsafe { endpoint.GetMasterVolumeLevelScalar() }.ok();
        if !guard::should_restore(lowered, current) {
            return;
        }
        let _ = unsafe { endpoint.SetMasterVolumeLevelScalar(lowered.previous, &OWN_CHANGE) };
        // A mute switched off since was someone's choice, and one the guard
        // didn't set was never its to undo.
        if lowered.muted_by_lowering && unsafe { endpoint.GetMute() }.is_ok_and(|m| m.as_bool()) {
            let _ = unsafe { endpoint.SetMute(false, &OWN_CHANGE) };
        }
    }

    // MARK: Finer volume keys

    /// One 1% step of the default output; like the volume keys, it unmutes.
    pub fn step_output(&mut self, direction: Direction) {
        let Some(endpoint) = self.output.as_ref() else {
            return;
        };
        unsafe {
            let Ok(current) = endpoint.volume.GetMasterVolumeLevelScalar() else {
                return;
            };
            let _ = endpoint
                .volume
                .SetMasterVolumeLevelScalar(roller::stepped(current, direction), &ROLLER_CHANGE);
            if endpoint.volume.GetMute().is_ok_and(|m| m.as_bool()) {
                let _ = endpoint.volume.SetMute(false, &ROLLER_CHANGE);
            }
        }
    }

    /// A tick while the mixer shows; an expired session means the list changed.
    fn poll(&mut self) {
        let expired = self.sessions.iter().any(|s| {
            unsafe { s.control.GetState() }.map_or(true, |state| state == AudioSessionStateExpired)
        });
        if expired {
            self.reload_sessions();
        }
    }

    pub fn watch(&mut self, on: bool) {
        self.watching = on;
        self.sounding.clear();
        WATCHING.store(on, Ordering::SeqCst);
        if on {
            self.reload_sessions();
            if let Some(ticker) = TICKER.get() {
                ticker.unpark();
            }
        }
    }

    /// The default output's (or microphone's) level; a level above zero unmutes.
    pub fn set_level(
        &mut self,
        input: bool,
        volume: Option<f32>,
        muted: Option<bool>,
    ) -> Result<(), String> {
        let endpoint = if input {
            self.input.as_ref()
        } else {
            self.output.as_ref()
        };
        let endpoint = endpoint.ok_or(if input {
            "Microphone unavailable"
        } else {
            "Output unavailable"
        })?;
        unsafe {
            if let Some(v) = volume {
                endpoint
                    .volume
                    .SetMasterVolumeLevelScalar(v, &OWN_CHANGE)
                    .map_err(|e| e.message())?;
                if v > 0.0 && muted.is_none() && !(input && self.mic.muted) {
                    endpoint
                        .volume
                        .SetMute(false, &OWN_CHANGE)
                        .map_err(|e| e.message())?;
                }
            }
            if let Some(m) = muted {
                endpoint
                    .volume
                    .SetMute(m, &OWN_CHANGE)
                    .map_err(|e| e.message())?;
            }
        }
        Ok(())
    }

    pub fn set_default(&mut self, id: &str) -> Result<(), String> {
        let policy = self
            .policy
            .as_ref()
            .ok_or("Switching devices is unavailable")?;
        policy.set_default(id)?;
        // A device picked by hand answers the last automatic failure.
        if self.outputs.iter().any(|d| d.id == id) {
            self.priority_error = None;
        } else {
            self.input_priority_error = None;
        }
        self.reload_devices();
        Ok(())
    }

    /// Every session of the app; a level above zero unmutes it.
    pub fn set_app(
        &mut self,
        key: &str,
        volume: Option<f32>,
        muted: Option<bool>,
    ) -> Result<(), String> {
        let (mut done, mut error) = (false, None);
        let live = self.sessions.iter().filter(|s| {
            s.owner.key == key
                && unsafe { s.control.GetState() }
                    .is_ok_and(|state| state != AudioSessionStateExpired)
        });
        // One session gone bad doesn't stop the app's others.
        for s in live {
            let result = unsafe {
                volume
                    .map_or(Ok(()), |v| {
                        s.simple.SetMasterVolume(v, &OWN_CHANGE).and_then(|_| {
                            if v > 0.0 && muted.is_none() {
                                s.simple.SetMute(false, &OWN_CHANGE)
                            } else {
                                Ok(())
                            }
                        })
                    })
                    .and_then(|_| muted.map_or(Ok(()), |m| s.simple.SetMute(m, &OWN_CHANGE)))
            };
            match result {
                Ok(()) => done = true,
                Err(e) => error = Some(e.message()),
            }
        }
        if done {
            Ok(())
        } else {
            Err(error.unwrap_or_else(|| "The app has no audio session".into()))
        }
    }

    pub fn set_mic_mute(&mut self, muted: Option<bool>) -> Result<bool, String> {
        let target = muted.unwrap_or(!self.mic.muted);
        if target == self.mic.muted {
            return Ok(target);
        }
        let ids = if target {
            self.mic.mute(&self.input_mutes())
        } else {
            self.mic.unmute()
        };
        for id in &ids {
            // One unplugged meanwhile stays owed its unmute until it returns.
            if self.set_device_mute(id, target).is_err() && !target {
                self.mic.ours.insert(id.clone());
            }
        }
        save_mic(&self.mic);
        Ok(target)
    }

    /// The Music page's player volume, through the same session list.
    pub fn player_volume(
        &mut self,
        matches: &dyn Fn(&str) -> bool,
        level: Option<f32>,
    ) -> Result<f32, String> {
        self.reload_sessions();
        let mut result: Option<f32> = None;
        for s in self
            .sessions
            .iter()
            .filter(|s| !s.exe_stem.is_empty() && matches(&s.exe_stem))
        {
            unsafe {
                if let Some(level) = level {
                    s.simple
                        .SetMasterVolume(level, &OWN_CHANGE)
                        .map_err(|e| e.message())?;
                    if level > 0.0 {
                        s.simple
                            .SetMute(false, &OWN_CHANGE)
                            .map_err(|e| e.message())?;
                    }
                }
                let value = if s.simple.GetMute().is_ok_and(|m| m.as_bool()) {
                    0.0
                } else {
                    s.simple.GetMasterVolume().map_err(|e| e.message())?
                };
                result = Some(result.map_or(value, |current| current.max(value)));
            }
        }
        result.ok_or_else(|| "No audio session for this player".into())
    }
}

// MARK: Helpers

fn device_id(device: &IMMDevice) -> Option<String> {
    unsafe {
        let raw: PWSTR = device.GetId().ok()?;
        let id = raw.to_string().ok();
        CoTaskMemFree(Some(raw.0 as _));
        id
    }
}

fn friendly_name(device: &IMMDevice) -> Option<String> {
    string_property(device, &FRIENDLY_NAME)
}

fn string_property(device: &IMMDevice, key: &PROPERTYKEY) -> Option<String> {
    unsafe {
        let store = device.OpenPropertyStore(STGM_READ).ok()?;
        let value = store.GetValue(key).ok()?;
        let text = value.to_string();
        (!text.is_empty()).then_some(text)
    }
}

fn form_factor(device: &IMMDevice) -> Option<u32> {
    unsafe {
        let store = device.OpenPropertyStore(STGM_READ).ok()?;
        let value = store.GetValue(&FORM_FACTOR).ok()?;
        u32::try_from(&value).ok()
    }
}

/// PID → (parent PID, exe file name) for every running process.
fn process_snapshot() -> HashMap<u32, (u32, String)> {
    let mut map = HashMap::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return map;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut ok = Process32FirstW(snapshot, &mut entry).is_ok();
        while ok {
            let end = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            map.insert(
                entry.th32ProcessID,
                (
                    entry.th32ParentProcessID,
                    String::from_utf16_lossy(&entry.szExeFile[..end]),
                ),
            );
            ok = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
    map
}

/// When the process started, in FILETIME ticks; 0 when it can't be opened.
fn creation_time(pid: u32) -> u64 {
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return 0;
        };
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        let ok =
            GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user).is_ok();
        let _ = CloseHandle(process);
        if ok {
            (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime)
        } else {
            0
        }
    }
}

fn stem(exe: &str) -> String {
    let name = exe.rsplit(['\\', '/']).next().unwrap_or(exe);
    name.strip_suffix(".exe")
        .or_else(|| name.strip_suffix(".EXE"))
        .unwrap_or(name)
        .to_string()
}

fn load_priority() -> Saved {
    super::saved_file(PRIORITY_FILE)
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .map(|value| Saved::from_value(&value))
        .unwrap_or_default()
}

fn save_priority(saved: &Saved) {
    write_file(PRIORITY_FILE, saved);
}

fn load_mic() -> MicMute {
    super::saved_file(MIC_FILE)
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_mic(mic: &MicMute) {
    write_file(MIC_FILE, mic);
}

fn write_file(name: &str, value: &impl serde::Serialize) {
    let Some(path) = super::saved_file(name) else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(bytes) = serde_json::to_vec(value) {
        let temp = path.with_extension("json.tmp");
        if std::fs::write(&temp, bytes).is_ok() {
            let _ = std::fs::rename(&temp, &path);
        }
    }
}
