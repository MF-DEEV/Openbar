// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The screen recorder (Sources/Vorssaint/Services/Recorder): an area of one
//! display, the system's sound and the microphone, into an MP4 in the Videos
//! folder. Windows.Graphics.Capture gives the frames, WASAPI the sound and a
//! Media Foundation sink writer encodes H.264 and AAC. Nothing runs between
//! recordings. The Mac's editor and its zoom, cursor and export tools aren't
//! ported: a recording ends as a plain file.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::capture::Rect;

#[cfg(windows)]
mod audio;
#[cfg(windows)]
mod engine;

pub const STARTED: &str = "recorder://started";
pub const ENDED: &str = "recorder://ended";

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;
/// What the sound waits for, so a late packet still lands in its place.
const MIX_DELAY_SECONDS: f64 = 0.1;
/// A source further behind than this drops its oldest sound.
const MAX_QUEUE_SECONDS: usize = 1;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AudioChoice {
    pub system: bool,
    pub microphone: bool,
}

/// How a recording ended.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ended {
    pub path: Option<String>,
    pub folder: Option<String>,
    pub seconds: f64,
    /// A sound the person asked for that could not be opened.
    pub missing_sound: bool,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Shared {
    stop: Mutex<Option<Arc<AtomicBool>>>,
}

/// H.264 needs even sizes; the odd pixel goes rather than stretching the rest.
pub fn even_area(area: Rect) -> Option<Rect> {
    let width = area.width & !1;
    let height = area.height & !1;
    (width >= 2 && height >= 2).then_some(Rect {
        x: area.x,
        y: area.y,
        width,
        height,
    })
}

