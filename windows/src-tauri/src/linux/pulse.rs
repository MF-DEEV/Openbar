// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The sound server of a Linux desktop, PulseAudio or PipeWire's pulse server,
//! through `pactl` (version 16 and later, for its JSON): the outputs and
//! microphones, what plays and records, and the changes to them. The engine
//! talks to this through [`Pulse`], so its decisions can be tested without a
//! server.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Mutex, Once};
use std::time::Duration;

use serde_json::Value;

/// What a volume of 100% is, in the server's own units.
const FULL: f64 = 65536.0;

/// An output or a microphone.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Node {
    /// The server's number for it, which a recording names its source by.
    pub index: u32,
    /// The server's name for it, which stays the same across restarts.
    pub id: String,
    /// What the person calls it ("Arctis Nova 7 Analog Stereo").
    pub name: String,
    /// The loudest channel, 1.0 being 100%; a server can go above.
    pub volume: f32,
    pub muted: bool,
    /// `pci`, `usb`, `bluetooth`; empty for a virtual device.
    pub bus: String,
    /// `headphone`, `headset`, `speaker`, ...; empty when not said.
    pub form_factor: String,
}

/// What one app plays (a sink input), or records (a source output).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stream {
    pub index: u32,
    /// `application.name`, as the app calls itself.
    pub app: String,
    /// The program's name, lower case (`application.process.binary`).
    pub binary: String,
    pub icon: String,
    /// `event` for the desktop's own sounds.
    pub role: String,
    /// What the stream says it is (`media.name`): "Peak detect" for a level meter.
    pub media: String,
    pub volume: f32,
    pub muted: bool,
    /// Paused: the stream is open but plays nothing.
    pub corked: bool,
    /// The number of the source a recording reads from.
    pub source: u32,
    /// The number of the output a playing stream goes to.
    pub sink: u32,
}

/// What changed on the server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A device came or went, a default moved, a level or name changed.
    Devices,
    /// An app started or stopped playing, or its stream changed.
    Streams,
    /// An app started or stopped recording.
    Recordings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Output,
    Input,
    Stream,
}

/// The server, as the engine uses it.
pub trait Pulse: Send {
    /// The default output's and microphone's names.
    fn defaults(&self) -> Option<(Option<String>, Option<String>)>;
    fn outputs(&self) -> Option<Vec<Node>>;
    /// Microphones only, not the monitors of outputs.
    fn inputs(&self) -> Option<Vec<Node>>;
    /// What plays now.
    fn streams(&self) -> Option<Vec<Stream>>;
    /// What records now.
    fn recordings(&self) -> Option<Vec<Stream>>;
    /// `id` is a device's name, or a stream's index.
    fn set_volume(&self, kind: Kind, id: &str, volume: f32) -> Result<(), String>;
    fn set_mute(&self, kind: Kind, id: &str, muted: bool) -> Result<(), String>;
    fn set_default(&self, kind: Kind, id: &str) -> Result<(), String>;
}

// What `pactl` prints.

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn property(item: &Value, key: &str) -> String {
    item.get("properties")
        .map(|p| text(p, key))
        .unwrap_or_default()
}

/// The loudest channel: 1.0 for 100%, or 1.0 when the item says nothing.
fn level(item: &Value) -> f32 {
    let loudest = item
        .get("volume")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|channels| channels.values())
        .filter_map(|channel| channel.get("value").and_then(Value::as_f64))
        .fold(None, |best: Option<f64>, v| {
            Some(best.map_or(v, |b| b.max(v)))
        });
    loudest.map_or(1.0, |v| (v / FULL) as f32)
}

fn items(json: &str) -> Vec<Value> {
    match serde_json::from_str::<Value>(json) {
        Ok(Value::Array(items)) => items,
        _ => Vec::new(),
    }
}

/// Outputs, or microphones. A microphone list also holds the monitor of every
/// output, which is not a microphone.
pub fn parse_nodes(json: &str, microphones: bool) -> Vec<Node> {
    items(json)
        .iter()
        .filter_map(|item| {
            let id = text(item, "name");
            let monitor = id.ends_with(".monitor") || property(item, "media.class") == "Audio/Sink";
            if id.is_empty() || (microphones && monitor) {
                return None;
            }
            let description = text(item, "description");
            Some(Node {
                index: item.get("index").and_then(Value::as_u64).unwrap_or(0) as u32,
                name: if description.is_empty() {
                    id.clone()
                } else {
                    description
                },
                id,
                volume: level(item),
                muted: item.get("mute").and_then(Value::as_bool).unwrap_or(false),
                bus: property(item, "device.bus"),
                form_factor: property(item, "device.form_factor"),
            })
        })
        .collect()
}

