// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Render loopback only: never opens a microphone. All COM clients stay on
//! the monitoring MTA. RAII stops streams before releasing their interfaces.
use super::spectrum::{BANDS, Spectrum};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Media::Audio::*,
        System::{
            Com::{
                BLOB, CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, StructuredStorage::PROPVARIANT,
            },
            Variant::VT_BLOB,
        },
    },
    core::{GUID, HRESULT, Interface, Ref, Result, implement},
};

#[implement(IActivateAudioInterfaceCompletionHandler)]
struct Completion {
    ready: mpsc::Sender<()>,
    // The async operation may outlive our bounded wait. Keep its blob alive.
    _params: Box<AUDIOCLIENT_ACTIVATION_PARAMS>,
}
impl IActivateAudioInterfaceCompletionHandler_Impl for Completion_Impl {
    fn ActivateCompleted(&self, _: Ref<IActivateAudioInterfaceAsyncOperation>) -> Result<()> {
        let _ = self.ready.send(());
        Ok(())
    }
}

fn process_client(pid: u32) -> Result<IAudioClient> {
    let mut params = Box::new(AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: pid,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    });
    // This VT_BLOB borrows the handler-owned Rust box. PROPVARIANT's normal
    // destructor would incorrectly CoTaskMemFree that pointer a second time.
    let mut variant = std::mem::ManuallyDrop::new(PROPVARIANT::default());
    unsafe {
        (*variant.Anonymous.Anonymous).vt = VT_BLOB;
        (*variant.Anonymous.Anonymous).Anonymous.blob = BLOB {
            cbSize: std::mem::size_of_val(params.as_ref()) as u32,
            pBlobData: params.as_mut() as *mut _ as *mut u8,
        };
    }
    let (tx, rx) = mpsc::channel();
    let handler: IActivateAudioInterfaceCompletionHandler = Completion {
        ready: tx,
        _params: params,
    }
    .into();
    let operation = unsafe {
        ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            Some(&*variant),
            &handler,
        )?
    };
    rx.recv_timeout(Duration::from_millis(750))
        .map_err(|_| windows::core::Error::from_hresult(HRESULT(0x800705B4u32 as i32)))?;
    let mut status = HRESULT(0);
    let mut interface = None;
    unsafe {
        operation.GetActivateResult(&mut status, &mut interface)?;
    }
    status.ok()?;
    interface.ok_or_else(windows::core::Error::empty)?.cast()
}

pub fn default_device() -> Result<(IMMDevice, String)> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
        let raw = device.GetId()?;
        let id = raw.to_string();
        CoTaskMemFree(Some(raw.0.cast()));
        Ok((device, id?))
    }
}

pub struct Capture {
    client: IAudioClient,
    reader: IAudioCaptureClient,
    channels: usize,
    bytes: usize,
    float: bool,
    spectrum: Spectrum,
    last_packet: Instant,
}

impl Capture {
    pub fn process(pid: u32) -> Result<Self> {
        let client = process_client(pid)?;
        let format = WAVEFORMATEX {
            wFormatTag: 1,
            nChannels: 2,
            nSamplesPerSec: 48000,
            nAvgBytesPerSec: 192000,
            nBlockAlign: 4,
            wBitsPerSample: 16,
            cbSize: 0,
        };
        Self::initialize(client, &format, false, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM)
    }

