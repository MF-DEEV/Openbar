// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Where Watch meets Windows (spec 04 §6.4): the watched window's frame,
//! title and owner, the area's picture, the person's number format, and
//! the notification for a watch that ends while the island is hidden.
//!
//! The picture comes from one `Windows.Graphics.Capture` session per watch
//! (a free-threaded frame pool of two buffers), which reads a window even
//! while it is covered or on another desktop. Each tick takes the newest
//! frame; the session only sends one when the window redraws, so the last
//! frame is kept on the GPU and cropped again until a newer one comes. The
//! pointer is left out, and the yellow border too where Windows grants
//! borderless capture (`GraphicsCaptureAccess`, Windows 11); elsewhere the
//! border shows around the watched window. Where the session can't start
//! (no WGC, no Direct3D), the window is drawn with `PrintWindow`
//! (`PW_RENDERFULLCONTENT`) and a display region is read with the capture
//! overlay's DXGI/`BitBlt` snapshot, every tick.

use super::support::Area;
#[cfg(any(windows, test))]
use super::support::PixelRect;
use crate::capture::snapshot::Frame;

/// The watched window as Windows has it now.
#[derive(Clone, Debug)]
pub struct WindowInfo {
    /// Its visible frame (DWM's extended frame bounds), in desktop pixels.
    pub bounds: Area,
    /// Pixels per point on its display (1.5 at 150 %).
    pub scale: f64,
    pub title: Option<String>,
    pub pid: u32,
}

/// One reading's picture of the area.
#[cfg_attr(not(windows), allow(dead_code))]
pub enum Grab {
    Frame(Frame),
    /// The capture started and its first picture hasn't come yet.
    Waiting,
    /// Nothing to read: no picture in time, or the area left the window.
    Missing,
    /// Windows refused to capture it.
    Denied,
}

/// Tight, opaque RGBA of `rect` from BGRA rows `pitch` bytes apart.
#[cfg(any(windows, test))]
fn crop_bgra(bgra: &[u8], pitch: usize, rect: PixelRect) -> Frame {
    let (x, w) = (rect.x as usize, rect.width as usize);
    let mut rgba = Vec::with_capacity(w * rect.height as usize * 4);
    for y in rect.y as usize..(rect.y + rect.height) as usize {
        for px in bgra[y * pitch + x * 4..y * pitch + (x + w) * 4]
            .as_chunks::<4>()
            .0
        {
            rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
    }
    Frame {
        width: rect.width,
        height: rect.height,
        rgba,
    }
}

/// The toast's XML: the outcome, then where it happened.
#[cfg(any(windows, test))]
fn toast_xml(title: &str, body: &str) -> String {
    let escape = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    };
    format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
        escape(title),
        escape(body)
    )
}

pub use imp::*;

