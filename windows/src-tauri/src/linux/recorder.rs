// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The screen recorder on Linux, where Windows has Windows.Graphics.Capture,
//! WASAPI and Media Foundation: `ffmpeg` grabs the area from the X server
//! (x11grab), takes the system's sound (the default output's monitor) and the
//! microphone from the sound server, mixes them, and writes H.264 and AAC into
//! an MP4. `recorder/mod.rs` uses this as its `engine`.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gtk::glib::{self, UserDirectory};

use super::pulse::{Pactl, Pulse};
use crate::capture::Rect;
use crate::recorder::{AudioChoice, Ended};

const FRAMES_PER_SECOND: u32 = 30;
/// How long ffmpeg may take to show its first frame.
const FIRST_FRAME_WAIT: Duration = Duration::from_secs(5);
/// How long it may take to close the file once told to stop.
const FINISH_WAIT: Duration = Duration::from_secs(15);

/// The person's Videos folder, which the recording goes to.
pub fn videos_folder() -> Option<PathBuf> {
    glib::user_special_dir(UserDirectory::Videos)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Videos")))
}

/// The sound sources to record, by the names the sound server knows them.
#[derive(Debug, Default, PartialEq)]
struct Sources {
    system: Option<String>,
    microphone: Option<String>,
}

/// What was asked for, as sources, and whether a sound the person asked for
/// could not be found (the recording goes on without it).
fn sources(choice: AudioChoice, pulse: &dyn Pulse) -> (Sources, bool) {
    let (sink, source) = pulse.defaults().unwrap_or((None, None));
    let system = sink
        .filter(|_| choice.system)
        .map(|sink| format!("{sink}.monitor"));
    // The default source can be the monitor of an output when there is no microphone.
    let microphone = source.filter(|_| choice.microphone).filter(|name| {
        pulse
            .inputs()
            .is_some_and(|inputs| inputs.iter().any(|input| &input.id == name))
    });
    let missing =
        (choice.system && system.is_none()) || (choice.microphone && microphone.is_none());
    (Sources { system, microphone }, missing)
}

/// `:0` is display 0, which x11grab names `:0.0`; an offset is `+x,y` after it.
fn grab_input(display: &str, x: i32, y: i32) -> String {
    let screen = if display.contains('.') {
        display.to_string()
    } else {
        format!("{display}.0")
    };
    format!("{screen}+{x},{y}")
}

/// The ffmpeg command line: `size` pixels of the display from `origin`, the
/// sources mixed into one track, to `path`.
fn arguments(
    display: &str,
    origin: (i32, i32),
    size: (i32, i32),
    sources: &Sources,
    path: &Path,
) -> Vec<String> {
    let mut args: Vec<String> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostats",
        "-progress",
        "pipe:1",
        "-stats_period",
        "0.1",
        "-y",
        "-f",
        "x11grab",
        "-framerate",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.push(FRAMES_PER_SECOND.to_string());
    args.extend(["-draw_mouse", "1", "-video_size"].map(String::from));
    args.push(format!("{}x{}", size.0, size.1));
    args.extend(["-i".to_string(), grab_input(display, origin.0, origin.1)]);
    let sounds: Vec<&String> = [&sources.system, &sources.microphone]
        .into_iter()
        .flatten()
        .collect();
    for name in &sounds {
        args.extend(["-thread_queue_size", "1024", "-f", "pulse", "-i"].map(String::from));
        args.push((*name).clone());
    }
    args.extend(
        [
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-tune",
            "zerolatency",
            "-crf",
            "23",
            "-pix_fmt",
            "yuv420p",
        ]
        .map(String::from),
    );
    match sounds.len() {
        0 => args.push("-an".into()),
        1 => args.extend(
            ["-map", "0:v", "-map", "1:a", "-c:a", "aac", "-b:a", "160k"].map(String::from),
        ),
        _ => args.extend(
            [
                "-filter_complex",
                "[1:a][2:a]amix=inputs=2:duration=longest:normalize=0[mixed]",
                "-map",
                "0:v",
                "-map",
                "[mixed]",
                "-c:a",
                "aac",
                "-b:a",
                "160k",
            ]
            .map(String::from),
        ),
    }
    args.push(path.to_string_lossy().into_owned());
    args
}

/// What `-progress` has said so far: the frames written and the seconds of
/// recording in the file.
#[derive(Default)]
struct Progress {
    frames: AtomicU64,
    microseconds: AtomicU64,
}