/// Sink inputs, or source outputs. An app is known by what it calls itself or
/// its program; a stream with neither (a filter inside PipeWire) is not an app.
pub fn parse_streams(json: &str) -> Vec<Stream> {
    items(json)
        .iter()
        .filter_map(|item| {
            let app = property(item, "application.name");
            let binary = property(item, "application.process.binary").to_lowercase();
            if app.is_empty() && binary.is_empty() {
                return None;
            }
            Some(Stream {
                index: item.get("index").and_then(Value::as_u64)? as u32,
                app,
                binary,
                icon: property(item, "application.icon_name"),
                role: property(item, "media.role"),
                media: property(item, "media.name"),
                volume: level(item),
                muted: item.get("mute").and_then(Value::as_bool).unwrap_or(false),
                corked: item.get("corked").and_then(Value::as_bool).unwrap_or(false),
                source: item.get("source").and_then(Value::as_u64).unwrap_or(0) as u32,
                sink: item.get("sink").and_then(Value::as_u64).unwrap_or(0) as u32,
            })
        })
        .collect()
}

pub fn parse_defaults(json: &str) -> (Option<String>, Option<String>) {
    let info = serde_json::from_str::<Value>(json).unwrap_or(Value::Null);
    let name = |key: &str| Some(text(&info, key)).filter(|n| !n.is_empty());
    (name("default_sink_name"), name("default_source_name"))
}

/// One line of `pactl subscribe`: `Event 'change' on sink #62`.
pub fn parse_event(line: &str) -> Option<Event> {
    let facility = line.strip_prefix("Event '")?.split(" on ").nth(1)?;
    let facility = facility.split([' ', '#']).next()?;
    match facility {
        "sink" | "source" | "server" | "card" => Some(Event::Devices),
        "sink-input" => Some(Event::Streams),
        "source-output" => Some(Event::Recordings),
        _ => None,
    }
}

/// A volume as `pactl` is given it: the server's own units, so a level comes
/// out exactly as set.
fn units(volume: f32) -> String {
    ((f64::from(volume.clamp(0.0, 1.5)) * FULL).round() as u32).to_string()
}

/// The server, asked through `pactl`.
pub struct Pactl {
    program: PathBuf,
}

impl Default for Pactl {
    fn default() -> Self {
        Self::with_program("pactl")
    }
}

impl Pactl {
    pub fn with_program(program: impl Into<PathBuf>) -> Self {
        Pactl {
            program: program.into(),
        }
    }

    fn run(&self, arguments: &[&str]) -> Option<String> {
        super::command::run(&self.program, &[("LC_ALL", "C")], arguments, None)
    }

    fn done(&self, arguments: &[&str]) -> Result<(), String> {
        self.run(arguments)
            .map(|_| ())
            .ok_or_else(|| format!("pactl {} failed", arguments.first().copied().unwrap_or("")))
    }

    /// Whether `pactl` is there and speaks JSON, which only version 16 does.
    pub fn available(&self) -> bool {
        self.run(&["--format=json", "info"]).is_some()
    }
}

impl Pulse for Pactl {
    fn defaults(&self) -> Option<(Option<String>, Option<String>)> {
        self.run(&["--format=json", "info"])
            .map(|json| parse_defaults(&json))
    }

    fn outputs(&self) -> Option<Vec<Node>> {
        self.run(&["--format=json", "list", "sinks"])
            .map(|json| parse_nodes(&json, false))
    }

    fn inputs(&self) -> Option<Vec<Node>> {
        self.run(&["--format=json", "list", "sources"])
            .map(|json| parse_nodes(&json, true))
    }

    fn streams(&self) -> Option<Vec<Stream>> {
        self.run(&["--format=json", "list", "sink-inputs"])
            .map(|json| parse_streams(&json))
    }

    fn recordings(&self) -> Option<Vec<Stream>> {
        self.run(&["--format=json", "list", "source-outputs"])
            .map(|json| parse_streams(&json))
    }

