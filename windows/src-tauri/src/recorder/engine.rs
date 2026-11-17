// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The recording thread: frames from Windows.Graphics.Capture, copied out of
//! the GPU, converted and written, with the mixed sound between them. All
//! writing stays on this one thread.

use super::{AudioChoice, CHANNELS, Ended, Mixer, Queue, SAMPLE_RATE, audio, bgra_to_nv12};
use crate::capture::Rect;
use crate::com::Apartment;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};
use windows::Graphics::Capture::{
    Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Foundation::{HMODULE, POINT};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP,
};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromPoint};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::UI::Shell::{FOLDERID_Videos, KF_FLAG_CREATE, SHGetKnownFolderPath};
use windows::core::{HSTRING, Interface, factory};

const FPS: u32 = 30;
const FRAME_GAP: Duration = Duration::from_millis(1000 / FPS as u64);
/// A still screen sends no frames; the video keeps moving anyway.
const REPEAT_AFTER: Duration = Duration::from_millis(500);
const FIRST_FRAME_WAIT: Duration = Duration::from_secs(5);

type Result<T> = std::result::Result<T, String>;

fn text(error: windows::core::Error) -> String {
    error.message().to_string()
}

pub fn videos_folder() -> Option<PathBuf> {
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_Videos, KF_FLAG_CREATE, None).ok()?;
        let out = path.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(path.0 as *const _));
        out.map(|p| p.join("Screen Recordings"))
    }
}

fn device() -> Result<ID3D11Device> {
    let make = |kind: D3D_DRIVER_TYPE| unsafe {
        let mut device = None;
        D3D11CreateDevice(
            None,
            kind,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )
        .map(|_| device)
    };
    make(D3D_DRIVER_TYPE_HARDWARE)
        .or_else(|_| make(D3D_DRIVER_TYPE_WARP))
        .map_err(text)?
        .ok_or_else(|| "no Direct3D device".to_owned())
}

struct Capture {
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    context: ID3D11DeviceContext,
    staging: ID3D11Texture2D,
    area: Rect,
    size: (i32, i32),
}

