// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The default microphone in shared mode, converted by WASAPI to 16 kHz
//! mono floats. Shared mode leaves the device and other apps alone: a call
//! or Discord keeps its microphone, and the input device never changes.
//! The audio stays in memory and is dropped with the recording.

use super::{HARD_CAP_SECONDS, Problem, SAMPLE_RATE, SPEECH_RMS, level, read_wav, rms};
use crate::com::Apartment;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{E_ACCESSDENIED, ERROR_NOT_FOUND};
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};

const FLOAT_TAG: u16 = 3;
/// One meter reading per 100 ms of audio.
const WINDOW: usize = SAMPLE_RATE as usize / 10;

pub enum Source {
    Microphone,
    /// A WAV file at its own pace, then quiet, as a microphone would hear it.
    File(PathBuf),
}

/// What the microphone heard so far, filled from its own thread.
pub struct Audio {
    samples: Arc<Mutex<Vec<f32>>>,
    heard: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Audio {
    pub fn snapshot(&self) -> Vec<f32> {
        self.samples.lock().unwrap().clone()
    }

    pub fn len(&self) -> usize {
        self.samples.lock().unwrap().len()
    }

    /// Whether anything as loud as a voice came in.
    pub fn heard_speech(&self) -> bool {
        self.heard.load(Ordering::SeqCst)
    }

    /// The microphone closed: stopped, capped or lost.
    pub fn is_done(&self) -> bool {
        self.done.load(Ordering::SeqCst)
    }

    pub fn join(&self) {
        if let Some(thread) = self.thread.lock().unwrap().take() {
            let _ = thread.join();
        }
    }
}

/// Opens the source and keeps reading until `stop`; reports the level of
/// each 100 ms through `meter`. Fails at once when the microphone can't open.
pub fn start(
    source: Source,
    stop: Arc<AtomicBool>,
    meter: impl Fn(f32) + Send + 'static,
) -> Result<Audio, Problem> {
    let samples = Arc::new(Mutex::new(Vec::with_capacity(SAMPLE_RATE as usize * 10)));
    let heard = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let (opened, opening) = std::sync::mpsc::channel();
    let thread = {
        let (samples, heard, done) = (samples.clone(), heard.clone(), done.clone());
        std::thread::spawn(move || {
            let mut sink = Sink {
                samples,
                heard,
                window: Vec::with_capacity(WINDOW),
                meter,
            };
            match source {
                Source::Microphone => microphone(&mut sink, &stop, opened),
                Source::File(path) => file(&mut sink, &stop, &path, opened),
            }
            done.store(true, Ordering::SeqCst);
        })
    };
    match opening.recv() {
        Ok(Ok(())) => Ok(Audio {
            samples,
            heard,
            done,
            thread: Mutex::new(Some(thread)),
        }),
        Ok(Err(problem)) => {
            let _ = thread.join();
            Err(problem)
        }
        Err(_) => Err(Problem::Failed),
    }
}

struct Sink<F: Fn(f32)> {
    samples: Arc<Mutex<Vec<f32>>>,
    heard: Arc<AtomicBool>,
    window: Vec<f32>,
    meter: F,
}

impl<F: Fn(f32)> Sink<F> {
    fn push(&mut self, chunk: &[f32]) {
        self.samples.lock().unwrap().extend_from_slice(chunk);
        for &sample in chunk {
            self.window.push(sample);
            if self.window.len() == WINDOW {
                let loudness = rms(&self.window);
                if loudness >= SPEECH_RMS {
                    self.heard.store(true, Ordering::SeqCst);
                }
                (self.meter)(level(loudness));
                self.window.clear();
            }
        }
    }

    fn full(&self) -> bool {
        self.samples.lock().unwrap().len() >= (SAMPLE_RATE * HARD_CAP_SECONDS) as usize
    }
}

fn format() -> WAVEFORMATEX {
    WAVEFORMATEX {
        wFormatTag: FLOAT_TAG,
        nChannels: 1,
        nSamplesPerSec: SAMPLE_RATE,
        nAvgBytesPerSec: SAMPLE_RATE * 4,
        nBlockAlign: 4,
        wBitsPerSample: 32,
        cbSize: 0,
    }
}

fn problem(error: &windows::core::Error) -> Problem {
    match error.code() {
        code if code == E_ACCESSDENIED => Problem::MicrophoneDenied,
        // No capture device at all (HRESULT_FROM_WIN32(ERROR_NOT_FOUND)), or it just went away.
        code if code == ERROR_NOT_FOUND.to_hresult() || code == AUDCLNT_E_DEVICE_INVALIDATED => {
            Problem::NoMicrophone
        }
        _ => Problem::Failed,
    }
}

fn open() -> windows::core::Result<(IAudioClient, IAudioCaptureClient)> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eCapture, eCommunications)
            .or_else(|_| enumerator.GetDefaultAudioEndpoint(eCapture, eConsole))?;
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
            // One second of room, so a slow read never drops audio.
            10_000_000,
            0,
            &format(),
            None,
        )?;
        let reader: IAudioCaptureClient = client.GetService()?;
        client.Start()?;
        Ok((client, reader))
    }
}

fn microphone<F: Fn(f32)>(
    sink: &mut Sink<F>,
    stop: &AtomicBool,
    opened: std::sync::mpsc::Sender<Result<(), Problem>>,
) {
    let Ok(_apartment) = Apartment::mta() else {
        let _ = opened.send(Err(Problem::Failed));
        return;
    };
    let (client, reader) = match open() {
        Ok(pair) => pair,
        Err(error) => {
            let _ = opened.send(Err(problem(&error)));
            return;
        }
    };
    let _ = opened.send(Ok(()));
    while !stop.load(Ordering::SeqCst) && !sink.full() {
        if drain(&reader, sink).is_err() {
            break; // the device went away
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    unsafe {
        let _ = client.Stop();
    }
}

fn drain<F: Fn(f32)>(
    reader: &IAudioCaptureClient,
    sink: &mut Sink<F>,
) -> windows::core::Result<()> {
    unsafe {
        while reader.GetNextPacketSize()? > 0 {
            let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0, 0);
            reader.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
            let count = frames as usize;
            if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                sink.push(&vec![0.0; count]);
            } else {
                sink.push(std::slice::from_raw_parts(data.cast::<f32>(), count));
            }
            reader.ReleaseBuffer(frames)?;
        }
    }
    Ok(())
}

fn file<F: Fn(f32)>(
    sink: &mut Sink<F>,
    stop: &AtomicBool,
    path: &std::path::Path,
    opened: std::sync::mpsc::Sender<Result<(), Problem>>,
) {
    let audio = match std::fs::read(path)
        .map_err(|e| e.to_string())
        .and_then(|b| read_wav(&b))
    {
        Ok(audio) => audio,
        Err(_) => {
            let _ = opened.send(Err(Problem::NoMicrophone));
            return;
        }
    };
    let _ = opened.send(Ok(()));
    let started = Instant::now();
    let mut at = 0usize;
    while !stop.load(Ordering::SeqCst) && !sink.full() {
        let due = (started.elapsed().as_secs_f64() * SAMPLE_RATE as f64) as usize;
        if due > at {
            let mut chunk = vec![0.0f32; due - at];
            let from = at.min(audio.len());
            let to = due.min(audio.len());
            chunk[..to - from].copy_from_slice(&audio[from..to]);
            sink.push(&chunk);
            at = due;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