    fn set_volume(&self, kind: Kind, id: &str, volume: f32) -> Result<(), String> {
        let command = match kind {
            Kind::Output => "set-sink-volume",
            Kind::Input => "set-source-volume",
            Kind::Stream => "set-sink-input-volume",
        };
        self.done(&[command, id, &units(volume)])
    }

    fn set_mute(&self, kind: Kind, id: &str, muted: bool) -> Result<(), String> {
        let command = match kind {
            Kind::Output => "set-sink-mute",
            Kind::Input => "set-source-mute",
            Kind::Stream => "set-sink-input-mute",
        };
        self.done(&[command, id, if muted { "1" } else { "0" }])
    }

    fn set_default(&self, kind: Kind, id: &str) -> Result<(), String> {
        match kind {
            Kind::Output => self.done(&["set-default-sink", id]),
            Kind::Input => self.done(&["set-default-source", id]),
            Kind::Stream => Err("A stream is not a device".into()),
        }
    }
}

type Listener = Box<dyn Fn(Event) + Send>;

static LISTENERS: Mutex<Vec<Listener>> = Mutex::new(Vec::new());
static LISTENING: Once = Once::new();

/// Calls `on_event` for every change the server reports, for as long as the
/// program runs. One `pactl subscribe` serves every listener.
pub fn subscribe(on_event: impl Fn(Event) + Send + 'static) {
    LISTENERS.lock().unwrap().push(Box::new(on_event));
    LISTENING.call_once(|| {
        let _ = std::thread::Builder::new()
            .name("openbar-pulse-events".into())
            .spawn(|| {
                listen("pactl".into(), Duration::from_secs(3), |event| {
                    for listener in LISTENERS.lock().unwrap().iter() {
                        listener(event);
                    }
                    true
                });
            });
    });
}

