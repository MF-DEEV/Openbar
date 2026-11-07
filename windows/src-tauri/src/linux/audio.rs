// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! System audio on Linux, where Windows has `engine.rs` over WASAPI: the same
//! engine over the sound server (PulseAudio, or PipeWire's pulse server, see
//! `pulse.rs`). `audio/mod.rs` includes this file as `engine`, since it uses
//! what that module keeps private.
//!
//! One worker thread owns the engine, and what the server reports (a level, a
//! device, an app starting to play) queues a reload onto it. An app is
//! "playing" while its stream is not paused, which is what the server knows
//! (Windows reads a peak meter).

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::guard::{self, Guard};
use super::mic::MicMute;
use super::priority::{self, Saved, Tracker};
use super::roller::{self, Direction};
use super::switcher;
use super::{Device, Level, Options, Priority, Ranked, Reading, State};
use crate::com::Worker;
use crate::linux::pulse::{self, Kind, Node, Pactl, Pulse, Stream};

const SYSTEM_KEY: &str = "system";
const MIC_FILE: &str = "mic-mute.json";
const PRIORITY_FILE: &str = "audio-priority.json";
/// A level that moves this soon after the island moved it is the echo of
/// that, not someone else's change.
const ECHO: Duration = Duration::from_millis(1200);

fn worker() -> &'static Worker {
    static W: OnceLock<Worker> = OnceLock::new();
    W.get_or_init(|| Worker::spawn("openbar-audio", false))
}

thread_local! {
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
}

static DEVICES_DIRTY: AtomicBool = AtomicBool::new(false);
static SESSIONS_DIRTY: AtomicBool = AtomicBool::new(false);

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
        for (delay, pass) in engine.later.drain(..) {
            after(delay, pass);
        }
        for (volume, muted) in engine.notices.drain(..) {
            super::level_from_outside(volume, muted);
        }
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

/// Runs a priority pass on the audio thread after `ms`.
fn after(ms: u64, pass: Pass) {
    let _ = std::thread::Builder::new()
        .name("openbar-audio-later".into())
        .spawn(move || {
            std::thread::sleep(Duration::from_millis(ms));
            post(move |e| e.run(pass));
        });
}

/// Queues a reload from the thread that listens to the server.
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
    pulse::subscribe(|event| match event {
        pulse::Event::Devices => request(true),
        pulse::Event::Streams => request(false),
        pulse::Event::Recordings => {}
    });
}

/// A priority pass the engine owes, run after a delay if no newer one was
/// asked for meanwhile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pass {
    Place(u64),
    Enforce(u64),
}

/// One stream and the app it belongs to.
#[derive(Clone, Debug)]
struct Session {
    stream: Stream,
    key: String,
    name: String,
    system: bool,
}

impl Session {
    fn new(stream: Stream) -> Self {
        let system = stream.role == "event";
        let (key, name) = if system {
            (SYSTEM_KEY.to_string(), "System sounds".to_string())
        } else {
            let name = if stream.app.is_empty() {
                stream.binary.clone()
            } else {
                stream.app.clone()
            };
            let key = if stream.binary.is_empty() {
                stream.app.to_lowercase()
            } else {
                stream.binary.clone()
            };
            (key, name)
        };
        Session {
            stream,
            key,
            name,
            system,
        }
    }

    /// What `shell_icon` draws for the app: its icon's name in the theme.
    fn icon(&self) -> Option<String> {
        let icon = if self.stream.icon.is_empty() {
            &self.stream.binary
        } else {
            &self.stream.icon
        };
        (!self.system && !icon.is_empty()).then(|| format!("icon:{icon}"))
    }
}

pub struct Engine {
    pulse: Box<dyn Pulse>,
    outputs: Vec<Device>,
    inputs: Vec<Device>,
    output_nodes: Vec<Node>,
    input_nodes: Vec<Node>,
    sessions: Vec<Session>,
    mic: MicMute,
    priority: Saved,
    output_tracker: Tracker,
    input_tracker: Tracker,
    /// The newest scheduled pass of each kind; older ones find it moved on.
    enforce_generation: u64,
    placement_generation: u64,
    guard: Guard,
    switch_failed: bool,
    priority_error: Option<String>,
    input_priority_error: Option<String>,
    /// Passes to schedule once the job in hand is done.
    later: Vec<(u64, Pass)>,
    /// Level changes to tell the island about once the job is done.
    notices: Vec<(f32, bool)>,
    /// Until when a change of the output's level is the island's own.
    echo_until: Option<Instant>,
    /// The default output's level when last looked at, to tell a change.
    known: Option<(String, f32, bool)>,
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

/// The Windows endpoint form factor a server's says, for `guard`.
fn form_factor(name: &str) -> Option<u32> {
    match name {
        "headphone" => Some(3),
        "headset" | "hands-free" => Some(5),
        "handset" => Some(6),
        _ => None,
    }
}

fn device(node: &Node, default: Option<&str>, output: bool) -> Device {
    Device {
        headphones: output
            && guard::looks_like_headphones(&node.name, form_factor(&node.form_factor)),
        // A device with no bus is one the server made up: a mixer, an equalizer.
        tier: Some(priority::tier(
            if node.bus.is_empty() {
                "ROOT"
            } else {
                &node.bus
            },
            &node.name,
        )),
        is_default: default == Some(node.id.as_str()),
        name: node.name.clone(),
        id: node.id.clone(),
    }
}

/// How much of a level the island shows: the server can go above 100%.
fn shown(volume: f32) -> f32 {
    volume.clamp(0.0, 1.0)
}

fn level(node: Option<&Node>) -> Level {
    node.map_or_else(Level::default, |n| Level {
        volume: Some(shown(n.volume)),
        muted: Some(n.muted),
    })
}

/// Whether a stream's program is the player an MPRIS player goes by: its
/// desktop entry (`org.gnome.Rhythmbox3`) names the program (`rhythmbox`)
/// only in part.
pub fn same_player(source: &str, program: &str) -> bool {
    let source = source.to_lowercase();
    !program.is_empty()
        && (source == program
            || source.split(['.', '-', '_']).any(|part| part == program)
            || (program.len() >= 5 && source.contains(program)))
}

impl Engine {
    fn new() -> Result<Self, String> {
        if !Pactl::default().available() {
            return Err(
                "pactl (PulseAudio 16 or later, or PipeWire's pulse server) is needed for sound"
                    .into(),
            );
        }
        let mut engine =
            Engine::with_backend(Box::new(Pactl::default()), load_mic(), load_priority());
        engine.reload_devices();
        engine.reload_sessions();
        super::switching_probed(true);
        Ok(engine)
    }

