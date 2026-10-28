// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The frozen picture a selection is made on (spec 05 §4.2, §15.2): DXGI
//! Desktop Duplication of the display, or GDI `BitBlt` from the screen where
//! duplication can't run (another adapter on a hybrid-GPU laptop, a rotated
//! display, the secure desktop, a display that shows nothing new in time).
//! Windows with `WDA_EXCLUDEFROMCAPTURE` are left out by both.

use super::Rect;

/// Straight RGBA, top-down, opaque.
#[derive(Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Frame {
    /// The part of the frame inside `r` (frame pixels), clamped to it.
    pub fn crop(&self, r: Rect) -> Frame {
        let x0 = r.x.clamp(0, self.width as i32) as usize;
        let y0 = r.y.clamp(0, self.height as i32) as usize;
        let x1 = (r.x + r.width).clamp(0, self.width as i32) as usize;
        let y1 = (r.y + r.height).clamp(0, self.height as i32) as usize;
        let (w, h) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
        let row = self.width as usize * 4;
        let mut rgba = Vec::with_capacity(w * h * 4);
        for y in y0..y0 + h {
            rgba.extend_from_slice(&self.rgba[y * row + x0 * 4..y * row + (x0 + w) * 4]);
        }
        Frame {
            width: w as u32,
            height: h as u32,
            rgba,
        }
    }

    pub fn pixel(&self, x: i32, y: i32) -> Option<[u8; 3]> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return None;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        Some([self.rgba[i], self.rgba[i + 1], self.rgba[i + 2]])
    }
}