/// Runs `program subscribe` and hands every change to `on_event` until it
/// answers false. The subscription is taken up again `retry` after the
/// server goes away (a restart of the sound server).
fn listen(program: PathBuf, retry: Duration, on_event: impl Fn(Event) -> bool) {
    loop {
        let child = Command::new(&program)
            .arg("subscribe")
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(mut child) = child {
            if let Some(out) = child.stdout.take() {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    if let Some(event) = parse_event(&line)
                        && !on_event(event)
                    {
                        let _ = child.kill();
                        let _ = child.wait();
                        return;
                    }
                }
            }
            let _ = child.wait();
        }
        std::thread::sleep(retry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed from a PipeWire machine's `pactl --format=json list ...`.
    const SINKS: &str = r#"[
      {"index":36,"name":"arctis_eq","description":"Arctis EQ","mute":false,
       "volume":{"front-left":{"value":65536,"value_percent":"100%"},"front-right":{"value":65536,"value_percent":"100%"}},
       "properties":{"media.class":"Audio/Sink"}},
      {"index":62,"name":"alsa_output.usb-SteelSeries_Arctis_Nova_7-00.analog-stereo","description":"Arctis Nova 7 Analog Stereo","mute":true,
       "volume":{"front-left":{"value":98304,"value_percent":"150%"},"front-right":{"value":32768,"value_percent":"50%"}},
       "properties":{"device.bus":"usb","device.form_factor":"headset","media.class":"Audio/Sink"}}]"#;

    const SOURCES: &str = r#"[
      {"index":36,"name":"arctis_eq.monitor","description":"Monitor of Arctis EQ","mute":false,"volume":{},"properties":{"media.class":"Audio/Sink"}},
      {"index":63,"name":"alsa_input.usb-SteelSeries_Arctis_Nova_7-00.mono-fallback","description":"Arctis Nova 7 Mono","mute":false,
       "volume":{"mono":{"value":52428,"value_percent":"80%"}},
       "properties":{"device.bus":"usb","device.form_factor":"headset","media.class":"Audio/Source"}},
      {"index":70,"name":"bluez_input.AA","description":"","mute":false,"volume":{},"properties":{"media.class":"Audio/Source"}}]"#;

    const INPUTS: &str = r#"[
      {"index":37,"corked":true,"mute":false,"volume":{"front-left":{"value":65536}},"properties":{"media.name":"Arctis EQ"}},
      {"index":91,"sink":62,"corked":false,"mute":false,"volume":{"front-left":{"value":32768},"front-right":{"value":16384}},
       "properties":{"application.name":"Spotify","application.process.binary":"Spotify","application.icon_name":"spotify-client","media.role":"music","media.name":"Song"}},
      {"index":92,"corked":true,"mute":true,"properties":{"application.name":"Notify","media.role":"event"}}]"#;

    #[test]
    fn outputs_are_read_with_the_loudest_channel() {
        let nodes = parse_nodes(SINKS, false);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].id, "arctis_eq");
        assert_eq!((nodes[0].volume, nodes[0].muted), (1.0, false));
        assert_eq!(nodes[0].bus, "");
        let arctis = &nodes[1];
        assert_eq!(arctis.name, "Arctis Nova 7 Analog Stereo");
        assert_eq!((arctis.volume, arctis.muted), (1.5, true));
        assert_eq!(
            (arctis.bus.as_str(), arctis.form_factor.as_str()),
            ("usb", "headset")
        );
    }

    #[test]
    fn microphones_leave_out_the_monitors_of_outputs() {
        let nodes = parse_nodes(SOURCES, true);
        let ids: Vec<_> = nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "alsa_input.usb-SteelSeries_Arctis_Nova_7-00.mono-fallback",
                "bluez_input.AA"
            ]
        );
        assert_eq!(nodes[0].volume, 52428.0 / 65536.0);
        // No description: the name stands in; no volume said: full.
        assert_eq!(
            (nodes[1].name.as_str(), nodes[1].volume),
            ("bluez_input.AA", 1.0)
        );
        // As outputs the same JSON would keep them all.
        assert_eq!(parse_nodes(SOURCES, false).len(), 3);
    }

    #[test]
    fn a_stream_with_no_app_is_not_one() {
        let streams = parse_streams(INPUTS);
        assert_eq!(streams.len(), 2);
        let spotify = &streams[0];
        assert_eq!(
            (spotify.index, spotify.app.as_str(), spotify.binary.as_str()),
            (91, "Spotify", "spotify")
        );
        assert_eq!(
            (spotify.volume, spotify.corked, spotify.role.as_str()),
            (0.5, false, "music")
        );
        assert_eq!(spotify.icon, "spotify-client");
        let notify = &streams[1];
        assert!(notify.corked && notify.muted);
        assert_eq!(
            (notify.role.as_str(), notify.binary.as_str()),
            ("event", "")
        );
    }

    #[test]
    fn a_recording_names_its_source() {
        let json = r#"[{"index":7,"source":63,"corked":false,"mute":false,
          "properties":{"application.name":"Discord","application.process.binary":"Discord","media.name":"playStream"}}]"#;
        let recordings = parse_streams(json);
        assert_eq!(
            (recordings[0].source, recordings[0].media.as_str()),
            (63, "playStream")
        );
        assert_eq!(parse_nodes(SOURCES, true)[0].index, 63);
    }

    #[test]
    fn bad_output_is_nothing() {
        assert!(parse_nodes("", false).is_empty());
        assert!(parse_nodes("{}", true).is_empty());
        assert!(parse_streams("not json").is_empty());
        assert_eq!(parse_defaults("[]"), (None, None));
    }

    #[test]
    fn the_defaults_come_from_info() {
        let info = r#"{"server_name":"PulseAudio (on PipeWire 1.6.9)","default_sink_name":"arctis_eq","default_source_name":"alsa_input.usb-X"}"#;
        assert_eq!(
            parse_defaults(info),
            (Some("arctis_eq".into()), Some("alsa_input.usb-X".into()))
        );
        assert_eq!(parse_defaults(r#"{"default_sink_name":""}"#), (None, None));
    }

    #[test]
    fn events_say_what_kind_of_thing_changed() {
        let event = |line: &str| parse_event(line);
        assert_eq!(event("Event 'change' on sink #62"), Some(Event::Devices));
        assert_eq!(event("Event 'new' on source #5"), Some(Event::Devices));
        assert_eq!(event("Event 'change' on server"), Some(Event::Devices));
        assert_eq!(event("Event 'change' on card #3"), Some(Event::Devices));
        assert_eq!(
            event("Event 'remove' on sink-input #91"),
            Some(Event::Streams)
        );
        assert_eq!(
            event("Event 'new' on source-output #7"),
            Some(Event::Recordings)
        );
        assert_eq!(event("Event 'change' on client #12"), None);
        assert_eq!(event("garbage"), None);
        assert_eq!(event(""), None);
    }

    #[test]
    fn a_level_is_given_in_the_servers_units() {
        assert_eq!(units(1.0), "65536");
        assert_eq!(units(0.0), "0");
        assert_eq!(units(0.5), "32768");
        assert_eq!(units(9.0), "98304");
        assert_eq!(units(-1.0), "0");
    }

    // A shell script standing in for `pactl`: it logs what it was asked and
    // answers the lists from files beside it.
    fn stand_in(dir: &std::path::Path) -> (Pactl, std::path::PathBuf) {
        std::fs::create_dir_all(dir).unwrap();
        let log = dir.join("calls.log");
        std::fs::write(dir.join("sinks.json"), SINKS).unwrap();
        std::fs::write(dir.join("sources.json"), SOURCES).unwrap();
        std::fs::write(dir.join("sink-inputs.json"), INPUTS).unwrap();
        std::fs::write(
            dir.join("info.json"),
            r#"{"default_sink_name":"arctis_eq","default_source_name":"mic"}"#,
        )
        .unwrap();
        let body = format!(
            r#"echo "$*" >> {log}
case "$*" in
  "--format=json info") cat {dir}/info.json ;;
  "--format=json list sinks") cat {dir}/sinks.json ;;
  "--format=json list sources") cat {dir}/sources.json ;;
  "--format=json list sink-inputs") cat {dir}/sink-inputs.json ;;
  "--format=json list source-outputs") echo '[]' ;;
  "subscribe") echo "Event 'change' on sink #1"; echo "Event 'new' on sink-input #2"; echo "noise"; echo "Event 'remove' on source-output #3" ;;
  *"refuse"*) exit 1 ;;