/// BGRA rows `pitch` bytes apart into NV12, by BT.709 limited range.
pub fn bgra_to_nv12(bgra: &[u8], pitch: usize, width: usize, height: usize, out: &mut [u8]) {
    let (luma, chroma) = out.split_at_mut(width * height);
    for y in 0..height {
        let row = &bgra[y * pitch..y * pitch + width * 4];
        for x in 0..width {
            let pixel = &row[x * 4..x * 4 + 4];
            let (b, g, r) = (pixel[0] as i32, pixel[1] as i32, pixel[2] as i32);
            luma[y * width + x] = ((47 * r + 157 * g + 16 * b + 128) >> 8).clamp(0, 255) as u8 + 16;
        }
    }
    for y in (0..height).step_by(2) {
        let row = &bgra[y * pitch..y * pitch + width * 4];
        let chroma_row = &mut chroma[y / 2 * width..y / 2 * width + width];
        for x in (0..width).step_by(2) {
            let pixel = &row[x * 4..x * 4 + 4];
            let (b, g, r) = (pixel[0] as i32, pixel[1] as i32, pixel[2] as i32);
            chroma_row[x] = (((-26 * r - 87 * g + 112 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
            chroma_row[x + 1] =
                (((112 * r - 102 * g - 10 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
        }
    }
}

/// Sound from one source, interleaved stereo, waiting to be mixed.
#[derive(Default)]
pub struct Queue(pub Mutex<VecDeque<f32>>);

impl Queue {
    pub fn push(&self, samples: &[f32]) {
        let mut queue = self.0.lock().unwrap();
        queue.extend(samples.iter().copied());
        let most = MAX_QUEUE_SECONDS * SAMPLE_RATE as usize * CHANNELS;
        let extra = queue.len().saturating_sub(most);
        queue.drain(..extra);
    }
}

/// Puts the sound on the recording's clock: silence is real time too, and
/// loopback sends nothing while nothing plays.
#[derive(Default)]
pub struct Mixer {
    written: u64,
}

impl Mixer {
    /// The next stereo block due at `elapsed` seconds, or none yet.
    pub fn next(
        &mut self,
        elapsed: f64,
        queues: &[Arc<Queue>],
        flush: bool,
    ) -> Option<(u64, Vec<i16>)> {
        let delay = if flush { 0.0 } else { MIX_DELAY_SECONDS };
        let due = ((elapsed - delay).max(0.0) * SAMPLE_RATE as f64) as u64;
        let frames = due.saturating_sub(self.written);
        if frames < 480 && !(flush && frames > 0) {
            return None;
        }
        let count = frames as usize * CHANNELS;
        let mut mixed = vec![0f32; count];
        for queue in queues {
            let mut queue = queue.0.lock().unwrap();
            let take = count.min(queue.len());
            for (slot, sample) in mixed.iter_mut().zip(queue.drain(..take)) {
                *slot += if sample.is_finite() { sample } else { 0.0 };
            }
        }
        let start = self.written;
        self.written = due;
        Some((
            start,
            mixed
                .into_iter()
                .map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
                .collect(),
        ))
    }
}

/// `dir/name.mp4`, or `dir/name 2.mp4` and so on when taken.
pub fn free_video_path(dir: &Path, name: &str) -> PathBuf {
    let stem: String = name
        .chars()
        .map(|c| {
            if r#"<>:"/\|?*"#.contains(c) || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let stem = if stem.trim().is_empty() {
        "Screen Recording".to_owned()
    } else {
        stem
    };
    let first = dir.join(format!("{stem}.mp4"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| dir.join(format!("{stem} {n}.mp4")))
        .find(|path| !path.exists())
        .unwrap()
}

/// Starts recording `area` (pixels of the display at `monitor`). Returns once
/// the first frame is known to come, or why it can't.
pub fn start<R: Runtime>(
    app: &AppHandle<R>,
    monitor: Rect,
    area: Rect,
    audio: AudioChoice,
    name: &str,
) -> Result<(), String> {
    let shared = app.state::<Shared>();
    let mut slot = shared.stop.lock().unwrap();
    if slot.is_some() {
        return Err("a recording is already running".into());
    }
    let area = even_area(area).ok_or("the area is too small to record")?;
    let stop = Arc::new(AtomicBool::new(false));
    run(app.clone(), monitor, area, audio, name, stop.clone())?;
    *slot = Some(stop);
    let _ = app.emit(STARTED, ());
    Ok(())
}

#[cfg(windows)]
fn run<R: Runtime>(
    app: AppHandle<R>,
    monitor: Rect,
    area: Rect,
    audio: AudioChoice,
    name: &str,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    let folder = engine::videos_folder().ok_or("the Videos folder is unavailable")?;
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let path = free_video_path(&folder, name);
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("recorder".into())
        .spawn(move || {
            let ended = engine::record(monitor, area, audio, &path, &stop, ready_tx);
            let ended = Ended {
                folder: Some(folder.to_string_lossy().into_owned()),
                ..ended
            };
            *app.state::<Shared>().stop.lock().unwrap() = None;
            let _ = app.emit(ENDED, ended);
        })
        .map_err(|e| e.to_string())?;
    ready_rx
        .recv()
        .map_err(|_| "the recorder stopped".to_owned())?
}

#[cfg(not(windows))]
fn run<R: Runtime>(
    _app: AppHandle<R>,
    _monitor: Rect,
    _area: Rect,
    _audio: AudioChoice,
    _name: &str,
    _stop: Arc<AtomicBool>,
) -> Result<(), String> {
    Err("recording needs Windows".into())
}

#[tauri::command]
pub fn recorder_stop(shared: tauri::State<'_, Shared>) {
    if let Some(stop) = shared.stop.lock().unwrap().as_ref() {
        stop.store(true, Ordering::SeqCst);
    }
}

#[tauri::command]
pub fn recorder_running(shared: tauri::State<'_, Shared>) -> bool {
    shared.stop.lock().unwrap().is_some()
}

/// Shows the recording in Explorer. Only a video in the Videos folder's
/// recordings, so a page can't point this at anything else.
#[tauri::command]
pub fn recorder_reveal(path: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        let folder = engine::videos_folder().ok_or("the Videos folder is unavailable")?;
        let target = PathBuf::from(&path);
        if target.parent() != Some(folder.as_path())
            || target.extension().and_then(|e| e.to_str()) != Some("mp4")
        {
            return Err("not a recording".into());
        }
        crate::files::shell::reveal(vec![path]);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("needs Windows".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn areas_go_even_and_too_small_ones_go_away() {
        let area = |width, height| Rect {
            x: 3,
            y: 5,
            width,
            height,
        };
        assert_eq!(even_area(area(101, 57)), Some(area(100, 56)));
        assert_eq!(even_area(area(100, 56)), Some(area(100, 56)));
        assert_eq!(even_area(area(2, 2)), Some(area(2, 2)));
        assert_eq!(even_area(area(1, 50)), None);
        assert_eq!(even_area(area(50, 1)), None);
        assert_eq!(even_area(area(0, 0)), None);
    }

    #[test]
    fn colors_land_in_nv12() {
        // Four pixels of one colour with a pitch wider than the row.
        let frame = |b: u8, g: u8, r: u8| {
            let mut data = Vec::new();
            for _ in 0..2 {
                for _ in 0..2 {
                    data.extend_from_slice(&[b, g, r, 255]);
                }
                data.extend_from_slice(&[9; 8]);
            }
            data
        };
        let mut out = [0u8; 6];
        bgra_to_nv12(&frame(0, 0, 0), 16, 2, 2, &mut out);
        assert_eq!(out, [16, 16, 16, 16, 128, 128], "black");
        bgra_to_nv12(&frame(255, 255, 255), 16, 2, 2, &mut out);
        assert!((233..=236).contains(&out[0]), "white luma {}", out[0]);
        assert!(
            out[4].abs_diff(128) <= 1 && out[5].abs_diff(128) <= 1,
            "white is neutral"
        );
        bgra_to_nv12(&frame(0, 0, 255), 16, 2, 2, &mut out);
        assert!(out[5] > 200 && out[4] < 128, "red pulls V up and U down");
    }

    #[test]
    fn the_mixer_keeps_real_time_through_silence() {
        let queue = Arc::new(Queue::default());
        let queues = vec![queue.clone()];
        let mut mixer = Mixer::default();
        assert!(
            mixer.next(0.05, &queues, false).is_none(),
            "the delay holds the start"
        );
        let (start, block) = mixer.next(0.2, &queues, false).unwrap();
        assert_eq!(start, 0);
        assert_eq!(block.len(), 4800 * 2, "100 ms of silence still counts");
        assert!(block.iter().all(|s| *s == 0));
        queue.push(&[0.5; 960]);
        let (start, block) = mixer.next(0.22, &queues, false).unwrap();
        assert_eq!(start, 4800);
        assert!(block.len() >= 960, "{}", block.len());
        assert!(
            block[..960].iter().all(|s| *s == 16383),
            "what was queued comes first"
        );
        assert!(block[960..].iter().all(|s| *s == 0), "then silence");
    }

    #[test]
    fn sources_add_up_clamped_and_bad_samples_are_silent() {
        let (a, b) = (Arc::new(Queue::default()), Arc::new(Queue::default()));
        a.push(&[0.8, f32::NAN]);
        b.push(&[0.8, 0.25]);
        let mut mixer = Mixer::default();
        let (_, block) = mixer.next(0.2, &[a, b], false).unwrap();
        assert_eq!(block[0], 32767);
        assert_eq!(block[1], (0.25f32 * 32767.0) as i16);
    }

    #[test]
    fn flushing_takes_the_delayed_tail_and_a_full_queue_drops_its_oldest() {
        let queue = Arc::new(Queue::default());
        let mut mixer = Mixer::default();
        mixer.next(0.2, std::slice::from_ref(&queue), false);
        let (start, block) = mixer.next(0.2, std::slice::from_ref(&queue), true).unwrap();
        assert_eq!(start, 4800);
        assert_eq!(block.len(), 4800 * 2);
        assert!(
            mixer
                .next(0.2, std::slice::from_ref(&queue), true)
                .is_none()
        );
        queue.push(&vec![0.1; 2 * SAMPLE_RATE as usize * CHANNELS]);
        assert_eq!(
            queue.0.lock().unwrap().len(),
            SAMPLE_RATE as usize * CHANNELS
        );
    }

    #[test]
    fn names_are_cleaned_and_never_overwrite() {
        let dir = std::env::temp_dir().join(format!("vors-rec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = free_video_path(&dir, r"a:b/c");
        assert_eq!(first.file_name().unwrap(), "a-b-c.mp4");
        std::fs::write(&first, b"x").unwrap();
        assert_eq!(
            free_video_path(&dir, r"a:b/c").file_name().unwrap(),
            "a-b-c 2.mp4"
        );
        assert_eq!(
            free_video_path(&dir, "  ").file_name().unwrap(),
            "Screen Recording.mp4"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