impl Capture {
    fn start(monitor: Rect, area: Rect) -> Result<Self> {
        unsafe {
            let device = device()?;
            let context = device.GetImmediateContext().map_err(text)?;
            let dxgi: IDXGIDevice = device.cast().map_err(text)?;
            let inspectable = CreateDirect3D11DeviceFromDXGIDevice(&dxgi).map_err(text)?;
            let d3d: IDirect3DDevice = inspectable.cast().map_err(text)?;
            let center = POINT {
                x: monitor.x + monitor.width / 2,
                y: monitor.y + monitor.height / 2,
            };
            let handle = MonitorFromPoint(center, MONITOR_DEFAULTTONEAREST);
            let interop =
                factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>().map_err(text)?;
            let item: GraphicsCaptureItem = interop.CreateForMonitor(handle).map_err(text)?;
            let size = item.Size().map_err(text)?;
            let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
                &d3d,
                DirectXPixelFormat::B8G8R8A8UIntNormalized,
                2,
                size,
            )
            .map_err(text)?;
            let session = pool.CreateCaptureSession(&item).map_err(text)?;
            let _ = session.SetIsCursorCaptureEnabled(true);
            // Windows 11's yellow frame; older builds refuse, and that's fine.
            let _ = session.SetIsBorderRequired(false);
            let desc = D3D11_TEXTURE2D_DESC {
                Width: area.width as u32,
                Height: area.height as u32,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
            };
            let mut staging = None;
            device
                .CreateTexture2D(&desc, None, Some(&mut staging))
                .map_err(text)?;
            session.StartCapture().map_err(text)?;
            Ok(Self {
                pool,
                session,
                context,
                staging: staging.ok_or("no staging texture")?,
                area,
                size: (size.Width, size.Height),
            })
        }
    }

    /// The newest frame since the last look, in NV12. `Err` when the display changed size.
    fn latest(&self, out: &mut [u8]) -> Result<bool> {
        let mut newest = None;
        while let Ok(frame) = self.pool.TryGetNextFrame() {
            newest = Some(frame);
        }
        let Some(frame) = newest else {
            return Ok(false);
        };
        let size = frame.ContentSize().map_err(text)?;
        if (size.Width, size.Height) != self.size {
            return Err("the display changed size".into());
        }
        unsafe {
            let access: IDirect3DDxgiInterfaceAccess =
                frame.Surface().map_err(text)?.cast().map_err(text)?;
            let texture: ID3D11Texture2D = access.GetInterface().map_err(text)?;
            let area = self.area;
            let source = D3D11_BOX {
                left: area.x as u32,
                top: area.y as u32,
                front: 0,
                right: (area.x + area.width) as u32,
                bottom: (area.y + area.height) as u32,
                back: 1,
            };
            self.context.CopySubresourceRegion(
                &self.staging,
                0,
                0,
                0,
                0,
                &texture,
                0,
                Some(&source),
            );
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .map_err(text)?;
            let bytes = std::slice::from_raw_parts(
                mapped.pData as *const u8,
                mapped.RowPitch as usize * area.height as usize,
            );
            bgra_to_nv12(
                bytes,
                mapped.RowPitch as usize,
                area.width as usize,
                area.height as usize,
                out,
            );
            self.context.Unmap(&self.staging, 0);
        }
        Ok(true)
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

struct Writer {
    sink: IMFSinkWriter,
    video: u32,
    audio: Option<u32>,
    frame_bytes: usize,
}

fn media_type() -> Result<IMFMediaType> {
    unsafe { MFCreateMediaType().map_err(text) }
}

impl Writer {
    fn open(path: &Path, width: u32, height: u32, sound: bool) -> Result<Self> {
        unsafe {
            let mut attributes = None;
            MFCreateAttributes(&mut attributes, 1).map_err(text)?;
            let attributes = attributes.ok_or("no attributes")?;
            attributes
                .SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)
                .map_err(text)?;
            let sink =
                MFCreateSinkWriterFromURL(&HSTRING::from(path.as_os_str()), None, &attributes)
                    .map_err(text)?;
            let size = ((width as u64) << 32) | height as u64;
            let rate = ((FPS as u64) << 32) | 1;
            let square = (1u64 << 32) | 1;
            let video_type = |subtype: &windows::core::GUID| -> Result<IMFMediaType> {
                let t = media_type()?;
                t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                    .map_err(text)?;
                t.SetGUID(&MF_MT_SUBTYPE, subtype).map_err(text)?;
                t.SetUINT64(&MF_MT_FRAME_SIZE, size).map_err(text)?;
                t.SetUINT64(&MF_MT_FRAME_RATE, rate).map_err(text)?;
                t.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, square)
                    .map_err(text)?;
                t.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
                    .map_err(text)?;
                Ok(t)
            };
            let out = video_type(&MFVideoFormat_H264)?;
            let bits =
                (width as u64 * height as u64 * FPS as u64 / 5).clamp(2_000_000, 40_000_000) as u32;
            out.SetUINT32(&MF_MT_AVG_BITRATE, bits).map_err(text)?;
            let video = sink.AddStream(&out).map_err(text)?;
            let input = video_type(&MFVideoFormat_NV12)?;
            input
                .SetUINT32(&MF_MT_DEFAULT_STRIDE, width)
                .map_err(text)?;
            sink.SetInputMediaType(video, &input, None).map_err(text)?;
            let audio = if sound {
                let t = media_type()?;
                t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
                    .map_err(text)?;
                t.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)
                    .map_err(text)?;
                t.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)
                    .map_err(text)?;
                t.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, SAMPLE_RATE)
                    .map_err(text)?;
                t.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, CHANNELS as u32)
                    .map_err(text)?;
                t.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 24_000)
                    .map_err(text)?;
                let stream = sink.AddStream(&t).map_err(text)?;
                let pcm = media_type()?;
                pcm.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
                    .map_err(text)?;
                pcm.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)
                    .map_err(text)?;
                pcm.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)
                    .map_err(text)?;
                pcm.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, SAMPLE_RATE)
                    .map_err(text)?;
                pcm.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, CHANNELS as u32)
                    .map_err(text)?;
                pcm.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, (CHANNELS * 2) as u32)
                    .map_err(text)?;
                pcm.SetUINT32(
                    &MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
                    SAMPLE_RATE * (CHANNELS as u32) * 2,
                )
                .map_err(text)?;
                sink.SetInputMediaType(stream, &pcm, None).map_err(text)?;
                Some(stream)
            } else {
                None
            };
            sink.BeginWriting().map_err(text)?;
            Ok(Self {
                sink,
                video,
                audio,
                frame_bytes: width as usize * height as usize * 3 / 2,
            })
        }
    }

    fn write(&self, stream: u32, bytes: &[u8], at_100ns: i64, duration: i64) -> Result<()> {
        unsafe {
            let buffer = MFCreateMemoryBuffer(bytes.len() as u32).map_err(text)?;
            let mut data = std::ptr::null_mut();
            buffer.Lock(&mut data, None, None).map_err(text)?;
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
            buffer.Unlock().map_err(text)?;
            buffer.SetCurrentLength(bytes.len() as u32).map_err(text)?;
            let sample = MFCreateSample().map_err(text)?;
            sample.AddBuffer(&buffer).map_err(text)?;
            sample.SetSampleTime(at_100ns).map_err(text)?;
            sample.SetSampleDuration(duration).map_err(text)?;
            self.sink.WriteSample(stream, &sample).map_err(text)
        }
    }

    fn sound(&self, start_frame: u64, block: &[i16]) -> Result<()> {
        let Some(stream) = self.audio else {
            return Ok(());
        };
        let bytes =
            unsafe { std::slice::from_raw_parts(block.as_ptr().cast::<u8>(), block.len() * 2) };
        let at = (start_frame as i128 * 10_000_000 / SAMPLE_RATE as i128) as i64;
        let frames = (block.len() / CHANNELS) as i64;
        self.write(stream, bytes, at, frames * 10_000_000 / SAMPLE_RATE as i64)
    }
}