#[cfg(windows)]
mod imp {
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};

    use windows::Graphics::Capture::{
        Direct3D11CaptureFramePool, GraphicsCaptureAccess, GraphicsCaptureAccessKind,
        GraphicsCaptureItem, GraphicsCaptureSession,
    };
    use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
    use windows::Graphics::DirectX::DirectXPixelFormat;
    use windows::Graphics::SizeInt32;
    use windows::Security::Authorization::AppCapabilityAccess::AppCapabilityAccessStatus;
    use windows::Win32::Foundation::{E_ACCESSDENIED, E_FAIL, HMODULE, HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Direct3D::{
        D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP,
    };
    use windows::Win32::Graphics::Direct3D11::{
        D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
        D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
        D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
    };
    use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
    use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
    use windows::Win32::Graphics::Dxgi::{IDXGIAdapter, IDXGIDevice};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleBitmap, CreateCompatibleDC,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, HGDIOBJ,
        MONITOR_DEFAULTTONEAREST, MonitorFromRect, MonitorFromWindow, ReleaseDC, SelectObject,
    };
    use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
    use windows::Win32::System::WinRT::Direct3D11::{
        CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
    };
    use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
        IsWindow, PW_RENDERFULLCONTENT,
    };
    use windows::core::{HSTRING, Interface, PCWSTR, Result};

    use super::super::Kind;
    use super::super::support::{Area, PixelRect, pixel_crop};
    use super::{Frame, Grab, WindowInfo, crop_bgra, toast_xml};
    use crate::capture::Rect;

    /// A capture that sends nothing for this long after starting counts the area hidden.
    const FIRST_FRAME: Duration = Duration::from_secs(2);

    fn hwnd(handle: isize) -> HWND {
        HWND(handle as *mut _)
    }

    pub fn window(handle: isize) -> Option<WindowInfo> {
        let hwnd = hwnd(handle);
        unsafe {
            if !IsWindow(Some(hwnd)).as_bool() {
                return None;
            }
            let mut r = RECT::default();
            let framed = DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                (&mut r as *mut RECT).cast(),
                size_of::<RECT>() as u32,
            );
            if framed.is_err() {
                GetWindowRect(hwnd, &mut r).ok()?;
            }
            if r.right <= r.left || r.bottom <= r.top {
                return None;
            }
            // The display's own scale: an unaware or system-aware window
            // reports 96 or the system's, while its frame and its picture
            // are in the display's pixels.
            let mut dpi = 0u32;
            let mut unused = 0u32;
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut unused).is_err() {
                dpi = GetDpiForWindow(hwnd);
            }
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let length = GetWindowTextLengthW(hwnd);
            let title = (length > 0).then(|| {
                let mut buffer = vec![0u16; length as usize + 1];
                let n = GetWindowTextW(hwnd, &mut buffer).max(0) as usize;
                String::from_utf16_lossy(&buffer[..n])
            });
            Some(WindowInfo {
                bounds: Area {
                    x: r.left as f64,
                    y: r.top as f64,
                    width: (r.right - r.left) as f64,
                    height: (r.bottom - r.top) as f64,
                },
                scale: if dpi == 0 { 1.0 } else { dpi as f64 / 96.0 },
                title: title.filter(|t| !t.trim().is_empty()),
                pid,
            })
        }
    }

    pub fn minimized(handle: isize) -> bool {
        unsafe { IsIconic(hwnd(handle)).as_bool() }
    }

    /// The decimal mark and whether thousands are grouped with a space, as
    /// the person's region writes numbers.
    pub fn number_marks() -> (String, bool) {
        use windows::Win32::Globalization::{GetLocaleInfoEx, LOCALE_SDECIMAL, LOCALE_STHOUSAND};
        let read = |kind: u32| {
            let mut buffer = [0u16; 8];
            let n = unsafe { GetLocaleInfoEx(PCWSTR::null(), kind, Some(&mut buffer)) };
            (n > 1).then(|| String::from_utf16_lossy(&buffer[..n as usize - 1]))
        };
        let decimal = read(LOCALE_SDECIMAL)
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| ".".into());
        let spaced = read(LOCALE_STHOUSAND)
            .is_some_and(|g| !g.is_empty() && g.chars().all(char::is_whitespace));
        (decimal, spaced)
    }

    /// Posts a toast when the app has package identity (the installer's
    /// sparse package); an unpackaged run has no AUMID to post under.
    pub fn toast(title: &str, body: &str) -> bool {
        use windows::ApplicationModel::Package;
        use windows::Data::Xml::Dom::XmlDocument;
        use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
        let Ok(_apartment) = crate::com::Apartment::mta() else {
            return false;
        };
        if Package::Current().is_err() {
            return false;
        }
        let post = || -> Result<()> {
            let document = XmlDocument::new()?;
            document.LoadXml(&HSTRING::from(toast_xml(title, body)))?;
            let toast = ToastNotification::CreateToastNotification(&document)?;
            ToastNotificationManager::CreateToastNotifier()?.Show(&toast)
        };
        post().is_ok()
    }

    /// `Vorssaint.exe --watch-probe <file> <window title part>`: runs the capture
    /// steps for that window one by one and writes how long each took, line by
    /// line, so a step that never returns shows as the last line.
    pub fn probe(path: &str, title: &str) {
        use std::io::Write;
        let Ok(mut out) = std::fs::File::create(path) else {
            return;
        };
        let began = Instant::now();
        let mut line = |text: String| {
            let _ = writeln!(out, "{:>6} ms  {text}", began.elapsed().as_millis());
            let _ = out.flush();
        };
        let _apartment = crate::com::Apartment::mta();
        let found = unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{
                EnumWindows, GetWindowTextW, IsWindowVisible,
            };
            unsafe extern "system" fn each(h: HWND, data: LPARAM) -> windows::core::BOOL {
                let (title, found) = unsafe { &mut *(data.0 as *mut (String, isize)) };
                let mut buf = [0u16; 512];
                let n = unsafe { GetWindowTextW(h, &mut buf) } as usize;
                if unsafe { IsWindowVisible(h) }.as_bool()
                    && String::from_utf16_lossy(&buf[..n]).contains(title.as_str())
                {
                    *found = h.0 as isize;
                    return false.into();
                }
                true.into()
            }
            let mut data = (title.to_string(), 0isize);
            let _ = EnumWindows(Some(each), LPARAM(&mut data as *mut _ as isize));
            data.1
        };
        line(format!("window {found:#x}"));
        if found == 0 {
            return;
        }
        line(format!(
            "supported {:?}",
            GraphicsCaptureSession::IsSupported()
        ));
        line(format!("device {:?}", device().map(|_| ())));
        line("borderless access…".into());
        line(format!("borderless {}", borderless()));
        let kind = Kind::Window {
            hwnd: found,
            crop: Area {
                x: 0.0,
                y: 0.0,
                width: 200.0,
                height: 60.0,
            },
        };
        let start = Capture::start(&kind);
        line(format!("start {:?}", start.as_ref().map(|_| ())));
        if let Ok(mut capture) = start {
            for _ in 0..10 {
                std::thread::sleep(Duration::from_millis(300));
                let grab = capture.grab(
                    Area {
                        x: 0.0,
                        y: 0.0,
                        width: 200.0,
                        height: 60.0,
                    },
                    (800.0, 600.0),
                );
                if let Ok(Grab::Frame(f)) = &grab {
                    line("ocr…".into());
                    let text = crate::ocr::recognize(&f.rgba, f.width, f.height).map(|r| r.text());
                    line(format!("ocr {:?}", text.map_err(|e| format!("{e:?}"))));
                }
                line(format!(
                    "grab {}",
                    match grab {
                        Ok(Grab::Frame(f)) => format!("frame {}x{}", f.width, f.height),
                        Ok(Grab::Waiting) => "waiting".into(),
                        Ok(Grab::Missing) => "missing".into(),
                        Ok(Grab::Denied) => "denied".into(),
                        Err(e) => format!("error {e:?}"),
                    }
                ));
            }
        }
        line(format!(
            "print_window {:?}",
            print_window(found).map(|f| (f.width, f.height))
        ));
    }

    /// Asked once: whether captures may leave out the yellow border. The
    /// answer may wait on a consent Windows never shows a background thread,
    /// so after a moment the watch goes on with the border.
    fn borderless() -> bool {
        static ALLOWED: OnceLock<bool> = OnceLock::new();
        *ALLOWED.get_or_init(|| {
            let (tx, rx) = std::sync::mpsc::channel();
            let _ = std::thread::Builder::new()
                .name("watch-borderless".into())
                .spawn(move || {
                    let _apartment = crate::com::Apartment::mta();
                    let allowed = GraphicsCaptureAccess::RequestAccessAsync(
                        GraphicsCaptureAccessKind::Borderless,
                    )
                    .and_then(|op| op.join())
                    .is_ok_and(|status| status == AppCapabilityAccessStatus::Allowed);
                    let _ = tx.send(allowed);
                });
            rx.recv_timeout(Duration::from_millis(1500))
                .unwrap_or(false)
        })
    }

    pub struct Source {
        kind: Kind,
        capture: Option<Capture>,
        /// Windows refused the capture item: say so if the fallback reads nothing either.
        denied: bool,
    }

    impl Source {
        pub fn new(kind: &Kind) -> Self {
            let (capture, denied) = match Capture::start(kind) {
                Ok(capture) => (Some(capture), false),
                Err(e) => (None, e.code() == E_ACCESSDENIED),
            };
            Source {
                kind: kind.clone(),
                capture,
                denied,
            }
        }

        /// The area now: `crop` in the units of `space`, the size of what
        /// it's measured on (the window in points, the display in pixels).
        pub fn grab(&mut self, crop: Area, space: (f64, f64)) -> Grab {
            if let Some(capture) = self.capture.as_mut() {
                match capture.grab(crop, space) {
                    Ok(grab) => return grab,
                    // The device went (a driver update, a GPU reset): read
                    // the rest of this watch the slower way.
                    Err(_) => self.capture = None,
                }
            }
            let whole = match self.kind {
                Kind::Window { hwnd, .. } => print_window(hwnd),
                Kind::Display { monitor, .. } => {
                    crate::capture::snapshot::capture(monitor_handle(monitor), monitor)
                }
            };
            let area = whole.and_then(|whole| {
                let r = pixel_crop(crop, space, (whole.width, whole.height))?;
                Some(whole.crop(Rect {
                    x: r.x as i32,
                    y: r.y as i32,
                    width: r.width as i32,
                    height: r.height as i32,
                }))
            });
            match area {
                Some(frame) => Grab::Frame(frame),
                None if self.denied => Grab::Denied,
                None => Grab::Missing,
            }
        }
    }

    fn monitor_handle(monitor: Rect) -> isize {
        let r = RECT {
            left: monitor.x,
            top: monitor.y,
            right: monitor.x + monitor.width,
            bottom: monitor.y + monitor.height,
        };
        unsafe { MonitorFromRect(&r, MONITOR_DEFAULTTONEAREST) }.0 as isize
    }

    /// One Windows.Graphics.Capture session and the last picture it sent.
    struct Capture {
        device: ID3D11Device,
        context: ID3D11DeviceContext,
        direct3d: IDirect3DDevice,
        _item: GraphicsCaptureItem,
        pool: Direct3D11CaptureFramePool,
        session: GraphicsCaptureSession,
        size: SizeInt32,
        started: Instant,
        /// The newest frame's content, copied off the pool, and its size.
        last: Option<(ID3D11Texture2D, u32, u32)>,
        /// Reused while the area keeps its size.
        staging: Option<(ID3D11Texture2D, u32, u32)>,
    }

    impl Capture {
        fn start(kind: &Kind) -> Result<Capture> {
            if !GraphicsCaptureSession::IsSupported()? {
                return Err(E_FAIL.into());
            }
            let (device, context) = device()?;
            let direct3d: IDirect3DDevice =
                unsafe { CreateDirect3D11DeviceFromDXGIDevice(&device.cast::<IDXGIDevice>()?)? }
                    .cast()?;
            let interop =
                windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
            let item: GraphicsCaptureItem = match kind {
                Kind::Window { hwnd: handle, .. } => unsafe {
                    interop.CreateForWindow(hwnd(*handle))?
                },
                Kind::Display { monitor, .. } => unsafe {
                    interop.CreateForMonitor(windows::Win32::Graphics::Gdi::HMONITOR(
                        monitor_handle(*monitor) as *mut _,
                    ))?
                },
            };
            let size = item.Size()?;
            let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
                &direct3d,
                DirectXPixelFormat::B8G8R8A8UIntNormalized,
                2,
                size,
            )?;
            let session = pool.CreateCaptureSession(&item)?;
            // Older Windows lacks these; the capture works without them.
            let _ = session.SetIsCursorCaptureEnabled(false);
            if borderless() {
                let _ = session.SetIsBorderRequired(false);
            }
            session.StartCapture()?;
            Ok(Capture {
                device,
                context,
                direct3d,
                _item: item,
                pool,
                session,
                size,
                started: Instant::now(),
                last: None,
                staging: None,
            })
        }

        fn grab(&mut self, crop: Area, space: (f64, f64)) -> Result<Grab> {
            let mut newest = None;
            while let Ok(frame) = self.pool.TryGetNextFrame() {
                newest = Some(frame);
            }
            if let Some(frame) = newest {
                let size = frame.ContentSize()?;
                let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
                let texture: ID3D11Texture2D = unsafe { access.GetInterface()? };
                let mut desc = D3D11_TEXTURE2D_DESC::default();
                unsafe { texture.GetDesc(&mut desc) };
                let (w, h) = (size.Width.max(1) as u32, size.Height.max(1) as u32);
                // A window that grew past the pool's buffers arrives cut to
                // their old size: keep the last whole picture until the
                // recreated pool sends one.
                if w <= desc.Width && h <= desc.Height {
                    let last = match self.last.take() {
                        Some(kept) if (kept.1, kept.2) == (w, h) => kept.0,
                        _ => self.texture(w, h, false)?,
                    };
                    let whole = D3D11_BOX {
                        left: 0,
                        top: 0,
                        front: 0,
                        right: w,
                        bottom: h,
                        back: 1,
                    };
                    unsafe {
                        self.context.CopySubresourceRegion(
                            &last,
                            0,
                            0,
                            0,
                            0,
                            &texture,
                            0,
                            Some(&whole),
                        )
                    };
                    self.last = Some((last, w, h));
                }
                let _ = frame.Close();
                // A resized window: the pool follows, the next frame fills it.
                if (size.Width, size.Height) != (self.size.Width, self.size.Height) {
                    self.size = size;
                    self.pool.Recreate(
                        &self.direct3d,
                        DirectXPixelFormat::B8G8R8A8UIntNormalized,
                        2,
                        size,
                    )?;
                }
            }
            let Some((last, w, h)) = self.last.clone() else {
                return Ok(if self.started.elapsed() < FIRST_FRAME {
                    Grab::Waiting
                } else {
                    Grab::Missing
                });
            };
            let Some(r) = pixel_crop(crop, space, (w, h)) else {
                return Ok(Grab::Missing);
            };
            let staging = match self.staging.take() {
                Some(kept) if (kept.1, kept.2) == (r.width, r.height) => kept.0,
                _ => self.texture(r.width, r.height, true)?,
            };
            let region = D3D11_BOX {
                left: r.x,
                top: r.y,
                front: 0,
                right: r.x + r.width,
                bottom: r.y + r.height,
                back: 1,
            };
            let frame = unsafe {
                self.context
                    .CopySubresourceRegion(&staging, 0, 0, 0, 0, &last, 0, Some(&region));
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                self.context
                    .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
                let pitch = mapped.RowPitch as usize;
                let bytes = std::slice::from_raw_parts(
                    mapped.pData.cast::<u8>(),
                    pitch * r.height as usize,
                );
                let frame = crop_bgra(
                    bytes,
                    pitch,
                    PixelRect {
                        x: 0,
                        y: 0,
                        width: r.width,
                        height: r.height,
                    },
                );
                self.context.Unmap(&staging, 0);
                frame
            };
            self.staging = Some((staging, r.width, r.height));
            Ok(Grab::Frame(frame))
        }

        /// A BGRA texture: on the GPU for the kept frame, CPU-readable for the area.
        fn texture(&self, width: u32, height: u32, staging: bool) -> Result<ID3D11Texture2D> {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: if staging {
                    D3D11_USAGE_STAGING
                } else {
                    D3D11_USAGE_DEFAULT
                },
                BindFlags: 0,
                CPUAccessFlags: if staging {
                    D3D11_CPU_ACCESS_READ.0 as u32
                } else {
                    0
                },
                MiscFlags: 0,
            };
            let mut texture = None;
            unsafe {
                self.device
                    .CreateTexture2D(&desc, None, Some(&mut texture))?
            };
            texture.ok_or_else(|| E_FAIL.into())
        }
    }

    impl Drop for Capture {
        /// Closing the session takes the border away at once.
        fn drop(&mut self) {
            let _ = self.session.Close();
            let _ = self.pool.Close();
        }
    }

    fn device() -> Result<(ID3D11Device, ID3D11DeviceContext)> {
        let drivers: [D3D_DRIVER_TYPE; 2] = [D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP];
        for driver in drivers {
            let mut device = None;
            let mut context = None;
            let created = unsafe {
                D3D11CreateDevice(
                    None::<&IDXGIAdapter>,
                    driver,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    None,
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    Some(&mut context),
                )
            };
            if created.is_ok()
                && let (Some(device), Some(context)) = (device, context)
            {
                return Ok((device, context));
            }
        }
        Err(E_FAIL.into())
    }

    /// The window drawn by itself (`PrintWindow`, full content, so Chromium
    /// and DirectX windows draw too), cut to its visible frame: the picture
    /// the crop is measured on.
    fn print_window(handle: isize) -> Option<Frame> {
        let info = window(handle)?;
        let hwnd = hwnd(handle);
        let mut outer = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut outer) }.ok()?;
        let (w, h) = (outer.right - outer.left, outer.bottom - outer.top);
        if w <= 0 || h <= 0 {
            return None;
        }
        let bgra = unsafe {
            let screen = GetDC(None);
            let memory = CreateCompatibleDC(Some(screen));
            let bitmap = CreateCompatibleBitmap(screen, w, h);
            let previous = SelectObject(memory, HGDIOBJ(bitmap.0));
            let printed =
                PrintWindow(hwnd, memory, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)).as_bool();
            SelectObject(memory, previous);
            let mut header = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bgra = vec![0u8; w as usize * h as usize * 4];
            let lines = GetDIBits(
                memory,
                bitmap,
                0,
                h as u32,
                Some(bgra.as_mut_ptr().cast()),
                &mut header,
                DIB_RGB_COLORS,
            );
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            let _ = DeleteDC(memory);
            ReleaseDC(None, screen);
            (printed && lines == h).then_some(bgra)
        }?;
        // The visible frame inside the window's rectangle, which also holds
        // the invisible resize borders.
        let x = (info.bounds.x as i32 - outer.left).clamp(0, w);
        let y = (info.bounds.y as i32 - outer.top).clamp(0, h);
        let frame = PixelRect {
            x: x as u32,
            y: y as u32,
            width: (info.bounds.width as i32).min(w - x).max(0) as u32,
            height: (info.bounds.height as i32).min(h - y).max(0) as u32,
        };
        (frame.width > 0 && frame.height > 0).then(|| crop_bgra(&bgra, w as usize * 4, frame))
    }
}

