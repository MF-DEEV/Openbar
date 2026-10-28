// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The bytes of the clipboard formats the history reads and writes:
//! bitmaps (CF_DIB, re-encoded to PNG and back), CF_HDROP's DROPFILES and
//! UTF-16 text. Pure, so it is tested on any platform; native.rs moves
//! these bytes in and out of the clipboard.

use std::io::Cursor;

use sha2::{Digest, Sha256};

/// Pictures larger than this many pixels are refused rather than decoded.
const MAX_PIXELS: u64 = 40_000_000;

/// A CF_DIB or CF_DIBV5 bitmap (no BMP file header) as PNG, at its own size.
pub fn dib_png(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    use image::ImageDecoder;
    let decoder = image::codecs::bmp::BmpDecoder::new_without_file_header(Cursor::new(bytes))
        .map_err(|_| "Unsupported clipboard image")?;
    let (width, height) = decoder.dimensions();
    if width as u64 * height as u64 > MAX_PIXELS {
        return Err("Clipboard image is too large".into());
    }
    let image =
        image::DynamicImage::from_decoder(decoder).map_err(|_| "Cannot read clipboard image")?;
    let mut png = Cursor::new(Vec::new());
    image
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|_| "Cannot read clipboard image")?;
    Ok(png.into_inner())
}

/// A PNG's width and height, read from its header; None for anything else.
pub fn png_dimensions(png: &[u8]) -> Option<(u32, u32)> {
    let (w, h) = image::ImageReader::with_format(Cursor::new(png), image::ImageFormat::Png)
        .into_dimensions()
        .ok()?;
    (w > 0 && h > 0).then_some((w, h))
}

/// A PNG as a 24-bit bottom-up CF_DIB for apps that don't read "PNG".
/// Transparent pixels are laid over white, as most of those apps would.
pub fn png_to_dib(png: &[u8]) -> Option<Vec<u8>> {
    let (w, h) = png_dimensions(png)?;
    if w as u64 * h as u64 > MAX_PIXELS {
        return None;
    }
    let rgba = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    let stride = (w as usize * 3).div_ceil(4) * 4;
    let mut out = Vec::with_capacity(40 + stride * h as usize);
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&((stride * h as usize) as u32).to_le_bytes());
    out.extend_from_slice(&[0; 16]); // resolution and palette counts
    let over_white =
        |c: u8, a: u8| ((c as u32 * a as u32 + 255 * (255 - a as u32) + 127) / 255) as u8;
    for y in (0..h).rev() {
        let start = out.len();
        for x in 0..w {
            let [r, g, b, a] = rgba.get_pixel(x, y).0;
            out.extend_from_slice(&[over_white(b, a), over_white(g, a), over_white(r, a)]);
        }
        out.resize(start + stride, 0);
    }
    Some(out)
}

/// CF_HDROP's memory: a DROPFILES header (pFiles = 20, no point, wide
/// names), then each path in UTF-16 with its terminator, then one more.
pub fn dropfiles(paths: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&20u32.to_le_bytes()); // pFiles
    out.extend_from_slice(&[0; 8]); // pt
    out.extend_from_slice(&0u32.to_le_bytes()); // fNC
    out.extend_from_slice(&1u32.to_le_bytes()); // fWide
    for path in paths {
        for unit in path.encode_utf16().chain([0]) {
            out.extend_from_slice(&unit.to_le_bytes());
        }
    }
    out.extend_from_slice(&[0, 0]);
    out
}

/// CF_UNICODETEXT's memory: UTF-16 with a terminator.
pub fn utf16z(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// CF_UNICODETEXT's characters up to the first terminator.
pub fn text_from_utf16(units: &[u16]) -> String {
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    String::from_utf16_lossy(&units[..end])
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The PNG inside a `data:image/png;base64,` URL.
pub fn png_from_data_url(url: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    let (header, data) = url.split_once(',')?;
    if !header.eq_ignore_ascii_case("data:image/png;base64") {
        return None;
    }
    let png = base64::engine::general_purpose::STANDARD
        .decode(data)
        .ok()?;
    png_dimensions(&png).map(|_| png)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32, px: [u8; 4]) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(w, h, image::Rgba(px));
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn clipboard_dib_is_encoded_as_png_without_resizing() {
        // A bottom-up 2 x 1, 24-bit DIB with one red and one green pixel.
        let mut dib = vec![0u8; 48];
        dib[0..4].copy_from_slice(&40u32.to_le_bytes());
        dib[4..8].copy_from_slice(&2i32.to_le_bytes());
        dib[8..12].copy_from_slice(&1i32.to_le_bytes());
        dib[12..14].copy_from_slice(&1u16.to_le_bytes());
        dib[14..16].copy_from_slice(&24u16.to_le_bytes());
        dib[40..46].copy_from_slice(&[0, 0, 255, 0, 255, 0]);
        let png = dib_png(dib).unwrap();
        let decoded = image::load_from_memory(&png).unwrap().to_rgb8();
        assert_eq!(decoded.dimensions(), (2, 1));
        assert_eq!(decoded.get_pixel(0, 0).0, [255, 0, 0]);
        assert_eq!(decoded.get_pixel(1, 0).0, [0, 255, 0]);
    }

    #[test]
    fn png_round_trips_through_a_dib_laid_over_white() {
        let source = png(3, 2, [255, 0, 0, 128]);
        assert_eq!(png_dimensions(&source), Some((3, 2)));
        let dib = png_to_dib(&source).unwrap();
        // 40-byte header, rows padded from 9 to 12 bytes.
        assert_eq!(dib.len(), 40 + 12 * 2);
        let back = image::load_from_memory(&dib_png(dib).unwrap())
            .unwrap()
            .to_rgb8();
        assert_eq!(back.dimensions(), (3, 2));
        assert_eq!(back.get_pixel(2, 1).0, [255, 127, 127]);
        assert_eq!(png_dimensions(b"not a png"), None);
    }

    #[test]
    fn dropfiles_lists_wide_paths_after_a_twenty_byte_header() {
        let bytes = dropfiles(&[r"C:\a.txt".into(), "D:\\ż".into()]);
        assert_eq!(&bytes[0..4], &20u32.to_le_bytes());
        assert_eq!(&bytes[16..20], &1u32.to_le_bytes());
        let units: Vec<u16> = bytes[20..]
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let expected: Vec<u16> = "C:\\a.txt\0D:\\ż\0\0".encode_utf16().collect();
        assert_eq!(units, expected);
        assert_eq!(dropfiles(&[]).len(), 22);
    }

    #[test]
    fn utf16_text_and_hashes() {
        let units: Vec<u16> = utf16z("zażółć")
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(units.last(), Some(&0));
        assert_eq!(text_from_utf16(&units), "zażółć");
        assert_eq!(text_from_utf16(&[104, 105]), "hi");
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn data_urls_must_carry_a_real_png() {
        use base64::Engine as _;
        let source = png(1, 1, [0, 0, 0, 255]);
        let url = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&source)
        );
        assert_eq!(png_from_data_url(&url), Some(source));
        assert_eq!(png_from_data_url("data:image/png;base64,aGVsbG8="), None);
        assert_eq!(png_from_data_url("data:text/plain;base64,aGVsbG8="), None);
    }
}