/// BGRA rows `pitch` bytes apart into tight, opaque RGBA.
#[cfg(any(windows, test))]
fn from_bgra(bgra: &[u8], width: u32, height: u32, pitch: usize) -> Frame {
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height as usize {
        for px in bgra[y * pitch..y * pitch + width as usize * 4]
            .as_chunks::<4>()
            .0
        {
            rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
    }
    Frame {
        width,
        height,
        rgba,
    }
}

/// The display's picture, `monitor` being its rect in desktop pixels.
pub fn capture(monitor: isize, rect: Rect) -> Option<Frame> {
    #[cfg(windows)]
    {
        imp::duplicate(monitor, rect).or_else(|| imp::bitblt(rect))
    }
    #[cfg(not(windows))]
    {
        let _ = monitor;
        // Designing on the Mac: a grey test card the size of the display.
        let (w, h) = (rect.width.max(1) as u32, rect.height.max(1) as u32);
        Some(Frame {
            width: w,
            height: h,
            rgba: [96, 96, 96, 255].repeat((w * h) as usize),
        })
    }
}

#[cfg(windows)]
mod imp {
    use super::{Frame, Rect, from_bgra};
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
    use windows::Win32::Graphics::Direct3D11::{
        D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
        D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
        D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
    };
    use windows::Win32::Graphics::Dxgi::Common::{
        DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_MODE_ROTATION_IDENTITY, DXGI_MODE_ROTATION_UNSPECIFIED,
    };
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, DXGI_OUTDUPL_FRAME_INFO, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput,
        IDXGIOutput1, IDXGIResource,
    };
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleBitmap,
        CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, HGDIOBJ,
        ROP_CODE, ReleaseDC, SRCCOPY, SelectObject,
    };
    use windows::core::Interface;

    /// The adapter and output that drive `monitor`.
    fn output_for(monitor: isize) -> Option<(IDXGIAdapter1, IDXGIOutput)> {
        let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.ok()?;
        for a in 0.. {
            let adapter = unsafe { factory.EnumAdapters1(a) }.ok()?;
            for o in 0.. {
                let Ok(output) = (unsafe { adapter.EnumOutputs(o) }) else {
                    break;
                };
                let desc = unsafe { output.GetDesc() }.ok()?;
                if desc.Monitor.0 as isize == monitor {
                    let upright = desc.Rotation == DXGI_MODE_ROTATION_IDENTITY
                        || desc.Rotation == DXGI_MODE_ROTATION_UNSPECIFIED;
                    return upright.then_some((adapter, output));
                }
            }
        }
        None
    }

    pub fn duplicate(monitor: isize, rect: Rect) -> Option<Frame> {
        let (adapter, output) = output_for(monitor)?;
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        unsafe {
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .ok()?;
        }
        let (device, context) = (device?, context?);
        let duplication =
            unsafe { output.cast::<IDXGIOutput1>().ok()?.DuplicateOutput(&device) }.ok()?;
        // The first frame carries the whole desktop; a pointer-only update
        // (no present time) doesn't, so wait for the next.
        let mut texture = None;
        for _ in 0..3 {
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource: Option<IDXGIResource> = None;
            unsafe { duplication.AcquireNextFrame(250, &mut info, &mut resource) }.ok()?;
            if info.LastPresentTime != 0 {
                texture = resource.and_then(|r| r.cast::<ID3D11Texture2D>().ok());
                break;
            }
            let _ = unsafe { duplication.ReleaseFrame() };
        }
        let texture = texture?;
        let frame = (|| unsafe {
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            texture.GetDesc(&mut desc);
            if desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM
                || desc.Width != rect.width as u32
                || desc.Height != rect.height as u32
            {
                return None;
            }
            desc.Usage = D3D11_USAGE_STAGING;
            desc.BindFlags = 0;
            desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            desc.MiscFlags = 0;
            let mut staging: Option<ID3D11Texture2D> = None;
            device
                .CreateTexture2D(&desc, None, Some(&mut staging))
                .ok()?;
            let staging = staging?;
            context.CopyResource(&staging, &texture);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .ok()?;
            let pitch = mapped.RowPitch as usize;
            let bytes =
                std::slice::from_raw_parts(mapped.pData.cast::<u8>(), pitch * desc.Height as usize);
            let frame = from_bgra(bytes, desc.Width, desc.Height, pitch);
            context.Unmap(&staging, 0);
            Some(frame)
        })();
        let _ = unsafe { duplication.ReleaseFrame() };
        frame
    }

    pub fn bitblt(rect: Rect) -> Option<Frame> {
        let (w, h) = (rect.width, rect.height);
        if w <= 0 || h <= 0 {
            return None;
        }
        unsafe {
            let screen = GetDC(None);
            let memory = CreateCompatibleDC(Some(screen));
            let bitmap = CreateCompatibleBitmap(screen, w, h);
            let previous = SelectObject(memory, HGDIOBJ(bitmap.0));
            let copied = BitBlt(
                memory,
                0,
                0,
                w,
                h,
                Some(screen),
                rect.x,
                rect.y,
                ROP_CODE(SRCCOPY.0 | CAPTUREBLT.0),
            );
            let mut info = BITMAPINFO {
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
            SelectObject(memory, previous);
            let lines = GetDIBits(
                memory,
                bitmap,
                0,
                h as u32,
                Some(bgra.as_mut_ptr().cast()),
                &mut info,
                DIB_RGB_COLORS,
            );
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            let _ = DeleteDC(memory);
            ReleaseDC(None, screen);
            (copied.is_ok() && lines == h)
                .then(|| from_bgra(&bgra, w as u32, h as u32, w as usize * 4))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Frame {
        // 3×2, each pixel's red is its index.
        let rgba = (0..6u8).flat_map(|i| [i, 0, 0, 255]).collect();
        Frame {
            width: 3,
            height: 2,
            rgba,
        }
    }

    #[test]
    fn crops_clamp_to_the_frame() {
        let c = frame().crop(Rect {
            x: 1,
            y: 0,
            width: 5,
            height: 2,
        });
        assert_eq!((c.width, c.height), (2, 2));
        assert_eq!(
            c.rgba.chunks(4).map(|p| p[0]).collect::<Vec<_>>(),
            [1, 2, 4, 5]
        );
        let none = frame().crop(Rect {
            x: 9,
            y: 9,
            width: 2,
            height: 2,
        });
        assert_eq!((none.width, none.height), (0, 0));
    }

    #[test]
    fn pixels_outside_are_none() {
        assert_eq!(frame().pixel(2, 1), Some([5, 0, 0]));
        assert_eq!(frame().pixel(3, 0), None);
        assert_eq!(frame().pixel(-1, 0), None);
    }

    #[test]
    fn bgra_rows_with_padding_become_tight_opaque_rgba() {
        // 1×2 with an 8-byte pitch; GDI leaves alpha at zero.
        let bgra = [1, 2, 3, 0, 9, 9, 9, 9, 4, 5, 6, 0, 9, 9, 9, 9];
        let f = from_bgra(&bgra, 1, 2, 8);
        assert_eq!(f.rgba, [3, 2, 1, 255, 6, 5, 4, 255]);
    }
}