#[cfg(not(windows))]
mod imp {
    use super::super::Kind;
    use super::super::support::Area;
    use super::{Grab, WindowInfo};

    pub fn window(_handle: isize) -> Option<WindowInfo> {
        None
    }

    pub fn minimized(_handle: isize) -> bool {
        false
    }

    pub fn number_marks() -> (String, bool) {
        (".".into(), false)
    }

    pub fn toast(_title: &str, _body: &str) -> bool {
        false
    }

    /// Designing on the Mac: nothing to read, so the area stays hidden.
    pub struct Source;

    impl Source {
        pub fn new(_kind: &Kind) -> Self {
            Source
        }

        pub fn grab(&mut self, _crop: Area, _space: (f64, f64)) -> Grab {
            Grab::Missing
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgra_crops_into_tight_opaque_rgba() {
        // 3 × 2 BGRA with an 16-byte pitch; each pixel's blue is its index.
        let mut bgra = vec![0u8; 16 * 2];
        for (i, (x, y)) in [(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1)]
            .into_iter()
            .enumerate()
        {
            bgra[y * 16 + x * 4] = i as u8;
            bgra[y * 16 + x * 4 + 2] = 100;
        }
        let frame = crop_bgra(
            &bgra,
            16,
            PixelRect {
                x: 1,
                y: 0,
                width: 2,
                height: 2,
            },
        );
        assert_eq!((frame.width, frame.height), (2, 2));
        assert_eq!(
            frame.rgba.chunks(4).map(|p| p[2]).collect::<Vec<_>>(),
            [1, 2, 4, 5]
        );
        assert!(frame.rgba.chunks(4).all(|p| p[0] == 100 && p[3] == 255));
    }

    #[test]
    fn toast_text_is_escaped() {
        let xml = toast_xml("It shows <Done> & more", "Tom's \"app\"");
        assert!(xml.contains("<text>It shows &lt;Done&gt; &amp; more</text>"));
        assert!(xml.contains("<text>Tom&apos;s &quot;app&quot;</text>"));
    }
}
