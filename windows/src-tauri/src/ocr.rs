// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Text in a picture, offline, with `Windows.Media.Ocr` (spec 05 §4.2, §15.2):
//! the engine for the person's profile languages, fed a BGRA
//! `SoftwareBitmap`. Small crops are scaled up first (the engine misses
//! text much under ~20 px tall) and large ones down to
//! `OcrEngine.MaxImageDimension`. Lines come back in reading order, as the
//! Mac joins Vision's (QuickToolsSupport.joinedRecognizedText).
//!
//! ```ignore
//! let result = ocr::recognize(&rgba, width, height)?; // blocking; call off the UI thread
//! let text = result.text();
//! ```

use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrLine {
    pub text: String,
    /// The line's box in the input picture's pixels.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct OcrResult {
    /// In reading order: top to bottom, left to right within a row.
    pub lines: Vec<OcrLine>,
}

impl OcrResult {
    /// The lines, one per line of text; empty when nothing was found.
    pub fn text(&self) -> String {
        self.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OcrError {
    /// No OCR language is installed for the profile languages
    /// (`TryCreateFromUserProfileLanguages` returned null).
    NoLanguage,
    Failed(String),
}

impl std::fmt::Display for OcrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OcrError::NoLanguage => f.write_str("no OCR language installed"),
            OcrError::Failed(e) => f.write_str(e),
        }
    }
}

/// Recognizes `rgba` (straight RGBA, top-down). Blocking.
pub fn recognize(rgba: &[u8], width: u32, height: u32) -> Result<OcrResult, OcrError> {
    if width == 0 || height == 0 || rgba.len() < width as usize * height as usize * 4 {
        return Err(OcrError::Failed("empty picture".into()));
    }
    imp::recognize(rgba, width, height)
}

/// Shorter sides under these get 3× and 2×: a crop of one line of UI text
/// reads far better scaled up.
#[cfg_attr(not(windows), allow(dead_code))]
const UPSCALE_3X_BELOW: u32 = 200;
#[cfg_attr(not(windows), allow(dead_code))]
const UPSCALE_2X_BELOW: u32 = 600;

/// How much to scale a `width` × `height` picture before recognition:
/// small crops up, anything past the engine's `max_dimension` down to it.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn scale_for(width: u32, height: u32, max_dimension: u32) -> f64 {
    let shorter = width.min(height);
    let longer = width.max(height).max(1) as f64;
    let up: f64 = if shorter < UPSCALE_3X_BELOW {
        3.0
    } else if shorter < UPSCALE_2X_BELOW {
        2.0
    } else {
        1.0
    };
    up.min(max_dimension as f64 / longer)
}

/// The scaled size, at least 1 × 1.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn scaled_size(width: u32, height: u32, scale: f64) -> (u32, u32) {
    let s = |v: u32| ((v as f64 * scale).round() as u32).max(1);
    (s(width), s(height))
}

/// Reading order: rows top to bottom, a row being the lines whose middles
/// sit within half a line of the row's first; left to right inside a row.
/// Empty lines drop out.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn reading_order(mut lines: Vec<OcrLine>) -> Vec<OcrLine> {
    lines.retain(|l| !l.text.trim().is_empty());
    lines.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    let mut out: Vec<OcrLine> = Vec::with_capacity(lines.len());
    let mut row: Vec<OcrLine> = Vec::new();
    for line in lines {
        let joins = row.first().is_some_and(|first| {
            let middle = |l: &OcrLine| l.y + l.height / 2.0;
            (middle(&line) - middle(first)).abs() <= first.height.max(line.height) / 2.0
        });
        if !joins {
            row.sort_by(|a, b| a.x.total_cmp(&b.x));
            out.append(&mut row);
        }
        row.push(line);
    }
    row.sort_by(|a, b| a.x.total_cmp(&b.x));
    out.append(&mut row);
    out
}

#[cfg(windows)]
mod imp {
    use super::{OcrError, OcrLine, OcrResult, reading_order, scale_for, scaled_size};
    use image::{RgbaImage, imageops};
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::Streams::DataWriter;