    fn with_backend(pulse: Box<dyn Pulse>, mic: MicMute, priority: Saved) -> Self {
        let mut engine = Engine {
            pulse,
            outputs: Vec::new(),
            inputs: Vec::new(),
            output_nodes: Vec::new(),
            input_nodes: Vec::new(),
            sessions: Vec::new(),
            mic,
            priority,
            output_tracker: Tracker::default(),
            input_tracker: Tracker::default(),
            enforce_generation: 0,
            placement_generation: 0,
            guard: Guard::default(),
            switch_failed: false,
            priority_error: None,
            input_priority_error: None,
            later: Vec::new(),
            notices: Vec::new(),
            echo_until: None,
            known: None,
        };
        // Restored while muted: the microphones present now were present
        // before, so one unmuted by hand meanwhile stays as it is.
        if engine.mic.muted {
            engine.mic.seen = engine.engine_inputs().into_iter().map(|n| n.id).collect();
        }
        engine
    }

    fn engine_inputs(&self) -> Vec<Node> {
        self.pulse.inputs().unwrap_or_default()
    }

    fn default_output(&self) -> Option<&Node> {
        let id = self.outputs.iter().find(|d| d.is_default)?.id.as_str();
        self.output_nodes.iter().find(|n| n.id == id)
    }

    fn default_input(&self) -> Option<&Node> {
        let id = self.inputs.iter().find(|d| d.is_default)?.id.as_str();
        self.input_nodes.iter().find(|n| n.id == id)
    }

    /// Devices, defaults and levels, from the server.
    fn reload_devices(&mut self) {
        self.reload_devices_with(&super::options());
    }

    fn reload_devices_with(&mut self, options: &Options) {
        let (Some((default_out, default_in)), Some(outputs), Some(inputs)) = (
            self.pulse.defaults(),
            self.pulse.outputs(),
            self.pulse.inputs(),
        ) else {
            // The server did not answer (it may be restarting): keep what is known.
            return;
        };
        self.outputs = outputs
            .iter()
            .map(|n| device(n, default_out.as_deref(), true))
            .collect();
        self.inputs = inputs
            .iter()
            .map(|n| device(n, default_in.as_deref(), false))
            .collect();
        self.output_nodes = outputs;
        self.input_nodes = inputs;
        self.watch_output_level();
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
        self.guard_headphones(options);
        self.observe_priority();
    }

    /// Tells the island when the default output's level moved from outside
    /// (the keys, another app), so it shows its volume notice.
    fn watch_output_level(&mut self) {
        let now = self
            .default_output()
            .map(|n| (n.id.clone(), shown(n.volume), n.muted));
        let echo = self.echo_until.is_some_and(|until| Instant::now() < until);
        if let (Some((id, volume, muted)), Some((before, was, was_muted))) = (&now, &self.known)
            && id == before
            && !echo
            && ((volume - was).abs() > 0.004 || muted != was_muted)
        {
            self.notices.push((*volume, *muted));
        }
        self.known = now;
    }

    fn input_mutes(&self) -> Vec<(String, bool)> {
        self.input_nodes
            .iter()
            .map(|n| (n.id.clone(), n.muted))
            .collect()
    }

    fn set_device_mute(&mut self, id: &str, muted: bool) -> Result<(), String> {
        self.pulse.set_mute(Kind::Input, id, muted)?;
        if let Some(node) = self.input_nodes.iter_mut().find(|n| n.id == id) {
            node.muted = muted;
        }
        Ok(())
    }

    fn reload_sessions(&mut self) {
        if let Some(streams) = self.pulse.streams() {
            self.sessions = streams.into_iter().map(Session::new).collect();
        }
    }

    /// The mixer's readings: state and level of every stream.
    fn readings(&self) -> Vec<Reading> {
        self.sessions
            .iter()
            .map(|s| Reading {
                key: s.key.clone(),
                path: s.icon(),
                name: s.name.clone(),
                system: s.system,
                volume: shown(s.stream.volume),
                muted: s.stream.muted,
                playing: !s.stream.corked,
            })
            .collect()
    }

    fn state(&self) -> State {
        State {
            supported: true,
            output: level(self.default_output()),
            input: level(self.default_input()),
            outputs: self.outputs.clone(),
            inputs: self.inputs.clone(),
            can_switch: true,
            mic_muted: self.mic.muted,
            apps: super::group(&self.readings()),
            priority: self.priority_view(),
            switcher_refused: super::SWITCHER_REFUSED.load(Ordering::SeqCst),
            switch_failed: self.switch_failed,
            priority_error: self.priority_error.clone(),
            input_priority_error: self.input_priority_error.clone(),
            roller_failed: super::ROLLER_FAILED.load(Ordering::SeqCst),
        }
    }

    fn run(&mut self, pass: Pass) {
        match pass {
            Pass::Place(generation) if generation == self.placement_generation => {
                self.place_new_devices();
            }
            Pass::Enforce(generation) if generation == self.enforce_generation => {
                self.enforce_priority();
            }
            _ => {}
        }
    }