esac
"#,
            log = log.display(),
            dir = dir.display()
        );
        let program = super::super::command::fake_program(dir, "pactl", &body);
        (Pactl::with_program(program), log)
    }

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("openbar-pulse-{name}-{}", std::process::id()))
    }

    #[test]
    fn every_command_reaches_pactl_with_the_right_arguments() {
        let dir = temp("args");
        let (pactl, log) = stand_in(&dir);
        assert!(pactl.available());
        pactl.set_volume(Kind::Output, "arctis_eq", 0.5).unwrap();
        pactl.set_volume(Kind::Input, "mic", 1.0).unwrap();
        pactl.set_volume(Kind::Stream, "91", 0.25).unwrap();
        pactl.set_mute(Kind::Output, "arctis_eq", true).unwrap();
        pactl.set_mute(Kind::Input, "mic", false).unwrap();
        pactl.set_mute(Kind::Stream, "91", true).unwrap();
        pactl.set_default(Kind::Output, "arctis_eq").unwrap();
        pactl.set_default(Kind::Input, "mic").unwrap();
        assert!(pactl.set_default(Kind::Stream, "91").is_err());
        assert_eq!(
            std::fs::read_to_string(&log)
                .unwrap()
                .lines()
                .skip(1)
                .collect::<Vec<_>>(),
            [
                "set-sink-volume arctis_eq 32768",
                "set-source-volume mic 65536",
                "set-sink-input-volume 91 16384",
                "set-sink-mute arctis_eq 1",
                "set-source-mute mic 0",
                "set-sink-input-mute 91 1",
                "set-default-sink arctis_eq",
                "set-default-source mic",
            ]
        );
        // A refused command is an error that names it.
        assert_eq!(
            pactl.set_mute(Kind::Output, "refuse", true),
            Err("pactl set-sink-mute failed".into())
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_lists_come_through_the_program() {
        let dir = temp("lists");
        let (pactl, _) = stand_in(&dir);
        assert_eq!(
            pactl.defaults(),
            Some((Some("arctis_eq".into()), Some("mic".into())))
        );
        assert_eq!(pactl.outputs().unwrap().len(), 2);
        assert_eq!(pactl.inputs().unwrap().len(), 2);
        assert_eq!(pactl.streams().unwrap().len(), 2);
        assert_eq!(pactl.recordings(), Some(Vec::new()));
        // No pactl at all: nothing, not a crash.
        let missing = Pactl::with_program("/nonexistent/pactl");
        assert!(!missing.available());
        assert!(missing.outputs().is_none() && missing.defaults().is_none());
        assert!(missing.set_volume(Kind::Output, "x", 0.5).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn subscription_events_are_passed_on_and_noise_is_not() {
        let dir = temp("events");
        let (pactl, _) = stand_in(&dir);
        let seen = std::sync::Mutex::new(Vec::new());
        // Stops at the third event; the stand-in prints four lines, one of them noise.
        listen(pactl.program.clone(), Duration::from_millis(10), |event| {
            let mut seen = seen.lock().unwrap();
            seen.push(event);
            seen.len() < 3
        });
        assert_eq!(
            *seen.lock().unwrap(),
            [Event::Devices, Event::Streams, Event::Recordings]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_subscription_is_taken_up_again_when_the_server_goes_away() {
        let dir = temp("retry");
        let (pactl, _) = stand_in(&dir);
        let count = std::sync::atomic::AtomicUsize::new(0);
        // The stand-in ends after its lines; listening goes on to the next run.
        listen(pactl.program.clone(), Duration::from_millis(10), |_| {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1 < 7
        });
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 7);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[ignore = "talks to the sound server of this desktop; OPENBAR_LIVE_SINK names a sink made for the test"]
    fn the_sound_server_is_read_and_a_test_sink_and_stream_are_obeyed() {
        let pactl = Pactl::default();
        assert!(pactl.available(), "no pactl, or a pactl older than 16");
        let (sink, inputs) = (pactl.outputs().unwrap(), pactl.inputs().unwrap());
        println!("defaults: {:?}", pactl.defaults());
        println!(
            "outputs: {:?}",
            sink.iter()
                .map(|n| (&n.id, n.volume, n.muted, &n.bus, &n.form_factor))
                .collect::<Vec<_>>()
        );
        println!(
            "inputs: {:?}",
            inputs
                .iter()
                .map(|n| (&n.id, n.volume, n.muted))
                .collect::<Vec<_>>()
        );
        println!(
            "streams: {:?}",
            pactl
                .streams()
                .unwrap()
                .iter()
                .map(|s| (s.index, &s.app, &s.binary, s.corked))
                .collect::<Vec<_>>()
        );
        let Ok(name) = std::env::var("OPENBAR_LIVE_SINK") else {
            return;
        };
        // Changes reach the listener.
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        subscribe(move |event| {
            let _ = tx.lock().unwrap().send(event);
        });
        std::thread::sleep(Duration::from_millis(800));
        let read = |pactl: &Pactl| {
            pactl
                .outputs()
                .unwrap()
                .into_iter()
                .find(|n| n.id == name)
                .expect("the test sink is not there")
        };
        pactl.set_volume(Kind::Output, &name, 0.37).unwrap();
        pactl.set_mute(Kind::Output, &name, true).unwrap();
        let node = read(&pactl);
        assert!(
            (node.volume - 0.37).abs() < 0.001,
            "volume read back as {}",
            node.volume
        );
        assert!(node.muted);
        pactl.set_mute(Kind::Output, &name, false).unwrap();
        assert!(!read(&pactl).muted);
        assert!(
            rx.recv_timeout(Duration::from_secs(3))
                .is_ok_and(|e| e == Event::Devices),
            "no change event reached the listener"
        );
        // A stream named by OPENBAR_LIVE_APP, playing silence into the test sink.
        if let Ok(app) = std::env::var("OPENBAR_LIVE_APP") {
            let stream = pactl
                .streams()
                .unwrap()
                .into_iter()
                .find(|s| s.app == app)
                .expect("the test stream is not there");
            pactl
                .set_volume(Kind::Stream, &stream.index.to_string(), 0.5)
                .unwrap();
            pactl
                .set_mute(Kind::Stream, &stream.index.to_string(), true)
                .unwrap();
            let after = pactl
                .streams()
                .unwrap()
                .into_iter()
                .find(|s| s.app == app)
                .unwrap();
            assert!((after.volume - 0.5).abs() < 0.001 && after.muted);
            assert!(!after.corked);
            println!(
                "stream {} controlled: volume {} muted {}",
                after.index, after.volume, after.muted
            );
        }
        println!("test sink controlled: ok");
    }
}