    pub fn recognize(rgba: &[u8], width: u32, height: u32) -> Result<OcrResult, OcrError> {
        let _apartment = crate::com::Apartment::mta().map_err(OcrError::Failed)?;
        let failed = |e: windows::core::Error| OcrError::Failed(e.to_string());
        // Null when no installed OCR language matches the profile's.
        let engine = OcrEngine::TryCreateFromUserProfileLanguages().map_err(|_| OcrError::NoLanguage)?;
        let max = OcrEngine::MaxImageDimension().unwrap_or(10_000);
        let scale = scale_for(width, height, max);
        let (w, h) = scaled_size(width, height, scale);
        let pixels = rgba[..width as usize * height as usize * 4].to_vec();
        let source = RgbaImage::from_raw(width, height, pixels).ok_or(OcrError::Failed("bad picture".into()))?;
        let scaled = if (w, h) == (width, height) {
            source
        } else {
            let filter = if scale > 1.0 { imageops::FilterType::CatmullRom } else { imageops::FilterType::Triangle };
            imageops::resize(&source, w, h, filter)
        };
        let mut bgra = scaled.into_raw();
        for px in bgra.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        let writer = DataWriter::new().map_err(failed)?;
        writer.WriteBytes(&bgra).map_err(failed)?;
        let buffer = writer.DetachBuffer().map_err(failed)?;
        let bitmap = SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Bgra8, w as i32, h as i32)
            .map_err(failed)?;
        let result = engine.RecognizeAsync(&bitmap).and_then(|op| op.join()).map_err(failed)?;
        let mut lines = Vec::new();
        for line in result.Lines().map_err(failed)? {
            let text = line.Text().map_err(failed)?.to_string();
            // A line's box is its words' union, back in the input's pixels.
            let (mut left, mut top, mut right, mut bottom) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for word in line.Words().map_err(failed)? {
                let r = word.BoundingRect().map_err(failed)?;
                left = left.min(r.X as f64);
                top = top.min(r.Y as f64);
                right = right.max((r.X + r.Width) as f64);
                bottom = bottom.max((r.Y + r.Height) as f64);
            }
            if left > right {
                (left, top, right, bottom) = (0.0, 0.0, 0.0, 0.0);
            }
            lines.push(OcrLine {
                text,
                x: left / scale,
                y: top / scale,
                width: (right - left) / scale,
                height: (bottom - top) / scale,
            });
        }
        Ok(OcrResult { lines: reading_order(lines) })
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{OcrError, OcrResult};

    pub fn recognize(_rgba: &[u8], _width: u32, _height: u32) -> Result<OcrResult, OcrError> {
        Err(OcrError::NoLanguage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, x: f64, y: f64) -> OcrLine {
        OcrLine { text: text.into(), x, y, width: 40.0, height: 12.0 }
    }

    #[test]
    fn small_crops_scale_up_and_large_pictures_down() {
        assert_eq!(scale_for(150, 30, 10_000), 3.0);
        assert_eq!(scale_for(900, 400, 10_000), 2.0);
        assert_eq!(scale_for(1920, 1080, 10_000), 1.0);
        // A long, thin strip: 3× would pass the engine's limit, so it stops there.
        assert_eq!(scale_for(4000, 100, 10_000), 2.5);
        assert!((scale_for(20_000, 5_000, 10_000) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn scaled_sizes_round_and_never_reach_zero() {
        assert_eq!(scaled_size(150, 30, 3.0), (450, 90));
        assert_eq!(scaled_size(3, 1, 0.1), (1, 1));
        assert_eq!(scaled_size(1001, 333, 0.5), (501, 167));
    }

    #[test]
    fn lines_join_in_reading_order() {
        let lines = vec![
            line("second row", 0.0, 40.0),
            line("right", 200.0, 2.0),
            line("left", 0.0, 0.0),
            line("   ", 0.0, 80.0),
        ];
        let result = OcrResult { lines: reading_order(lines) };
        assert_eq!(result.text(), "left\nright\nsecond row");
    }

    #[test]
    fn a_wobbling_baseline_stays_on_its_row() {
        // The right word sits 5 px lower than the left: still one row.
        let lines = vec![line("b", 100.0, 5.0), line("a", 0.0, 0.0), line("c", 0.0, 30.0)];
        let text: Vec<_> = reading_order(lines).into_iter().map(|l| l.text).collect();
        assert_eq!(text, ["a", "b", "c"]);
    }

    #[test]
    fn nothing_found_is_empty_text() {
        assert_eq!(OcrResult::default().text(), "");
    }
}