    // MARK: Headphone guard

    /// Lowers the speakers the server fell back to when headphones left, or
    /// gives them their level back when headphones are the output again.
    fn guard_headphones(&mut self, options: &Options) {
        let outputs: Vec<guard::Output> = self
            .outputs
            .iter()
            .map(|d| guard::Output {
                id: &d.id,
                headphones: d.headphones,
            })
            .collect();
        let default = Self::current_id(&self.outputs);
        let action = self.guard.observe(
            &outputs,
            default.as_deref(),
            options.lower_on_disconnect,
            options.lower_percent,
        );
        match action {
            guard::Action::None => {}
            guard::Action::Lower { id, volume, mute } => {
                let Some(node) = self.output_nodes.iter().find(|n| n.id == id).cloned() else {
                    return;
                };
                let previous = Some(shown(node.volume));
                // Not the island's own change: its volume notice shows the lowered level.
                if self.pulse.set_volume(Kind::Output, &id, volume).is_err() {
                    return;
                }
                // Only a mute the output didn't have is this guard's to undo.
                let muted =
                    mute && !node.muted && self.pulse.set_mute(Kind::Output, &id, true).is_ok();
                self.guard.lowered(&id, previous, volume, muted);
            }
            guard::Action::Restore(lowered) => self.restore(&lowered),
        }
    }

    fn restore(&mut self, lowered: &guard::Lowered) {
        let Some(node) = self
            .pulse
            .outputs()
            .and_then(|nodes| nodes.into_iter().find(|n| n.id == lowered.id))
        else {
            return;
        };
        if !guard::should_restore(lowered, Some(shown(node.volume))) {
            return;
        }
        let _ = self
            .pulse
            .set_volume(Kind::Output, &lowered.id, lowered.previous);
        // A mute switched off since was someone's choice, and one the guard
        // didn't set was never its to undo.
        if lowered.muted_by_lowering && node.muted {
            let _ = self.pulse.set_mute(Kind::Output, &lowered.id, false);
        }
    }

    /// The app is quitting: speakers the guard lowered get their level back
    /// while it is still the one the guard set.
    pub fn restore_lowered(&mut self) {
        let Some(lowered) = self.guard.lowered.take() else {
            return;
        };
        self.restore(&lowered);
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
            self.later.push((
                priority::PLACEMENT_DELAY_MS,
                Pass::Place(self.placement_generation),
            ));
        }
        if changed_out || changed_in {
            self.schedule_enforcement();
        }
    }