impl Progress {
    /// One `key=value` line of ffmpeg's progress report.
    fn line(&self, line: &str) {
        let Some((key, value)) = line.trim().split_once('=') else {
            return;
        };
        let Ok(number) = value.trim().parse::<u64>() else {
            return;
        };
        match key {
            "frame" => self.frames.store(number, Ordering::SeqCst),
            "out_time_us" => self.microseconds.store(number, Ordering::SeqCst),
            _ => {}
        }
    }

    fn seconds(&self) -> f64 {
        self.microseconds.load(Ordering::SeqCst) as f64 / 1e6
    }
}

/// The last words ffmpeg said on its error output, which say why it failed.
fn last_words(lines: &Mutex<Vec<String>>) -> Option<String> {
    let lines = lines.lock().unwrap();
    let words: Vec<&str> = lines
        .iter()
        .rev()
        .take(2)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    (!words.is_empty()).then(|| words.join(": "))
}

fn failed(error: impl Into<String>, seconds: f64, missing: bool) -> Ended {
    Ended {
        path: None,
        folder: None,
        seconds,
        missing_sound: missing,
        error: Some(error.into()),
    }
}

struct Waits {
    first_frame: Duration,
    finish: Duration,
}

/// Runs `program` as a recording until `stop`: answers `ready` when its first
/// frame is in, tells it to finish with `q`, and waits for the file to close.
fn session(
    program: &Path,
    args: &[String],
    path: &Path,
    stop: &Arc<AtomicBool>,
    ready: Sender<Result<(), String>>,
    missing: bool,
    waits: &Waits,
) -> Ended {
    let mut child: Child = match Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let error = format!("ffmpeg did not start: {error}");
            let _ = ready.send(Err(error.clone()));
            return failed(error, 0.0, missing);
        }
    };
    let progress = Arc::new(Progress::default());
    let said = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut readers = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        let progress = progress.clone();
        readers.push(std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                progress.line(&line);
            }
        }));
    }
    if let Some(stderr) = child.stderr.take() {
        let said = said.clone();
        readers.push(std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let mut lines = said.lock().unwrap();
                lines.push(line);
                let extra = lines.len().saturating_sub(20);
                lines.drain(..extra);
            }
        }));
    }
    let finish = |child: &mut Child, readers: Vec<std::thread::JoinHandle<()>>| {
        let _ = child.kill();
        let _ = child.wait();
        readers.into_iter().for_each(|reader| drop(reader.join()));
        let _ = std::fs::remove_file(path);
    };

    // Until the first frame is in, a failure is the person's to hear of at once.
    let began = Instant::now();
    while progress.frames.load(Ordering::SeqCst) == 0 {
        if let Ok(Some(status)) = child.try_wait() {
            readers.into_iter().for_each(|reader| drop(reader.join()));
            let error = last_words(&said).unwrap_or_else(|| format!("ffmpeg stopped ({status})"));
            let _ = std::fs::remove_file(path);
            let _ = ready.send(Err(error.clone()));
            return failed(error, 0.0, missing);
        }
        if began.elapsed() > waits.first_frame {
            finish(&mut child, readers);
            let error = "the display sent no picture".to_string();
            let _ = ready.send(Err(error.clone()));
            return failed(error, 0.0, missing);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = ready.send(Ok(()));
    let started = Instant::now();

    // Recording, until told to stop or until ffmpeg gives up on its own.
    let mut stopped_by_itself = None;
    while !stop.load(Ordering::SeqCst) {
        if let Ok(Some(status)) = child.try_wait() {
            stopped_by_itself = Some(status);
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut problem = None;
    if stopped_by_itself.is_none() {
        // `q` makes ffmpeg write the end of the file and exit.
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(b"q");
            let _ = stdin.flush();
        }
        let deadline = Instant::now() + waits.finish;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    stopped_by_itself = Some(status);
                    break;
                }
                Ok(None) if Instant::now() > deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    problem = Some("ffmpeg did not close the file in time".to_string());
                    break;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                Err(_) => break,
            }
        }
    }
    readers.into_iter().for_each(|reader| drop(reader.join()));
    let seconds = match progress.seconds() {
        0.0 => started.elapsed().as_secs_f64(),
        seconds => seconds,
    };
    if let Some(status) = stopped_by_itself
        && !status.success()
    {
        problem = Some(last_words(&said).unwrap_or_else(|| format!("ffmpeg stopped ({status})")));
    }
    // A file ffmpeg did not close has no end, and no player opens it.
    if problem.is_some() || !path.is_file() {
        let _ = std::fs::remove_file(path);
        return failed(
            problem.unwrap_or_else(|| "nothing was recorded".into()),
            seconds,
            missing,
        );
    }
    Ended {
        path: Some(path.to_string_lossy().into_owned()),
        folder: None,
        seconds,
        missing_sound: missing,
        error: None,
    }
}