    pub fn system(device: &IMMDevice) -> Result<Self> {
        unsafe {
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
            let format = client.GetMixFormat()?;
            let result = (|| {
                let tag = (*format).wFormatTag;
                let float = if tag == 0xfffe && (*format).cbSize >= 22 {
                    let sub = (*(format as *const WAVEFORMATEXTENSIBLE)).SubFormat;
                    if sub == GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71) {
                        true
                    } else if sub == GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71) {
                        false
                    } else {
                        return Err(windows::core::Error::from_hresult(HRESULT(
                            0x80004001u32 as i32,
                        )));
                    }
                } else {
                    tag == 3
                };
                if ![1, 3, 0xfffe].contains(&tag) {
                    return Err(windows::core::Error::from_hresult(HRESULT(
                        0x80004001u32 as i32,
                    )));
                }
                Self::initialize(client, format, float, 0)
            })();
            CoTaskMemFree(Some(format.cast()));
            result
        }
    }

    fn initialize(
        client: IAudioClient,
        format: *const WAVEFORMATEX,
        float: bool,
        flags: u32,
    ) -> Result<Self> {
        unsafe {
            let channels = (*format).nChannels as usize;
            let bits = (*format).wBitsPerSample;
            let rate = (*format).nSamplesPerSec;
            let bytes = bits as usize / 8;
            if channels == 0
                || rate == 0
                || (float && bits != 32)
                || (!float && ![16, 24, 32].contains(&bits))
                || (*format).nBlockAlign as usize != channels * bytes
            {
                return Err(windows::core::Error::from_hresult(HRESULT(
                    0x80004001u32 as i32,
                )));
            }
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK | flags,
                1_000_000,
                0,
                format,
                None,
            )?;
            let reader = client.GetService()?;
            let mut capture = Self {
                client,
                reader,
                channels,
                bytes,
                float,
                spectrum: Spectrum::new(rate),
                last_packet: Instant::now(),
            };
            capture.client.Start()?;
            capture.spectrum.clear();
            Ok(capture)
        }
    }

    pub fn drain(&mut self) -> Result<()> {
        unsafe {
            // Bound each pass, even if an endpoint is producing faster than we drain.
            for _ in 0..64 {
                if self.reader.GetNextPacketSize()? == 0 {
                    break;
                }
                let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0, 0);
                self.reader
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                if flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0 {
                    self.spectrum.clear();
                }
                let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null();
                for frame in 0..frames as usize {
                    let mut mono = 0.;
                    if !silent {
                        for channel in 0..self.channels {
                            let ptr = data.add((frame * self.channels + channel) * self.bytes);
                            let value = if self.float {
                                std::ptr::read_unaligned(ptr.cast::<f32>())
                            } else {
                                match self.bytes {
                                    2 => {
                                        std::ptr::read_unaligned(ptr.cast::<i16>()) as f32 / 32768.
                                    }
                                    3 => {
                                        i32::from_le_bytes([0, *ptr, *ptr.add(1), *ptr.add(2)])
                                            as f32
                                            / 2147483648.
                                    }
                                    _ => {
                                        std::ptr::read_unaligned(ptr.cast::<i32>()) as f32
                                            / 2147483648.
                                    }
                                }
                            };
                            mono += if value.is_finite() { value } else { 0. };
                        }
                    }
                    self.spectrum.push(mono / self.channels as f32);
                }
                self.reader.ReleaseBuffer(frames)?;
                self.last_packet = Instant::now();
            }
            // Some drivers stop delivering packets on silence. Never reuse old audio.
            if self.last_packet.elapsed() > Duration::from_millis(100) {
                self.spectrum.clear();
            }
            Ok(())
        }
    }

    pub fn levels(&self) -> [f32; BANDS] {
        self.spectrum.levels()
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = unsafe { self.client.Stop() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests play brief tones through the real render endpoint. They are
    // opt-in so ordinary CI never requires hardware or makes audible sounds.
    struct Render {
        client: IAudioClient,
        writer: IAudioRenderClient,
        phase: usize,
    }
    impl Render {
        fn new(device: &IMMDevice) -> Result<Self> {
            unsafe {
                let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
                let format = WAVEFORMATEX {
                    wFormatTag: 1,
                    nChannels: 2,
                    nSamplesPerSec: 48000,
                    nAvgBytesPerSec: 192000,
                    nBlockAlign: 4,
                    wBitsPerSample: 16,
                    cbSize: 0,
                };
                client.Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
                    1_000_000,
                    0,
                    &format,
                    None,
                )?;
                let writer = client.GetService()?;
                client.Start()?;
                Ok(Self {
                    client,
                    writer,
                    phase: 0,
                })
            }
        }
        fn fill(&mut self, amplitude: f32) -> Result<()> {
            unsafe {
                let available = self.client.GetBufferSize()? - self.client.GetCurrentPadding()?;
                if available == 0 {
                    return Ok(());
                }
                let data = self.writer.GetBuffer(available)? as *mut i16;
                for frame in 0..available as usize {
                    let value = (amplitude
                        * 32767.
                        * (std::f32::consts::TAU * 750. * self.phase as f32 / 48000.).sin())
                        as i16;
                    *data.add(frame * 2) = value;
                    *data.add(frame * 2 + 1) = value;
                    self.phase += 1;
                }
                self.writer.ReleaseBuffer(available, 0)
            }
        }
    }
    impl Drop for Render {
        fn drop(&mut self) {
            let _ = unsafe { self.client.Stop() };
        }
    }

    fn measure(render: &mut Render, capture: &mut Capture, amplitude: f32) -> [f32; BANDS] {
        let until = Instant::now() + Duration::from_millis(450);
        while Instant::now() < until {
            render.fill(amplitude).unwrap();
            capture.drain().unwrap();
            std::thread::sleep(Duration::from_millis(8));
        }
        capture.levels()
    }

    fn verify(mut capture: Capture, mut render: Render) {
        let loud = measure(&mut render, &mut capture, 0.5);
        let quiet = measure(&mut render, &mut capture, 0.015);
        let silent = measure(&mut render, &mut capture, 0.);
        println!("750 Hz: loud={loud:?}, quiet={quiet:?}, silent={silent:?}");
        assert!(
            loud[3] > quiet[3] + 0.15,
            "loopback must distinguish amplitude"
        );
        assert!(quiet[3] > 0., "quiet audio should still register");
        assert!(silent[3] < 0.01, "silence should clear the spectrum");
        drop(render);
        std::thread::sleep(Duration::from_millis(200));
        capture.drain().unwrap();
        // Drain once more after a driver which queued a final trailing packet.
        std::thread::sleep(Duration::from_millis(150));
        capture.drain().unwrap();
        assert!(
            capture.levels()[3] < 0.01,
            "stopped render must not leave stale levels"
        );
    }

    #[test]
    #[ignore = "plays brief tones on the interactive Windows output device"]
    fn real_process_loopback_tracks_volume_and_silence() {
        let _apartment = crate::com::Apartment::mta().unwrap();
        let (device, _) = default_device().unwrap();
        let render = Render::new(&device).unwrap();
        let capture = Capture::process(std::process::id()).unwrap();
        verify(capture, render);
    }

    #[test]
    #[ignore = "requires a real SMTC music player actively playing audible music"]
    fn real_music_player_resolves_and_produces_a_spectrum() {
        use windows::Media::Control::{
            GlobalSystemMediaTransportControlsSessionManager as Manager,
            GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
        };
        let _apartment = crate::com::Apartment::mta().unwrap();
        let manager = Manager::RequestAsync().unwrap().join().unwrap();
        let session = manager.GetCurrentSession().unwrap();
        assert_eq!(
            session.GetPlaybackInfo().unwrap().PlaybackStatus().unwrap(),
            Status::Playing
        );
        let source = session.SourceAppUserModelId().unwrap().to_string();
        let pids = crate::audio::player_processes(source.clone()).unwrap();
        assert!(!pids.is_empty(), "No WASAPI session resolved for {source}");
        let mut captures: Vec<_> = pids
            .iter()
            .map(|&pid| Capture::process(pid).unwrap())
            .collect();
        let until = Instant::now() + Duration::from_secs(2);
        let mut peak = 0f32;
        while Instant::now() < until {
            for capture in &mut captures {
                capture.drain().unwrap();
                peak = peak.max(capture.levels().into_iter().fold(0f32, f32::max));
            }
            std::thread::sleep(Duration::from_millis(33));
        }
        println!("Actual music: source={source}, processes={pids:?}, strongest band={peak}");
        assert!(peak > 0.01, "The player's loopback stayed silent");
    }

    #[test]
    #[ignore = "plays brief tones; requires otherwise quiet Windows system audio"]
    fn real_system_loopback_tracks_volume_and_silence() {
        let _apartment = crate::com::Apartment::mta().unwrap();
        let (device, _) = default_device().unwrap();
        let render = Render::new(&device).unwrap();
        let capture = Capture::system(&device).unwrap();
        verify(capture, render);
    }
}