/// Runs a whole recording. `ready` answers once the file is open and sound
/// sources tried, so the person hears of a failure at once.
pub fn record(
    monitor: Rect,
    area: Rect,
    choice: AudioChoice,
    path: &Path,
    stop: &Arc<AtomicBool>,
    ready: Sender<Result<()>>,
) -> Ended {
    let fail = |error: String, seconds: f64, missing: bool| Ended {
        path: None,
        folder: None,
        seconds,
        missing_sound: missing,
        error: Some(error),
    };
    let _apartment = Apartment::mta();
    if let Err(error) = unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }.map_err(text) {
        let _ = ready.send(Err(error.clone()));
        return fail(error, 0.0, false);
    }
    let ended = (|| {
        let capture = match Capture::start(monitor, area) {
            Ok(capture) => capture,
            Err(error) => {
                let _ = ready.send(Err(error.clone()));
                return fail(error, 0.0, false);
            }
        };
        // The sound sources first: a stream that can't open leaves no empty track.
        let halt = Arc::new(AtomicBool::new(false));
        let mut threads = Vec::new();
        let mut queues = Vec::new();
        let mut missing = false;
        for (wanted, loopback) in [(choice.system, true), (choice.microphone, false)] {
            if !wanted {
                continue;
            }
            let queue = Arc::new(Queue::default());
            let (ok_tx, ok_rx) = std::sync::mpsc::channel();
            threads.push(audio::spawn(loopback, queue.clone(), halt.clone(), ok_tx));
            if ok_rx.recv().unwrap_or(false) {
                queues.push(queue);
            } else {
                missing = true;
            }
        }
        let writer = match Writer::open(
            path,
            area.width as u32,
            area.height as u32,
            !queues.is_empty(),
        ) {
            Ok(writer) => writer,
            Err(error) => {
                halt.store(true, Ordering::SeqCst);
                threads.into_iter().for_each(|t| drop(t.join()));
                let _ = ready.send(Err(error.clone()));
                return fail(error, 0.0, missing);
            }
        };
        let began = Instant::now();
        let _ = ready.send(Ok(()));
        let mut nv12 = vec![0u8; writer.frame_bytes];
        let mut mixer = Mixer::default();
        let mut wrote = false;
        let mut last = began - REPEAT_AFTER;
        let mut problem = None;
        while !stop.load(Ordering::SeqCst) {
            let now = Instant::now();
            if now.duration_since(last) >= FRAME_GAP {
                match capture.latest(&mut nv12) {
                    Err(error) => {
                        problem = Some(error);
                        break;
                    }
                    Ok(fresh) => {
                        if fresh || (wrote && now.duration_since(last) >= REPEAT_AFTER) {
                            let at = now.duration_since(began).as_nanos() as i64 / 100;
                            if let Err(error) =
                                writer.write(writer.video, &nv12, at, 10_000_000 / FPS as i64)
                            {
                                problem = Some(error);
                                break;
                            }
                            wrote = true;
                            last = now;
                        }
                    }
                }
            }
            if !wrote && now.duration_since(began) > FIRST_FRAME_WAIT {
                problem = Some("the display sent no picture".to_owned());
                break;
            }
            if writer.audio.is_some()
                && let Some((start, block)) =
                    mixer.next(began.elapsed().as_secs_f64(), &queues, false)
                && let Err(error) = writer.sound(start, &block)
            {
                problem = Some(error);
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let seconds = began.elapsed().as_secs_f64();
        halt.store(true, Ordering::SeqCst);
        threads.into_iter().for_each(|t| drop(t.join()));
        if writer.audio.is_some()
            && let Some((start, block)) = mixer.next(seconds, &queues, true)
            && wrote
        {
            let _ = writer.sound(start, &block);
        }
        drop(capture);
        if !wrote {
            let _ = std::fs::remove_file(path);
            return fail(
                problem.unwrap_or_else(|| "nothing was recorded".into()),
                seconds,
                missing,
            );
        }
        match unsafe { writer.sink.Finalize() }.map_err(text) {
            Ok(()) => Ended {
                path: Some(path.to_string_lossy().into_owned()),
                folder: None,
                seconds,
                missing_sound: missing,
                error: problem,
            },
            Err(error) => fail(error, seconds, missing),
        }
    })();
    unsafe {
        let _ = MFShutdown();
    }
    ended
}