fn find_ffmpeg() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join("ffmpeg"))
            .find(|candidate| candidate.is_file())
    })
}

/// Runs a whole recording. `ready` answers once the first frame is in, so the
/// person hears of a failure at once.
pub fn record(
    monitor: Rect,
    area: Rect,
    choice: AudioChoice,
    path: &Path,
    stop: &Arc<AtomicBool>,
    ready: Sender<Result<(), String>>,
) -> Ended {
    let Some(program) = find_ffmpeg() else {
        let error = "recording needs ffmpeg, which is not installed".to_string();
        let _ = ready.send(Err(error.clone()));
        return failed(error, 0.0, false);
    };
    let Some(display) = std::env::var("DISPLAY").ok().filter(|d| !d.is_empty()) else {
        let error = "there is no X display to record".to_string();
        let _ = ready.send(Err(error.clone()));
        return failed(error, 0.0, false);
    };
    let (sources, missing) = sources(choice, &Pactl::default());
    let args = arguments(
        &display,
        (monitor.x + area.x, monitor.y + area.y),
        (area.width, area.height),
        &sources,
        path,
    );
    session(
        &program,
        &args,
        path,
        stop,
        ready,
        missing,
        &Waits {
            first_frame: FIRST_FRAME_WAIT,
            finish: FINISH_WAIT,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::super::command::fake_program;
    use super::super::pulse::{Kind, Node, Stream};
    use super::*;
    use std::sync::mpsc::channel;
    use x11rb::connection::Connection as _;
    use x11rb::protocol::xproto::{
        ChangeWindowAttributesAux, ConnectionExt as _, CreateWindowAux, WindowClass,
    };

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("openbar-recorder-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[derive(Default)]
    struct Server {
        defaults: Option<(Option<String>, Option<String>)>,
        inputs: Option<Vec<Node>>,
    }

    impl Pulse for Server {
        fn defaults(&self) -> Option<(Option<String>, Option<String>)> {
            self.defaults.clone()
        }
        fn outputs(&self) -> Option<Vec<Node>> {
            None
        }
        fn inputs(&self) -> Option<Vec<Node>> {
            self.inputs.clone()
        }
        fn streams(&self) -> Option<Vec<Stream>> {
            None
        }
        fn recordings(&self) -> Option<Vec<Stream>> {
            None
        }
        fn set_volume(&self, _: Kind, _: &str, _: f32) -> Result<(), String> {
            Err("unused".into())
        }
        fn set_mute(&self, _: Kind, _: &str, _: bool) -> Result<(), String> {
            Err("unused".into())
        }
        fn set_default(&self, _: Kind, _: &str) -> Result<(), String> {
            Err("unused".into())
        }
    }

    fn microphone(id: &str) -> Node {
        Node {
            id: id.into(),
            ..Default::default()
        }
    }

    fn server() -> Server {
        Server {
            defaults: Some((Some("speakers".into()), Some("headset".into()))),
            inputs: Some(vec![microphone("headset")]),
        }
    }

    const BOTH: AudioChoice = AudioChoice {
        system: true,
        microphone: true,
    };
    const NONE: AudioChoice = AudioChoice {
        system: false,
        microphone: false,
    };

    #[test]
    fn the_system_sound_is_the_default_outputs_monitor_and_the_microphone_the_default_input() {
        let (found, missing) = sources(BOTH, &server());
        assert_eq!(found.system.as_deref(), Some("speakers.monitor"));
        assert_eq!(found.microphone.as_deref(), Some("headset"));
        assert!(!missing);
        let (found, missing) = sources(NONE, &server());
        assert_eq!(found, Sources::default());
        assert!(!missing, "nothing was asked for, so nothing is missing");
        let (only_system, _) = sources(
            AudioChoice {
                system: true,
                microphone: false,
            },
            &server(),
        );
        assert_eq!(only_system.microphone, None);
    }

    #[test]
    fn a_sound_that_cannot_be_found_is_reported_and_left_out() {
        // The default input is the monitor of an output, not a microphone.
        let no_microphone = Server {
            inputs: Some(vec![microphone("other")]),
            ..server()
        };
        let (found, missing) = sources(BOTH, &no_microphone);
        assert_eq!(found.microphone, None);
        assert_eq!(found.system.as_deref(), Some("speakers.monitor"));
        assert!(missing);
        // A server that cannot be asked has no sound to give.
        let (found, missing) = sources(BOTH, &Server::default());
        assert_eq!(found, Sources::default());
        assert!(missing);
    }

    #[test]
    fn the_grab_names_the_display_and_the_corner_of_the_area() {
        assert_eq!(grab_input(":0", 1920, 40), ":0.0+1920,40");
        assert_eq!(grab_input(":97.1", 0, 0), ":97.1+0,0");
        assert_eq!(grab_input("localhost:10", 5, 6), "localhost:10.0+5,6");
    }

    fn has_pair(args: &[String], first: &str, second: &str) -> bool {
        args.windows(2)
            .any(|pair| pair[0] == first && pair[1] == second)
    }

    #[test]
    fn the_command_line_grabs_the_area_and_mixes_the_sound_that_was_asked_for() {
        let path = Path::new("/videos/a b.mp4");
        let silent = arguments(":0", (100, 50), (640, 360), &Sources::default(), path);
        assert!(has_pair(&silent, "-video_size", "640x360"));
        assert!(has_pair(&silent, "-i", ":0.0+100,50"));
        assert!(silent.contains(&"-an".to_string()));
        assert!(!silent.contains(&"pulse".to_string()));
        assert_eq!(silent.last().map(String::as_str), Some("/videos/a b.mp4"));

        let one = arguments(
            ":0",
            (0, 0),
            (2, 2),
            &Sources {
                system: Some("speakers.monitor".into()),
                microphone: None,
            },
            path,
        );
        assert!(has_pair(&one, "-i", "speakers.monitor"));
        assert!(has_pair(&one, "-map", "1:a"));
        assert!(!one.iter().any(|arg| arg.contains("amix")));

        let two = arguments(
            ":0",
            (0, 0),
            (2, 2),
            &Sources {
                system: Some("speakers.monitor".into()),
                microphone: Some("headset".into()),
            },
            path,
        );
        assert!(has_pair(&two, "-i", "speakers.monitor") && has_pair(&two, "-i", "headset"));
        assert!(two.iter().any(|arg| arg.contains("amix=inputs=2")));
        assert!(has_pair(&two, "-map", "[mixed]"));
        // The grab is the first input, so the sounds are inputs 1 and 2.
        let inputs: Vec<&String> = two
            .iter()
            .enumerate()
            .filter(|(_, a)| *a == "-i")
            .map(|(i, _)| &two[i + 1])
            .collect();
        assert_eq!(inputs, [":0.0+0,0", "speakers.monitor", "headset"]);
    }

    #[test]
    fn progress_lines_give_the_frames_and_the_seconds() {
        let progress = Progress::default();
        for line in [
            "frame=12",
            "fps=30.0",
            "out_time_us=1500000",
            "progress=continue",
            "junk",
            "frame=oops",
        ] {
            progress.line(line);
        }
        assert_eq!(progress.frames.load(Ordering::SeqCst), 12);
        assert_eq!(progress.seconds(), 1.5);
    }

    fn quick() -> Waits {
        Waits {
            first_frame: Duration::from_millis(600),
            finish: Duration::from_millis(600),
        }
    }

    /// A stand-in ffmpeg: says it has a frame, waits to be told `q`, writes the file named by its last argument.
    const WELL_BEHAVED: &str = r#"
for a; do out="$a"; done
echo "frame=1"; echo "out_time_us=200000"; echo "progress=continue"
read -r key
echo "frame=30"; echo "out_time_us=1000000"; echo "progress=end"
echo video > "$out"
exit 0
"#;

    #[test]
    fn a_recording_runs_until_stopped_and_ends_with_the_file() {
        let dir = scratch("good");
        let program = fake_program(&dir, "ffmpeg", WELL_BEHAVED);
        let path = dir.join("out.mp4");
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let handle = {
            let (path, stop) = (path.clone(), stop.clone());
            std::thread::spawn(move || {
                session(
                    &program,
                    &[path.to_string_lossy().into_owned()],
                    &path,
                    &stop,
                    tx,
                    false,
                    &quick(),
                )
            })
        };
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            Ok(()),
            "ready once the first frame is in"
        );
        assert!(!handle.is_finished(), "it goes on until told to stop");
        stop.store(true, Ordering::SeqCst);
        let ended = handle.join().unwrap();
        assert_eq!(ended.path.as_deref(), Some(path.to_string_lossy().as_ref()));
        assert_eq!(ended.error, None);
        assert_eq!(
            ended.seconds, 1.0,
            "the time in the file, not the time on the clock"
        );
        assert!(path.is_file());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failure_before_the_first_frame_is_told_at_once_with_ffmpegs_own_words() {
        let dir = scratch("early");
        let program = fake_program(
            &dir,
            "ffmpeg",
            "echo 'Cannot open display :9' >&2\nexit 1\n",
        );
        let path = dir.join("out.mp4");
        std::fs::write(&path, "partial").unwrap();
        let (tx, rx) = channel();
        let ended = session(
            &program,
            &[],
            &path,
            &Arc::new(AtomicBool::new(false)),
            tx,
            true,
            &quick(),
        );
        assert_eq!(
            rx.recv().unwrap(),
            Err("Cannot open display :9".to_string())
        );
        assert_eq!(ended.error.as_deref(), Some("Cannot open display :9"));
        assert!(ended.missing_sound, "what was missing is passed on");
        assert!(!path.exists(), "no half file is left");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_display_that_sends_no_picture_ends_the_recording_and_the_program() {
        let dir = scratch("silent");
        let program = fake_program(&dir, "ffmpeg", "exec sleep 30\n");
        let (tx, rx) = channel();
        let started = Instant::now();
        let ended = session(
            &program,
            &[],
            &dir.join("out.mp4"),
            &Arc::new(AtomicBool::new(false)),
            tx,
            false,
            &quick(),
        );
        assert_eq!(
            rx.recv().unwrap(),
            Err("the display sent no picture".to_string())
        );
        assert_eq!(ended.error.as_deref(), Some("the display sent no picture"));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the stand-in was killed, not waited for"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn ffmpeg_giving_up_mid_recording_is_an_error_with_no_file() {
        let dir = scratch("dies");
        let body = "for a; do out=\"$a\"; done\necho video > \"$out\"\necho frame=1\nsleep 0.2\necho 'X connection broken' >&2\nexit 1\n";
        let program = fake_program(&dir, "ffmpeg", body);
        let path = dir.join("out.mp4");
        let (tx, rx) = channel();
        let ended = session(
            &program,
            &[path.to_string_lossy().into_owned()],
            &path,
            &Arc::new(AtomicBool::new(false)),
            tx,
            false,
            &quick(),
        );
        assert_eq!(rx.recv().unwrap(), Ok(()), "it had begun");
        assert_eq!(ended.error.as_deref(), Some("X connection broken"));
        assert_eq!(ended.path, None);
        assert!(!path.exists(), "a file with no end is not offered");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_ffmpeg_that_will_not_close_the_file_is_killed_and_the_recording_fails() {
        let dir = scratch("stuck");
        // Has its first frame, then ignores `q`.
        let program = fake_program(
            &dir,
            "ffmpeg",
            "echo frame=1\nwhile true; do sleep 1; done\n",
        );
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let handle = {
            let (dir, stop) = (dir.clone(), stop.clone());
            std::thread::spawn(move || {
                session(
                    &program,
                    &[],
                    &dir.join("out.mp4"),
                    &stop,
                    tx,
                    false,
                    &quick(),
                )
            })
        };
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), Ok(()));
        stop.store(true, Ordering::SeqCst);
        let ended = handle.join().unwrap();
        assert_eq!(
            ended.error.as_deref(),
            Some("ffmpeg did not close the file in time")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_program_is_told_at_once() {
        let (tx, rx) = channel();
        let ended = session(
            Path::new("/nonexistent/openbar/ffmpeg"),
            &[],
            Path::new("/nonexistent/out.mp4"),
            &Arc::new(AtomicBool::new(false)),
            tx,
            false,
            &quick(),
        );
        assert!(
            rx.recv()
                .unwrap()
                .unwrap_err()
                .starts_with("ffmpeg did not start")
        );
        assert!(ended.error.is_some());
    }

    fn probe(file: &Path, arguments: &[&str]) -> String {
        let output = Command::new("ffprobe")
            .args(["-v", "error"])
            .args(arguments)
            .arg(file)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// The loudest sample of the file's sound, in dB, from ffmpeg's own statistics.
    fn peak_db(file: &Path) -> Option<f64> {
        let output = Command::new("ffmpeg")
            .args(["-v", "info", "-nostats", "-i"])
            .arg(file)
            .args(["-vn", "-af", "astats=metadata=1", "-f", "null", "-"])
            .output()
            .ok()?;
        String::from_utf8_lossy(&output.stderr)
            .lines()
            .rfind(|line| line.contains("Peak level dB"))?
            .rsplit(':')
            .next()?
            .trim()
            .parse()
            .ok()
    }

    fn pactl(arguments: &[&str]) -> String {
        let output = Command::new("pactl").args(arguments).output().unwrap();
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// Three seconds of 440 Hz into the sink `sink`, at half the full level.
    fn play_tone(sink: &str) -> (Child, std::thread::JoinHandle<()>) {
        let mut player = Command::new("pacat")
            .args([
                "-p",
                &format!("--device={sink}"),
                "--rate=48000",
                "--format=s16le",
                "--channels=2",
            ])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = player.stdin.take().unwrap();
        let tone = std::thread::spawn(move || {
            let samples: Vec<u8> = (0..48_000 * 3)
                .flat_map(|n| {
                    let v = ((n as f64 * 440.0 * std::f64::consts::TAU / 48_000.0).sin()
                        * 0.5
                        * 32767.0) as i16;
                    let b = v.to_le_bytes();
                    [b[0], b[1], b[0], b[1]]
                })
                .collect();
            let _ = stdin.write_all(&samples);
        });
        (player, tone)
    }

    /// The colour of one pixel of the video, half a second in.
    fn pixel(file: &Path, x: u32, y: u32) -> [u8; 3] {
        let output = Command::new("ffmpeg")
            .args(["-v", "error", "-ss", "0.5", "-i"])
            .arg(file)
            .args(["-frames:v", "1", "-vf"])
            .arg(format!("format=rgb24,crop=1:1:{x}:{y}"))
            .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"])
            .output()
            .unwrap();
        output.stdout[..3].try_into().expect("a pixel")
    }

    #[test]
    #[ignore = "records a nested X server with the real ffmpeg, a sink of its own and a tone: OPENBAR_LIVE_RECORD=:97"]
    fn live_a_recording_has_the_size_the_video_the_sound_and_the_length() {
        let display = std::env::var("OPENBAR_LIVE_RECORD").expect("OPENBAR_LIVE_RECORD");
        assert!(
            display != ":0" && !display.starts_with(":0."),
            "refusing {display}: the real display's picture is the person's own"
        );
        let module = pactl(&[
            "load-module",
            "module-null-sink",
            "sink_name=openbar_rec_test",
        ]);
        let (mut player, tone) = play_tone("openbar_rec_test");
        // A black screen with a red square at (200, 100), 100 by 100: the area recorded
        // starts at (190, 90), so the square must be 10 in and the corner black.
        let (x, screen) = x11rb::connect(Some(&display)).unwrap();
        let root = x.setup().roots[screen].root;
        x.change_window_attributes(root, &ChangeWindowAttributesAux::new().background_pixel(0))
            .unwrap();
        x.clear_area(false, root, 0, 0, 0, 0).unwrap();
        let square = x.generate_id().unwrap();
        x.create_window(
            0,
            square,
            root,
            200,
            100,
            100,
            100,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new().background_pixel(0x00ff_0000),
        )
        .unwrap();
        x.map_window(square).unwrap();
        x.flush().unwrap();
        let dir = scratch("live");
        let path = dir.join("live.mp4");
        let sources = Sources {
            system: Some("openbar_rec_test.monitor".into()),
            microphone: None,
        };
        let args = arguments(&display, (190, 90), (120, 120), &sources, &path);
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let program = find_ffmpeg().expect("ffmpeg");
        let handle = {
            let (path, stop) = (path.clone(), stop.clone());
            std::thread::spawn(move || {
                session(
                    &program,
                    &args,
                    &path,
                    &stop,
                    tx,
                    false,
                    &Waits {
                        first_frame: FIRST_FRAME_WAIT,
                        finish: FINISH_WAIT,
                    },
                )
            })
        };
        let ready = rx.recv_timeout(Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(2500));
        stop.store(true, Ordering::SeqCst);
        let ended = handle.join().unwrap();
        let _ = player.kill();
        let _ = player.wait();
        let _ = tone.join();
        pactl(&["unload-module", &module]);
        assert_eq!(ready, Ok(Ok(())), "{ended:?}");
        assert_eq!(ended.error, None);
        assert!(path.is_file());
        let streams = probe(
            &path,
            &[
                "-show_entries",
                "stream=codec_type,codec_name,width,height",
                "-of",
                "csv=p=0",
            ],
        );
        assert!(
            [
                "h264,video,120,120",
                "video,h264,120,120",
                "h264,120,120,video"
            ]
            .iter()
            .any(|shape| streams.contains(shape)),
            "{streams}"
        );
        assert!(streams.contains("aac"), "{streams}");
        let length: f64 = probe(
            &path,
            &["-show_entries", "format=duration", "-of", "csv=p=0"],
        )
        .trim()
        .parse()
        .unwrap();
        assert!((2.0..3.6).contains(&length), "{length} s");
        assert!(
            (ended.seconds - length).abs() < 1.0,
            "{} vs {length}",
            ended.seconds
        );
        let inside = pixel(&path, 60, 60);
        let corner = pixel(&path, 2, 2);
        assert!(
            inside[0] > 200 && inside[1] < 60 && inside[2] < 60,
            "inside the square: {inside:?}"
        );
        assert!(corner.iter().all(|&c| c < 40), "outside it: {corner:?}");
        let peak = peak_db(&path).expect("the file has a sound track");
        assert!(peak > -12.0, "the tone is in the recording: peak {peak} dB");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    #[ignore = "records a nested X server with the real ffmpeg and two sinks of their own: OPENBAR_LIVE_RECORD=:97"]
    fn live_the_microphone_is_mixed_in_with_the_system_sound_at_its_own_level() {
        let display = std::env::var("OPENBAR_LIVE_RECORD").expect("OPENBAR_LIVE_RECORD");
        assert!(
            display != ":0" && !display.starts_with(":0."),
            "refusing {display}: the real display's picture is the person's own"
        );
        let silent = pactl(&[
            "load-module",
            "module-null-sink",
            "sink_name=openbar_rec_test_system",
        ]);
        let loud = pactl(&[
            "load-module",
            "module-null-sink",
            "sink_name=openbar_rec_test_mic",
        ]);
        // The tone is only in the second sink, which stands in for the microphone.
        let (mut player, tone) = play_tone("openbar_rec_test_mic");
        let dir = scratch("live-mix");
        let path = dir.join("mix.mp4");
        let sources = Sources {
            system: Some("openbar_rec_test_system.monitor".into()),
            microphone: Some("openbar_rec_test_mic.monitor".into()),
        };
        let args = arguments(&display, (0, 0), (120, 120), &sources, &path);
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let program = find_ffmpeg().expect("ffmpeg");
        let handle = {
            let (path, stop) = (path.clone(), stop.clone());
            std::thread::spawn(move || {
                session(
                    &program,
                    &args,
                    &path,
                    &stop,
                    tx,
                    false,
                    &Waits {
                        first_frame: FIRST_FRAME_WAIT,
                        finish: FINISH_WAIT,
                    },
                )
            })
        };
        let ready = rx.recv_timeout(Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(2000));
        stop.store(true, Ordering::SeqCst);
        let ended = handle.join().unwrap();
        let _ = player.kill();
        let _ = player.wait();
        let _ = tone.join();
        pactl(&["unload-module", &silent]);
        pactl(&["unload-module", &loud]);
        assert_eq!(ready, Ok(Ok(())), "{ended:?}");
        assert_eq!(ended.error, None);
        let peak = peak_db(&path).expect("the file has a sound track");
        // Half the full level is -6 dB; mixing two inputs the usual way would halve it again.
        assert!(
            peak > -9.0,
            "the microphone's tone comes through at its own level: peak {peak} dB"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_videos_folder_is_absolute() {
        if let Some(folder) = videos_folder() {
            assert!(folder.is_absolute());
        }
    }
}