    fn remember_names(&mut self) {
        let listed: HashSet<&String> = self
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
        self.later.push((
            priority::ENFORCE_DELAY_MS,
            Pass::Enforce(self.enforce_generation),
        ));
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
            let kind = if output { Kind::Output } else { Kind::Input };
            let result = self.pulse.set_default(kind, target);
            let error = if output {
                &mut self.priority_error
            } else {
                &mut self.input_priority_error
            };
            match result {
                Ok(()) => {
                    switched = true;
                    *error = None;
                }
                Err(e) => *error = Some(e),
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
            Some(next) => self.pulse.set_default(Kind::Output, next).is_ok(),
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

    // MARK: Finer volume keys

    /// One 1% step of the default output; like the volume keys, it unmutes.
    pub fn step_output(&mut self, direction: Direction) {
        let Some(node) = self.default_output().cloned() else {
            return;
        };
        let next = roller::stepped(shown(node.volume), direction);
        if self.pulse.set_volume(Kind::Output, &node.id, next).is_err() {
            return;
        }
        let mut muted = node.muted;
        if muted && self.pulse.set_mute(Kind::Output, &node.id, false).is_ok() {
            muted = false;
        }
        self.remember_output(&node.id, Some(next), Some(muted));
        // Not the island's own change: its volume notice shows these steps.
        self.notices.push((next, muted));
    }

    /// Keeps the default output's level as set, so the server's report of it
    /// is not taken for someone else's change.
    fn remember_output(&mut self, id: &str, volume: Option<f32>, muted: Option<bool>) {
        if let Some(node) = self.output_nodes.iter_mut().find(|n| n.id == id) {
            node.volume = volume.unwrap_or(node.volume);
            node.muted = muted.unwrap_or(node.muted);
            self.known = Some((id.to_string(), shown(node.volume), node.muted));
        }
    }

    pub fn watch(&mut self, on: bool) {
        if on {
            self.reload_sessions();
        }
    }

    /// The default output's (or microphone's) level; a level above zero unmutes.
    pub fn set_level(
        &mut self,
        input: bool,
        volume: Option<f32>,
        muted: Option<bool>,
    ) -> Result<(), String> {
        let node = if input {
            self.default_input()
        } else {
            self.default_output()
        }
        .cloned()
        .ok_or(if input {
            "Microphone unavailable"
        } else {
            "Output unavailable"
        })?;
        let kind = if input { Kind::Input } else { Kind::Output };
        let mut now_muted = node.muted;
        if let Some(v) = volume {
            self.pulse.set_volume(kind, &node.id, v)?;
            if v > 0.0 && muted.is_none() && !(input && self.mic.muted) && node.muted {
                self.pulse.set_mute(kind, &node.id, false)?;
                now_muted = false;
            }
        }
        if let Some(m) = muted {
            self.pulse.set_mute(kind, &node.id, m)?;
            now_muted = m;
        }
        if input {
            if let Some(n) = self.input_nodes.iter_mut().find(|n| n.id == node.id) {
                n.volume = volume.unwrap_or(n.volume);
                n.muted = now_muted;
            }
        } else {
            self.echo_until = Some(Instant::now() + ECHO);
            self.remember_output(&node.id, volume, Some(now_muted));
        }
        Ok(())
    }

    pub fn set_default(&mut self, id: &str) -> Result<(), String> {
        let kind = if self.outputs.iter().any(|d| d.id == id) {
            Kind::Output
        } else if self.inputs.iter().any(|d| d.id == id) {
            Kind::Input
        } else {
            return Err("That device is not connected".into());
        };
        self.pulse.set_default(kind, id)?;
        // A device picked by hand answers the last automatic failure.
        if kind == Kind::Output {
            self.priority_error = None;
        } else {
            self.input_priority_error = None;
        }
        self.reload_devices();
        Ok(())
    }

    /// Every stream of the app; a level above zero unmutes it.
    pub fn set_app(
        &mut self,
        key: &str,
        volume: Option<f32>,
        muted: Option<bool>,
    ) -> Result<(), String> {
        let (mut done, mut error) = (false, None);
        let pulse = &self.pulse;
        // One stream gone bad doesn't stop the app's others.
        for s in self.sessions.iter_mut().filter(|s| s.key == key) {
            let id = s.stream.index.to_string();
            let result = (|| {
                if let Some(v) = volume {
                    pulse.set_volume(Kind::Stream, &id, v)?;
                    s.stream.volume = v;
                    if v > 0.0 && muted.is_none() && s.stream.muted {
                        pulse.set_mute(Kind::Stream, &id, false)?;
                        s.stream.muted = false;
                    }
                }
                if let Some(m) = muted {
                    pulse.set_mute(Kind::Stream, &id, m)?;
                    s.stream.muted = m;
                }
                Ok::<(), String>(())
            })();
            match result {
                Ok(()) => done = true,
                Err(e) => error = Some(e),
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
        // What the microphones say now, not what they said at the last reload.
        if let Some(nodes) = self.pulse.inputs() {
            self.input_nodes = nodes;
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

    /// The Music page's player volume, through the same stream list.
    pub fn player_volume(
        &mut self,
        matches: &dyn Fn(&str) -> bool,
        level: Option<f32>,
    ) -> Result<f32, String> {
        self.reload_sessions();
        let mut result: Option<f32> = None;
        for s in self.sessions.iter_mut().filter(|s| {
            let program = Self::program(s);
            !program.is_empty() && matches(&program)
        }) {
            let id = s.stream.index.to_string();
            if let Some(level) = level {
                self.pulse.set_volume(Kind::Stream, &id, level)?;
                s.stream.volume = level;
                if level > 0.0 && s.stream.muted {
                    self.pulse.set_mute(Kind::Stream, &id, false)?;
                    s.stream.muted = false;
                }
            }
            let value = if s.stream.muted {
                0.0
            } else {
                shown(s.stream.volume)
            };
            result = Some(result.map_or(value, |current| current.max(value)));
        }
        result.ok_or_else(|| "No audio session for this player".into())
    }

    /// The program of a stream, to match a player by.
    fn program(session: &Session) -> String {
        if session.stream.binary.is_empty() {
            session.stream.app.to_lowercase()
        } else {
            session.stream.binary.clone()
        }
    }

    /// The streams of the player `matches` names, each with the monitor of the
    /// output it goes to: what plays is told apart by stream here, and
    /// recorded from that monitor.
    pub fn player_streams(&mut self, matches: &dyn Fn(&str) -> bool) -> Vec<(u32, String)> {
        self.reload_sessions();
        self.sessions
            .iter()
            .filter(|s| {
                let program = Self::program(s);
                !program.is_empty() && matches(&program)
            })
            .filter_map(|s| {
                let output = self
                    .output_nodes
                    .iter()
                    .find(|n| n.index == s.stream.sink)?;
                Some((s.stream.index, format!("{}.monitor", output.id)))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct World {
        default_output: Option<String>,
        default_input: Option<String>,
        outputs: Vec<Node>,
        inputs: Vec<Node>,
        streams: Vec<Stream>,
        calls: Vec<String>,
        /// Commands that fail, by their first word and the id.
        broken: Vec<String>,
    }

    /// A server in memory that does what it is told and says what it was told.
    #[derive(Clone, Default)]
    struct Fake(Arc<Mutex<World>>);

    impl Fake {
        fn calls(&self) -> Vec<String> {
            self.0.lock().unwrap().calls.clone()
        }
        fn world<T>(&self, f: impl FnOnce(&mut World) -> T) -> T {
            f(&mut self.0.lock().unwrap())
        }
    }

    impl Pulse for Fake {
        fn defaults(&self) -> Option<(Option<String>, Option<String>)> {
            self.world(|w| Some((w.default_output.clone(), w.default_input.clone())))
        }
        fn outputs(&self) -> Option<Vec<Node>> {
            self.world(|w| Some(w.outputs.clone()))
        }
        fn inputs(&self) -> Option<Vec<Node>> {
            self.world(|w| Some(w.inputs.clone()))
        }
        fn streams(&self) -> Option<Vec<Stream>> {
            self.world(|w| Some(w.streams.clone()))
        }
        fn recordings(&self) -> Option<Vec<Stream>> {
            Some(Vec::new())
        }
        fn set_volume(&self, kind: Kind, id: &str, volume: f32) -> Result<(), String> {
            self.world(|w| {
                w.calls.push(format!("volume {kind:?} {id} {volume:.2}"));
                if w.broken.iter().any(|b| b == id) {
                    return Err("broken".into());
                }
                let nodes = match kind {
                    Kind::Output => &mut w.outputs,
                    Kind::Input => &mut w.inputs,
                    Kind::Stream => {
                        for s in w.streams.iter_mut().filter(|s| s.index.to_string() == id) {
                            s.volume = volume;
                        }
                        return Ok(());
                    }
                };
                nodes
                    .iter_mut()
                    .filter(|n| n.id == id)
                    .for_each(|n| n.volume = volume);
                Ok(())
            })
        }
        fn set_mute(&self, kind: Kind, id: &str, muted: bool) -> Result<(), String> {
            self.world(|w| {
                w.calls.push(format!("mute {kind:?} {id} {muted}"));
                if w.broken.iter().any(|b| b == id) {
                    return Err("broken".into());
                }
                match kind {
                    Kind::Output => w
                        .outputs
                        .iter_mut()
                        .filter(|n| n.id == id)
                        .for_each(|n| n.muted = muted),
                    Kind::Input => w
                        .inputs
                        .iter_mut()
                        .filter(|n| n.id == id)
                        .for_each(|n| n.muted = muted),
                    Kind::Stream => w
                        .streams
                        .iter_mut()
                        .filter(|s| s.index.to_string() == id)
                        .for_each(|s| s.muted = muted),
                }
                Ok(())
            })
        }
        fn set_default(&self, kind: Kind, id: &str) -> Result<(), String> {
            self.world(|w| {
                w.calls.push(format!("default {kind:?} {id}"));
                match kind {
                    Kind::Output => w.default_output = Some(id.into()),
                    Kind::Input => w.default_input = Some(id.into()),
                    Kind::Stream => return Err("no".into()),
                }
                Ok(())
            })
        }
    }

    fn node(id: &str, name: &str, bus: &str, form: &str, volume: f32, muted: bool) -> Node {
        Node {
            id: id.into(),
            name: name.into(),
            volume,
            muted,
            bus: bus.into(),
            form_factor: form.into(),
            ..Default::default()
        }
    }

    fn stream(index: u32, app: &str, binary: &str, volume: f32) -> Stream {
        Stream {
            index,
            app: app.into(),
            binary: binary.into(),
            volume,
            ..Default::default()
        }
    }

    /// A desktop with speakers on the board, a USB headset, an equalizer, and a microphone each.
    fn desktop() -> (Fake, Engine) {
        let fake = Fake::default();
        fake.world(|w| {
            w.default_output = Some("speakers".into());
            w.default_input = Some("headset-mic".into());
            w.outputs = vec![
                node("speakers", "Built-in Audio", "pci", "internal", 0.5, false),
                node("headset", "Arctis Nova 7", "usb", "headset", 1.0, false),
                node("eq", "Arctis EQ", "", "", 1.0, false),
            ];
            w.inputs = vec![
                node(
                    "headset-mic",
                    "Arctis Nova 7 Mono",
                    "usb",
                    "headset",
                    0.8,
                    false,
                ),
                node("board-mic", "Built-in Microphone", "pci", "", 1.0, false),
            ];
            w.streams = vec![
                stream(1, "Spotify", "spotify", 0.4),
                stream(2, "Spotify", "spotify", 0.9),
                stream(3, "Firefox", "firefox", 1.0),
            ];
        });
        let mut engine =
            Engine::with_backend(Box::new(fake.clone()), MicMute::default(), Saved::default());
        engine.reload_devices_with(&Options::default());
        engine.reload_sessions();
        (fake, engine)
    }

    #[test]
    fn the_state_is_the_servers() {
        let (_, engine) = desktop();
        let state = engine.state();
        assert!(state.supported && state.can_switch);
        assert_eq!(
            state.output,
            Level {
                volume: Some(0.5),
                muted: Some(false)
            }
        );
        assert_eq!(
            state.input,
            Level {
                volume: Some(0.8),
                muted: Some(false)
            }
        );
        let names: Vec<_> = state
            .outputs
            .iter()
            .map(|d| (d.id.as_str(), d.is_default))
            .collect();
        assert_eq!(
            names,
            [("speakers", true), ("headset", false), ("eq", false)]
        );
        assert_eq!(state.inputs.len(), 2);
        // Worn devices are headphones, and the tiers say built in, plugged in, or made up.
        assert!(!engine.outputs[0].headphones && engine.outputs[1].headphones);
        assert_eq!(engine.outputs[0].tier, Some(priority::Tier::BuiltIn));
        assert_eq!(engine.outputs[1].tier, Some(priority::Tier::Hardware));
        assert_eq!(engine.outputs[2].tier, Some(priority::Tier::Virtual));
    }

    #[test]
    fn two_streams_of_an_app_are_one_column_with_the_loudest_level() {
        let (_, engine) = desktop();
        let apps = engine.state().apps;
        let keys: Vec<_> = apps
            .iter()
            .map(|a| (a.key.as_str(), a.name.as_str(), a.volume))
            .collect();
        assert_eq!(
            keys,
            [("firefox", "Firefox", 1.0), ("spotify", "Spotify", 0.9)]
        );
        assert!(apps.iter().all(|a| a.playing && !a.system));
    }

    #[test]
    fn paused_streams_do_not_play_and_event_sounds_are_the_systems() {
        let (fake, mut engine) = desktop();
        fake.world(|w| {
            w.streams[0].corked = true;
            w.streams[1].corked = true;
            w.streams.push(Stream {
                role: "event".into(),
                ..stream(9, "libcanberra", "", 1.0)
            });
        });
        engine.reload_sessions();
        let apps = engine.state().apps;
        assert!(!apps.iter().find(|a| a.key == "spotify").unwrap().playing);
        let system = apps.last().unwrap();
        assert!(system.system && system.key == "system" && system.name == "System sounds");
        assert!(system.path.is_none());
        assert_eq!(apps[1].path.as_deref(), Some("icon:spotify"));
    }

    #[test]
    fn a_level_above_zero_unmutes_and_the_microphone_stays_muted_under_mic_mute() {
        let (fake, mut engine) = desktop();
        fake.world(|w| w.outputs[0].muted = true);
        engine.reload_devices_with(&Options::default());
        engine.set_level(false, Some(0.3), None).unwrap();
        assert_eq!(
            fake.calls(),
            ["volume Output speakers 0.30", "mute Output speakers false"]
        );
        assert_eq!(
            engine.state().output,
            Level {
                volume: Some(0.3),
                muted: Some(false)
            }
        );
        // Muting by itself leaves the level.
        engine.set_level(false, None, Some(true)).unwrap();
        assert_eq!(
            engine.state().output,
            Level {
                volume: Some(0.3),
                muted: Some(true)
            }
        );
        // Under Mic Mute a louder microphone is not unmuted.
        engine.set_mic_mute(Some(true)).unwrap();
        let before = fake.calls().len();
        engine.set_level(true, Some(0.9), None).unwrap();
        assert_eq!(fake.calls()[before..], ["volume Input headset-mic 0.90"]);
    }

    #[test]
    fn a_missing_device_or_a_refusing_server_is_an_error() {
        let (fake, mut engine) = desktop();
        fake.world(|w| w.broken.push("speakers".into()));
        assert!(engine.set_level(false, Some(0.1), None).is_err());
        fake.world(|w| w.default_output = None);
        engine.reload_devices_with(&Options::default());
        assert_eq!(engine.state().output, Level::default());
        assert_eq!(
            engine.set_level(false, Some(0.1), None),
            Err("Output unavailable".into())
        );
    }

    #[test]
    fn the_default_goes_to_the_kind_of_device_asked_for() {
        let (fake, mut engine) = desktop();
        engine.set_default("headset").unwrap();
        engine.set_default("board-mic").unwrap();
        assert_eq!(
            fake.calls(),
            ["default Output headset", "default Input board-mic"]
        );
        let state = engine.state();
        assert!(
            state
                .outputs
                .iter()
                .find(|d| d.id == "headset")
                .unwrap()
                .is_default
        );
        assert!(
            state
                .inputs
                .iter()
                .find(|d| d.id == "board-mic")
                .unwrap()
                .is_default
        );
        assert!(engine.set_default("unplugged").is_err());
    }

    #[test]
    fn an_app_is_set_on_every_stream_and_unmuted_by_a_level() {
        let (fake, mut engine) = desktop();
        fake.world(|w| w.streams[0].muted = true);
        engine.reload_sessions();
        engine.set_app("spotify", Some(0.2), None).unwrap();
        assert_eq!(
            fake.calls(),
            [
                "volume Stream 1 0.20",
                "mute Stream 1 false",
                "volume Stream 2 0.20"
            ]
        );
        engine.set_app("spotify", None, Some(true)).unwrap();
        assert!(fake.world(|w| w.streams[..2].iter().all(|s| s.muted)));
        assert_eq!(
            engine.set_app("nothing", Some(0.5), None),
            Err("The app has no audio session".into())
        );
        // One stream gone bad does not stop the other.
        fake.world(|w| w.broken.push("1".into()));
        assert!(engine.set_app("spotify", Some(0.7), None).is_ok());
        assert_eq!(fake.world(|w| w.streams[1].volume), 0.7);
    }

    #[test]
    fn mic_mute_mutes_each_microphone_and_gives_back_only_its_own() {
        let (fake, mut engine) = desktop();
        fake.world(|w| w.inputs[1].muted = true);
        assert_eq!(engine.set_mic_mute(None), Ok(true));
        assert!(fake.world(|w| w.inputs.iter().all(|n| n.muted)));
        assert!(engine.state().mic_muted);
        assert_eq!(engine.set_mic_mute(Some(true)), Ok(true));
        assert_eq!(engine.set_mic_mute(None), Ok(false));
        // The one muted before stays muted.
        assert_eq!(
            fake.world(|w| w.inputs.iter().map(|n| n.muted).collect::<Vec<_>>()),
            [false, true]
        );
    }

    #[test]
    fn a_microphone_plugged_in_under_mic_mute_is_muted() {
        let (fake, mut engine) = desktop();
        engine.set_mic_mute(Some(true)).unwrap();
        fake.world(|w| {
            w.inputs
                .push(node("usb-mic", "USB Microphone", "usb", "", 1.0, false))
        });
        engine.reload_devices_with(&Options::default());
        assert!(fake.world(|w| w.inputs[2].muted));
    }

    #[test]
    fn the_island_hears_of_a_level_that_someone_else_moved_but_not_of_its_own() {
        let (fake, mut engine) = desktop();
        assert!(engine.notices.is_empty(), "the first look is not a change");
        engine.set_level(false, Some(0.3), None).unwrap();
        // The server's report of the island's own change is its echo.
        engine.reload_devices_with(&Options::default());
        assert!(engine.notices.is_empty());
        // After the echo window, a key press is news.
        engine.echo_until = None;
        fake.world(|w| w.outputs[0].volume = 0.45);
        engine.reload_devices_with(&Options::default());
        assert_eq!(engine.notices, [(0.45, false)]);
        engine.notices.clear();
        // Nothing moved: nothing said. A mute counts.
        engine.reload_devices_with(&Options::default());
        assert!(engine.notices.is_empty());
        fake.world(|w| w.outputs[0].muted = true);
        engine.reload_devices_with(&Options::default());
        assert_eq!(engine.notices, [(0.45, true)]);
        // A new default is not a level change.
        engine.notices.clear();
        fake.world(|w| w.default_output = Some("headset".into()));
        engine.reload_devices_with(&Options::default());
        assert!(engine.notices.is_empty());
    }

    #[test]
    fn a_finer_step_moves_one_percent_unmutes_and_is_shown() {
        let (fake, mut engine) = desktop();
        fake.world(|w| w.outputs[0].muted = true);
        engine.reload_devices_with(&Options::default());
        engine.notices.clear();
        engine.step_output(Direction::Up);
        assert_eq!(
            fake.calls(),
            ["volume Output speakers 0.51", "mute Output speakers false"]
        );
        assert_eq!(engine.notices, [(0.51, false)]);
        // The server's report of it is not a second notice.
        engine.reload_devices_with(&Options::default());
        assert_eq!(engine.notices.len(), 1);
    }

    #[test]
    fn the_headphone_guard_lowers_the_speakers_and_gives_their_level_back() {
        let (fake, mut engine) = desktop();
        let guard = Options {
            lower_on_disconnect: true,
            lower_percent: 20,
            ..Options::default()
        };
        // The headset is the output; then it is unplugged and the speakers take over.
        fake.world(|w| w.default_output = Some("headset".into()));
        engine.reload_devices_with(&guard);
        fake.world(|w| {
            w.outputs.retain(|n| n.id != "headset");
            w.default_output = Some("speakers".into());
        });
        engine.reload_devices_with(&guard);
        assert_eq!(fake.world(|w| w.outputs[0].volume), 0.2);
        // The headset returns as the output: the speakers are as they were.
        fake.world(|w| {
            w.outputs.push(node(
                "headset",
                "Arctis Nova 7",
                "usb",
                "headset",
                1.0,
                false,
            ));
            w.default_output = Some("headset".into());
        });
        engine.reload_devices_with(&guard);
        assert_eq!(fake.world(|w| w.outputs[0].volume), 0.5);
        // With the option off nothing is touched.
        let (fake, mut engine) = desktop();
        fake.world(|w| w.default_output = Some("headset".into()));
        engine.reload_devices_with(&Options::default());
        fake.world(|w| {
            w.outputs.retain(|n| n.id != "headset");
            w.default_output = Some("speakers".into());
        });
        engine.reload_devices_with(&Options::default());
        assert_eq!(fake.world(|w| w.outputs[0].volume), 0.5);
    }

    #[test]
    fn audio_priority_makes_the_best_connected_output_the_default() {
        let (fake, mut engine) = desktop();
        engine.set_priority(
            true,
            Some(true),
            Some(vec!["headset".into(), "speakers".into()]),
        );
        // The engine asks for a pass soon, and it makes the headset the default.
        let pass = engine
            .later
            .iter()
            .rev()
            .find(|(_, p)| matches!(p, Pass::Enforce(_)))
            .unwrap()
            .1;
        engine.run(pass);
        assert_eq!(
            fake.world(|w| w.default_output.clone()),
            Some("headset".into())
        );
        assert!(engine.priority_error.is_none());
        // An older pass than the newest does nothing.
        engine.set_priority(true, None, Some(vec!["speakers".into(), "headset".into()]));
        let stale = pass;
        engine.run(stale);
        assert_eq!(
            fake.world(|w| w.default_output.clone()),
            Some("headset".into())
        );
        // The headset is unplugged: the speakers are next in the list.
        fake.world(|w| w.outputs.retain(|n| n.id != "headset"));
        engine.reload_devices_with(&Options::default());
        let pass = engine
            .later
            .iter()
            .rev()
            .find(|(_, p)| matches!(p, Pass::Enforce(_)))
            .unwrap()
            .1;
        engine.run(pass);
        assert_eq!(
            fake.world(|w| w.default_output.clone()),
            Some("speakers".into())
        );
    }

    #[test]
    fn a_failed_automatic_switch_says_why_and_a_manual_one_clears_it() {
        let (fake, mut engine) = desktop();
        fake.world(|w| w.default_output = Some("eq".into()));
        engine.reload_devices_with(&Options::default());
        engine.set_priority(true, Some(true), Some(vec!["speakers".into(), "eq".into()]));
        // The server refuses the switch (a stand-in id that fails).
        fake.world(|w| w.broken.push("speakers".into()));
        struct Refusing(Fake);
        impl Pulse for Refusing {
            fn defaults(&self) -> Option<(Option<String>, Option<String>)> {
                self.0.defaults()
            }
            fn outputs(&self) -> Option<Vec<Node>> {
                self.0.outputs()
            }
            fn inputs(&self) -> Option<Vec<Node>> {
                self.0.inputs()
            }
            fn streams(&self) -> Option<Vec<Stream>> {
                self.0.streams()
            }
            fn recordings(&self) -> Option<Vec<Stream>> {
                self.0.recordings()
            }
            fn set_volume(&self, k: Kind, i: &str, v: f32) -> Result<(), String> {
                self.0.set_volume(k, i, v)
            }
            fn set_mute(&self, k: Kind, i: &str, m: bool) -> Result<(), String> {
                self.0.set_mute(k, i, m)
            }
            fn set_default(&self, _: Kind, _: &str) -> Result<(), String> {
                Err("the server said no".into())
            }
        }
        engine.pulse = Box::new(Refusing(fake.clone()));
        let pass = engine
            .later
            .iter()
            .rev()
            .find(|(_, p)| matches!(p, Pass::Enforce(_)))
            .unwrap()
            .1;
        engine.run(pass);
        assert_eq!(
            engine.state().priority_error.as_deref(),
            Some("the server said no")
        );
    }

    #[test]
    fn the_switcher_cycles_through_the_chosen_outputs() {
        let (fake, mut engine) = desktop();
        let cycle = vec!["speakers".to_string(), "headset".to_string()];
        engine.switch_to_next(&cycle);
        assert_eq!(
            fake.world(|w| w.default_output.clone()),
            Some("headset".into())
        );
        engine.switch_to_next(&cycle);
        assert_eq!(
            fake.world(|w| w.default_output.clone()),
            Some("speakers".into())
        );
        assert!(!engine.state().switch_failed);
        // None of the chosen outputs is connected: that is a failure to show.
        engine.switch_to_next(&["gone".to_string(), "missing".to_string()]);
        assert!(engine.state().switch_failed);
        engine.clear_switch_failure();
        assert!(!engine.state().switch_failed);
    }

    #[test]
    fn a_players_volume_is_read_and_set_through_its_streams() {
        let (fake, mut engine) = desktop();
        let spotify = |program: &str| program == "spotify";
        assert_eq!(engine.player_volume(&spotify, None), Ok(0.9));
        assert_eq!(engine.player_volume(&spotify, Some(0.25)), Ok(0.25));
        assert_eq!(
            fake.world(|w| (
                w.streams[0].volume,
                w.streams[1].volume,
                w.streams[2].volume
            )),
            (0.25, 0.25, 1.0)
        );
        // Muted reads as silent; a level unmutes.
        fake.world(|w| w.streams[0].muted = true);
        fake.world(|w| w.streams[1].muted = true);
        assert_eq!(engine.player_volume(&spotify, None), Ok(0.0));
        assert_eq!(engine.player_volume(&spotify, Some(0.6)), Ok(0.6));
        assert!(engine.player_volume(&|p: &str| p == "vlc", None).is_err());
        // Each stream with the monitor of the output it plays to.
        fake.world(|w| {
            w.outputs[1].index = 62;
            w.streams[0].sink = 62;
            w.streams[1].sink = 62;
        });
        engine.reload_devices_with(&Options::default());
        assert_eq!(
            engine.player_streams(&spotify),
            [
                (1, "headset.monitor".to_string()),
                (2, "headset.monitor".to_string())
            ]
        );
    }

    #[test]
    fn a_server_that_does_not_answer_leaves_what_is_known() {
        struct Silent;
        impl Pulse for Silent {
            fn defaults(&self) -> Option<(Option<String>, Option<String>)> {
                None
            }
            fn outputs(&self) -> Option<Vec<Node>> {
                None
            }
            fn inputs(&self) -> Option<Vec<Node>> {
                None
            }
            fn streams(&self) -> Option<Vec<Stream>> {
                None
            }
            fn recordings(&self) -> Option<Vec<Stream>> {
                None
            }
            fn set_volume(&self, _: Kind, _: &str, _: f32) -> Result<(), String> {
                Err("down".into())
            }
            fn set_mute(&self, _: Kind, _: &str, _: bool) -> Result<(), String> {
                Err("down".into())
            }
            fn set_default(&self, _: Kind, _: &str) -> Result<(), String> {
                Err("down".into())
            }
        }
        let (_, mut engine) = desktop();
        engine.pulse = Box::new(Silent);
        engine.reload_devices_with(&Options::default());
        engine.reload_sessions();
        assert_eq!(engine.state().outputs.len(), 3);
        assert_eq!(engine.state().apps.len(), 2);
    }

    #[test]
    fn a_player_is_found_among_the_programs_by_its_desktop_entry() {
        for (source, program) in [
            ("spotify", "spotify"),
            ("com.spotify.Client", "spotify"),
            ("org.gnome.Rhythmbox3", "rhythmbox"),
            ("org.videolan.VLC", "vlc"),
            ("google-chrome", "chrome"),
            ("firefox", "firefox"),
        ] {
            assert!(same_player(source, program), "{source} / {program}");
        }
        for (source, program) in [
            ("spotify", "firefox"),
            ("chromium", "chrome"),
            ("vlcplayer", "vlc"),
            ("spotify", ""),
        ] {
            assert!(!same_player(source, program), "{source} / {program}");
        }
    }

    #[test]
    fn form_factors_map_to_what_the_guard_knows() {
        assert_eq!(form_factor("headphone"), Some(3));
        assert_eq!(form_factor("headset"), Some(5));
        assert_eq!(form_factor("hands-free"), Some(5));
        assert_eq!(form_factor("speaker"), None);
        assert_eq!(form_factor(""), None);
    }

    #[test]
    #[ignore = "reads this desktop's sound server; OPENBAR_LIVE_PROGRAM names a program playing a stream made for the test"]
    fn the_engine_reads_the_desktop_and_sets_the_test_programs_stream() {
        let mut engine = Engine::with_backend(
            Box::new(Pactl::default()),
            MicMute::default(),
            Saved::default(),
        );
        engine.reload_devices_with(&Options::default());
        engine.reload_sessions();
        let state = engine.state();
        println!("output {:?} input {:?}", state.output, state.input);
        println!(
            "outputs {:?}",
            state
                .outputs
                .iter()
                .map(|d| (&d.name, d.is_default, d.headphones, d.tier))
                .collect::<Vec<_>>()
        );
        println!(
            "inputs {:?}",
            state
                .inputs
                .iter()
                .map(|d| (&d.name, d.is_default))
                .collect::<Vec<_>>()
        );
        println!(
            "apps {:?}",
            state
                .apps
                .iter()
                .map(|a| (&a.key, &a.name, &a.path, a.volume, a.muted, a.playing))
                .collect::<Vec<_>>()
        );
        let Ok(program) = std::env::var("OPENBAR_LIVE_PROGRAM") else {
            return;
        };
        assert!(
            state.apps.iter().any(|a| a.key == program),
            "the test stream is not in the mixer"
        );
        engine.set_app(&program, Some(0.3), None).unwrap();
        engine.set_app(&program, None, Some(true)).unwrap();
        engine.reload_sessions();
        let app = engine
            .state()
            .apps
            .into_iter()
            .find(|a| a.key == program)
            .unwrap();
        assert!(
            (app.volume - 0.3).abs() < 0.001 && app.muted && app.playing,
            "{app:?}"
        );
        let is_program = |p: &str| p == program;
        assert_eq!(
            engine.player_volume(&is_program, None),
            Ok(0.0),
            "muted reads as silent"
        );
        let level = engine.player_volume(&is_program, Some(0.8)).unwrap();
        assert!((level - 0.8).abs() < 0.001);
        assert_eq!(engine.player_streams(&is_program).len(), 1);
        println!("engine controlled {program}: ok");
    }
}
