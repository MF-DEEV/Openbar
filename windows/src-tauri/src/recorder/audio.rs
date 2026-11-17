// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! One sound source on its own thread: the default output's loopback or the
//! default microphone, converted by WASAPI to the recording's format.

use super::{CHANNELS, Queue, SAMPLE_RATE};
use crate::com::Apartment;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};

const FLOAT_TAG: u16 = 3;

fn format() -> WAVEFORMATEX {
    let align = (CHANNELS * 4) as u16;
    WAVEFORMATEX {
        wFormatTag: FLOAT_TAG,
        nChannels: CHANNELS as u16,
        nSamplesPerSec: SAMPLE_RATE,
        nAvgBytesPerSec: SAMPLE_RATE * align as u32,
        nBlockAlign: align,
        wBitsPerSample: 32,
        cbSize: 0,
    }
}

/// Opens the source; `ok` reports whether it did before sound is expected.
pub fn spawn(
    loopback: bool,
    queue: Arc<Queue>,
    stop: Arc<AtomicBool>,
    ok: std::sync::mpsc::Sender<bool>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let _apartment = Apartment::mta();
        let opened = open(loopback);
        let _ = ok.send(opened.is_ok());
        let Ok((client, reader)) = opened else {
            return;
        };
        while !stop.load(Ordering::SeqCst) {
            if drain(&reader, &queue).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        unsafe {
            let _ = client.Stop();
        }
    })
}

fn open(loopback: bool) -> windows::core::Result<(IAudioClient, IAudioCaptureClient)> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = if loopback {
            enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?
        } else {
            enumerator.GetDefaultAudioEndpoint(eCapture, eConsole)?
        };
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
        let mut flags =
            AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
        if loopback {
            flags |= AUDCLNT_STREAMFLAGS_LOOPBACK;
        }
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            flags,
            2_000_000,
            0,
            &format(),
            None,
        )?;
        let reader: IAudioCaptureClient = client.GetService()?;
        client.Start()?;
        Ok((client, reader))
    }
}

fn drain(reader: &IAudioCaptureClient, queue: &Queue) -> windows::core::Result<()> {
    unsafe {
        while reader.GetNextPacketSize()? > 0 {
            let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0, 0);
            reader.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
            let count = frames as usize * CHANNELS;
            if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                queue.push(&vec![0.0; count]);
            } else {
                queue.push(std::slice::from_raw_parts(data.cast::<f32>(), count));
            }
            reader.ReleaseBuffer(frames)?;
        }
    }
    Ok(())
}
