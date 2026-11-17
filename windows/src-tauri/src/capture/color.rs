// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The colour picker's copy formats (ColorValue.string, ColorValue.swift:181-222):
//! hex, rgb and hsl from 8-bit sRGB, with the bare-hex option that drops the
//! `#` (issue #168: some design tools reject pasted values that carry it).
//! The Mac's SwiftUI format is left out: Windows has no use for it.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ColorFormat {
    #[default]
    Hex,
    Rgb,
    Hsl,
}

impl ColorFormat {
    /// An unknown stored value reads as hex (ColorCopyFormat.sanitized).
    pub fn from_pref(value: Option<&str>) -> Self {
        match value {
            Some("rgb") => Self::Rgb,
            Some("hsl") => Self::Hsl,
            _ => Self::Hex,
        }
    }
}

/// `bare_hex` only affects hex.
pub fn format(r: u8, g: u8, b: u8, format: ColorFormat, bare_hex: bool) -> String {
    match format {
        ColorFormat::Hex => format!("{}{r:02X}{g:02X}{b:02X}", if bare_hex { "" } else { "#" }),
        ColorFormat::Rgb => format!("rgb({r}, {g}, {b})"),
        ColorFormat::Hsl => {
            let (h, s, l) = hsl(r, g, b);
            format!(
                "hsl({}, {}%, {}%)",
                h.round() as i32,
                (s * 100.0).round() as i32,
                (l * 100.0).round() as i32
            )
        }
    }
}

/// Hue in degrees, saturation and lightness 0…1 (ColorValue.hsl).
pub fn hsl(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let lightness = (max + min) / 2.0;
    if delta <= 0.000_001 {
        return (0.0, 0.0, lightness);
    }
    let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs());
    let mut hue = if max == r {
        ((g - b) / delta) % 6.0
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    } * 60.0;
    if hue < 0.0 {
        hue += 360.0;
    }
    (hue, saturation.clamp(0.0, 1.0), lightness)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_is_upper_case_with_or_without_the_hash() {
        assert_eq!(format(255, 136, 0, ColorFormat::Hex, false), "#FF8800");
        assert_eq!(format(255, 136, 0, ColorFormat::Hex, true), "FF8800");
        assert_eq!(format(0, 0, 0, ColorFormat::Hex, false), "#000000");
    }

    #[test]
    fn bare_hex_leaves_the_other_formats_alone() {
        assert_eq!(
            format(10, 20, 30, ColorFormat::Rgb, true),
            "rgb(10, 20, 30)"
        );
        assert_eq!(
            format(255, 0, 0, ColorFormat::Hsl, true),
            "hsl(0, 100%, 50%)"
        );
    }

    #[test]
    fn hsl_matches_the_mac() {
        assert_eq!(
            format(255, 255, 255, ColorFormat::Hsl, false),
            "hsl(0, 0%, 100%)"
        );
        assert_eq!(
            format(128, 128, 128, ColorFormat::Hsl, false),
            "hsl(0, 0%, 50%)"
        );
        assert_eq!(
            format(0, 255, 0, ColorFormat::Hsl, false),
            "hsl(120, 100%, 50%)"
        );
        assert_eq!(
            format(0, 0, 255, ColorFormat::Hsl, false),
            "hsl(240, 100%, 50%)"
        );
        // Magenta-ish: red is the max and green < blue, so the hue wraps past 360.
        assert_eq!(
            format(255, 0, 128, ColorFormat::Hsl, false),
            "hsl(330, 100%, 50%)"
        );
        assert_eq!(
            format(51, 102, 153, ColorFormat::Hsl, false),
            "hsl(210, 50%, 40%)"
        );
    }

    #[test]
    fn unknown_formats_read_as_hex() {
        assert_eq!(ColorFormat::from_pref(Some("swiftui")), ColorFormat::Hex);
        assert_eq!(ColorFormat::from_pref(None), ColorFormat::Hex);
        assert_eq!(ColorFormat::from_pref(Some("hsl")), ColorFormat::Hsl);
    }
}
