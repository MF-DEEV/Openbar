// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! What Watch decides without looking at the screen, ported from
//! `NotchWatchSupport.swift` (spec 04 §6.1): the reading worth showing, the
//! numbers and words a rule looks for, the coarse picture that says whether
//! the area moved, where the area sits in a window's picture, and the
//! tracker that decides when the rule is met.

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use regex::Regex;
use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

/// How often the area is read: often while its page is open, so the
/// preview feels live, and less while it only feeds the closed island.
pub const VISIBLE_INTERVAL: Duration = Duration::from_secs(1);
pub const BACKGROUND_INTERVAL: Duration = Duration::from_secs(2);
/// A still picture is read again this often: a few changed words in a
/// large area can move its coarse picture too little to notice.
pub const REREAD_INTERVAL: Duration = Duration::from_secs(10);
/// Text recognition gets slower with size and gains nothing past this.
pub const MAXIMUM_RECOGNITION_SIDE: u32 = 900;
pub const HEADLINE_LENGTH: usize = 22;
/// The lines a person can pin, top to bottom (WatchView:126).
pub const PICKABLE_LINES: usize = 12;
/// A chosen area must keep this much of a window, in points.
const MINIMUM_WINDOW_CROP: f64 = 8.0;

/// What a watched area has to do before the island speaks up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Condition {
    /// Its text changes, or its picture when it holds no text.
    #[default]
    Changes,
    /// It changed at least once and then held still.
    Settles,
    /// It shows the text the person typed, such as "Done".
    Contains,
    /// Its number reaches the one the person typed, counting up or down.
    Reaches,
}

/// Why a watch ended.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", content = "text", rename_all = "camelCase")]
pub enum Outcome {
    Changed,
    Settled,
    Shows(String),
    Reached(String),
    Closed,
}

// MARK: Text

/// Swift's `.whitespaces`: tab and the space separators (Zs).
fn is_space(c: char) -> bool {
    matches!(
        c,
        '\t' | ' ' | '\u{00A0}' | '\u{1680}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
    )
}

/// Swift's `.newlines`.
fn is_newline(c: char) -> bool {
    matches!(c, '\n' | '\u{0B}' | '\u{0C}' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

fn without_spaces(text: &str) -> String {
    text.chars().filter(|c| !is_space(*c)).collect()
}

fn length(text: &str) -> usize {
    text.graphemes(true).count()
}

fn cut(text: &str, length: usize) -> String {
    text.graphemes(true).take(length).collect()
}

/// For comparing and finding words: no case, accents, width or extra spaces.
pub fn normalized(text: &str) -> String {
    use icu_normalizer::DecomposingNormalizerBorrowed;
    use icu_properties::CodePointMapData;
    use icu_properties::props::GeneralCategory;
    // Case folding, as Foundation's: lower case, with the full foldings
    // lowercasing leaves (ß as ss, a final sigma as σ).
    let lower = text.to_lowercase().replace('ß', "ss").replace('ς', "σ");
    // Compatibility decomposition folds full- and half-width forms and
    // splits accents off their letters; the accents then drop.
    let decomposed = DecomposingNormalizerBorrowed::new_nfkd().normalize(&lower);
    let categories = CodePointMapData::<GeneralCategory>::new();
    let folded: String = decomposed
        .chars()
        .filter(|&c| categories.get(c) != GeneralCategory::NonspacingMark)
        .collect();
    folded
        .split(|c: char| is_space(c) || is_newline(c) || c.is_whitespace())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The area's lines, top to bottom, as a person can pick one of them.
pub fn lines(text: &str) -> Vec<String> {
    text.split(is_newline)
        .map(|l| l.trim_matches(is_space).to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// The part of the text worth showing beside the camera: a percentage
/// first, then a short first line as it is, then a clock or a number
/// with its unit, and otherwise the first line cut short.
pub fn headline(text: &str, groups_with_space: bool) -> String {
    let lines = lines(text);
    let Some(first) = lines.first() else { return String::new() };
    let joined = lines.join(" ");
    if let Some(percent) = first_match(percent_pattern(groups_with_space), text) {
        return without_spaces(&percent);
    }
    if length(first) <= HEADLINE_LENGTH {
        return first.clone();
    }
    if let Some(clock) = first_match(&CLOCK, &joined) {
        return clock;
    }
    if let Some(amount) = first_match(&AMOUNT, &joined) {
        return amount;
    }
    cut(first, HEADLINE_LENGTH - 1) + "…"
}

/// The line the person chose to show, cut to the strip's length, or the
/// automatic reading while that line is missing.
pub fn headline_for_line(text: &str, line: Option<usize>, groups_with_space: bool) -> String {
    let lines = lines(text);
    match line.and_then(|i| lines.get(i)) {
        Some(chosen) if length(chosen) <= HEADLINE_LENGTH => chosen.clone(),
        Some(chosen) => cut(chosen, HEADLINE_LENGTH - 1) + "…",
        None => headline(text, groups_with_space),
    }
}

// MARK: Numbers

// Keep the whole percentage and its sign: matching only the last group
// turns −1 250% into a positive 250%. Thousands spaced apart join only
// where the region writes them that way, as for any number, so "Step 2
// 100%" stays 100%. The % sign must share the number's line.
const NUMBER: &str = r"[-+−]?\d[\d.,]*\d|[-+−]?\d";
const SPACED_NUMBER: &str = r"[-+−]?\d{1,3}(?:[ \x{00A0}\x{202F}]\d{3})+(?:[.,]\d+)?";

static PERCENT: LazyLock<Regex> = LazyLock::new(|| percent_regex(false));
static SPACED_PERCENT: LazyLock<Regex> = LazyLock::new(|| percent_regex(true));
static CLOCK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b\d{1,2}:\d{2}(?::\d{2})?\b").unwrap());
static AMOUNT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[-+]?\d[\d.,]*(?:\s?[A-Za-z]{1,3}\b)?").unwrap());
static PLAIN_NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(NUMBER).unwrap());
static ANY_NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!("{SPACED_NUMBER}|{NUMBER}")).unwrap());

fn percent_regex(groups_with_space: bool) -> Regex {
    let spaced = if groups_with_space { format!("{SPACED_NUMBER}|") } else { String::new() };
    Regex::new(&format!(r"(?:{spaced}{NUMBER})[ \x{{00A0}}\x{{202F}}]?%")).unwrap()
}

fn percent_pattern(groups_with_space: bool) -> &'static Regex {
    if groups_with_space { &SPACED_PERCENT } else { &PERCENT }
}

fn first_match(pattern: &Regex, text: &str) -> Option<String> {
    pattern.find(text).map(|m| m.as_str().trim_matches(is_space).to_string())
}

/// The number a "reaches" rule compares: a percentage first, otherwise
/// the first number in the text. Both decimal marks are understood;
/// `decimal` settles a lone mark before three digits, which groups
/// thousands in one region and starts decimals in another.
/// `groups_with_space` also reads 10 000 as one number.
pub fn number(text: &str, decimal: &str, groups_with_space: bool) -> Option<f64> {
    if let Some(percent) = first_match(percent_pattern(groups_with_space), text) {
        return parse(percent.strip_suffix('%').unwrap_or(&percent), decimal);
    }
    let pattern: &Regex = if groups_with_space { &ANY_NUMBER } else { &PLAIN_NUMBER };
    first_match(pattern, text).and_then(|raw| parse(&raw, decimal))
}

pub fn parse(raw: &str, decimal: &str) -> Option<f64> {
    let digits = without_spaces(raw);
    let negative = digits.starts_with('-') || digits.starts_with('−');
    let mut digits = digits.trim_matches(['+', '-', '−']).to_string();
    match (digits.rfind('.'), digits.rfind(',')) {
        // The later mark separates the decimals; the other groups thousands.
        (Some(dot), Some(comma)) if dot > comma => digits = digits.replace(',', ""),
        (Some(_), Some(_)) => digits = digits.replace('.', "").replace(',', "."),
        (None, Some(_)) => {
            digits = if groups_thousands(&digits, ',', decimal) {
                digits.replace(',', "")
            } else {
                digits.replace(',', ".")
            }
        }
        (Some(_), None) => {
            if groups_thousands(&digits, '.', decimal) {
                digits = digits.replace('.', "");
            }
        }
        (None, None) => {}
    }
    // Only digits and one mark are left; Rust would also take "inf".
    if !digits.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    let value: f64 = digits.parse().ok().filter(|v: &f64| v.is_finite())?;
    Some(if negative { -value } else { value })
}

/// Whether the only mark in a number groups thousands: it appears more
/// than once, as in 1.234.567, or once before exactly three digits, as in
/// 1,234, unless the region writes decimals with it or the number starts
/// at zero, as 0,125 does.
fn groups_thousands(digits: &str, mark: char, decimal: &str) -> bool {
    let groups: Vec<&str> = digits.split(mark).collect();
    if groups.len() > 2 {
        return true;
    }
    if groups.len() != 2 || groups[1].chars().count() != 3 || groups[0] == "0" {
        return false;
    }
    decimal != mark.to_string()
}

/// A typed target, read as the person's region writes numbers.
pub fn typed_number(text: &str, decimal: &str) -> Option<f64> {
    number(&without_spaces(text), decimal, false)
}

// MARK: Pictures

/// The area as a 16 × 16 grid of coarse grey levels: small enough that
/// antialiasing and compression noise read the same, sharp enough that
/// a moving bar, a new line or a changed colour do not. `rgba` is tight,
/// top-down RGBA.
pub fn fingerprint(rgba: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    const SIDE: usize = 16;
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return None;
    }
    let mut cells = Vec::with_capacity(SIDE * SIDE);
    for cy in 0..SIDE {
        let (y0, y1) = (cy * h / SIDE, ((cy + 1) * h).div_ceil(SIDE).max(cy * h / SIDE + 1));
        for cx in 0..SIDE {
            let (x0, x1) = (cx * w / SIDE, ((cx + 1) * w).div_ceil(SIDE).max(cx * w / SIDE + 1));
            let mut sum = 0u64;
            for y in y0..y1 {
                for px in rgba[(y * w + x0) * 4..(y * w + x1) * 4].as_chunks::<4>().0 {
                    sum += (299 * px[0] as u64 + 587 * px[1] as u64 + 114 * px[2] as u64) / 1000;
                }
            }
            let grey = sum / ((y1 - y0) * (x1 - x0)) as u64;
            cells.push((grey.min(255) as u8) / 16);
        }
    }
    Some(cells)
}

/// Whether two fingerprints show the same picture, allowing a few cells
/// to cross one grey level, as a blinking caret or a shadow does.
pub fn same_fingerprint(lhs: Option<&[u8]>, rhs: Option<&[u8]>) -> bool {
    let (Some(lhs), Some(rhs)) = (lhs, rhs) else { return false };
    if lhs.len() != rhs.len() {
        return false;
    }
    let moved = lhs.iter().zip(rhs).filter(|(a, b)| a.abs_diff(**b) > 1).count();
    let nudged = lhs.iter().zip(rhs).filter(|(a, b)| a != b).count();
    moved == 0 && nudged <= 3
}

const IMAGE_PREFIX: &str = "image:";

/// What two readings are compared by: the normalized text, or the coarse
/// picture when the area holds no text.
pub fn signature(text: &str, fingerprint: Option<&[u8]>) -> Option<String> {
    let normalized = normalized(text);
    if !normalized.is_empty() {
        return Some(normalized);
    }
    fingerprint.map(|cells| {
        IMAGE_PREFIX.to_string() + &cells.iter().map(u8::to_string).collect::<Vec<_>>().join(".")
    })
}

/// Whether two readings say the same: the same text, or pictures that
/// differ no more than a blinking caret does.
pub fn same_signature(lhs: Option<&str>, rhs: Option<&str>) -> bool {
    let (Some(lhs), Some(rhs)) = (lhs, rhs) else { return false };
    if lhs == rhs {
        return true;
    }
    let cells = |s: &str| -> Option<Vec<u8>> {
        s.strip_prefix(IMAGE_PREFIX).map(|rest| rest.split('.').filter_map(|c| c.parse().ok()).collect())
    };
    match (cells(lhs), cells(rhs)) {
        (Some(a), Some(b)) => same_fingerprint(Some(&a), Some(&b)),
        _ => false,
    }
}

// MARK: Geometry

/// A rectangle in points (DIPs) or pixels, as the caller says.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Area {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Area {
    fn intersection(&self, other: &Area) -> Option<Area> {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = (self.x + self.width).min(other.x + other.width);
        let y1 = (self.y + self.height).min(other.y + other.height);
        (x1 > x0 && y1 > y0).then_some(Area { x: x0, y: y0, width: x1 - x0, height: y1 - y0 })
    }

    /// Pixels at `scale` (1.5 at 150 %) as points.
    pub fn in_points(&self, scale: f64) -> Area {
        Area { x: self.x / scale, y: self.y / scale, width: self.width / scale, height: self.height / scale }
    }
}

/// The area in pixels of a `width` × `height` picture, whole pixels.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// The part of a window's picture `crop` covers, `crop` being in points
/// from the window's top-left corner and `window` the window's size in
/// points. None when the area left the window.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn pixel_crop(crop: Area, window: (f64, f64), image: (u32, u32)) -> Option<PixelRect> {
    if window.0 <= 0.0 || window.1 <= 0.0 {
        return None;
    }
    let scale_x = image.0 as f64 / window.0;
    let scale_y = image.1 as f64 / window.1;
    // CGRect.integral: the smallest whole-pixel rect around it.
    let x0 = (crop.x * scale_x).floor();
    let y0 = (crop.y * scale_y).floor();
    let x1 = ((crop.x + crop.width) * scale_x).ceil();
    let y1 = ((crop.y + crop.height) * scale_y).ceil();
    let r = Area { x: x0, y: y0, width: x1 - x0, height: y1 - y0 }
        .intersection(&Area { x: 0.0, y: 0.0, width: image.0 as f64, height: image.1 as f64 })?;
    if r.width < 4.0 || r.height < 4.0 {
        return None;
    }
    Some(PixelRect { x: r.x as u32, y: r.y as u32, width: r.width as u32, height: r.height as u32 })
}

/// The area from the window's top-left corner, kept inside it, when the
/// window lies under the area's middle; both in the same units.
pub fn window_crop(area: Area, window: Area) -> Option<Area> {
    let (mx, my) = (area.x + area.width / 2.0, area.y + area.height / 2.0);
    let inside_x = mx >= window.x && mx < window.x + window.width;
    let inside_y = my >= window.y && my < window.y + window.height;
    if !inside_x || !inside_y {
        return None;
    }
    let inside = area.intersection(&window)?;
    if inside.width < MINIMUM_WINDOW_CROP || inside.height < MINIMUM_WINDOW_CROP {
        return None;
    }
    Some(Area { x: inside.x - window.x, y: inside.y - window.y, ..inside })
}

/// How far a large picture shrinks before recognition; 1 for small ones.
pub fn recognition_scale(width: u32, height: u32) -> f64 {
    let longest = width.max(height);
    if longest <= MAXIMUM_RECOGNITION_SIDE {
        1.0
    } else {
        MAXIMUM_RECOGNITION_SIDE as f64 / longest as f64
    }
}

// MARK: Tracker

/// Follows one area reading after reading and decides when its condition
/// is met. An area whose pixels did not move is not read again: it comes
/// back with its last signature, so a change that holds still is confirmed
/// and a quiet area can settle. `observe(None, …)` only lets time pass.
#[derive(Clone, Debug)]
pub struct Tracker {
    condition: Condition,
    text: String,
    target: Option<f64>,
    /// How the watched app writes numbers: as the person's region does.
    decimal: String,
    groups_with_space: bool,
    baseline: Option<String>,
    changed_since: Option<Instant>,
    current: Option<String>,
    last_change: Option<Instant>,
    saw_change: bool,
    start: Option<f64>,
}

impl Tracker {
    pub const SETTLE_INTERVAL: Duration = Duration::from_secs(30);
    /// How long a reading must keep differing before it counts as a change.
    pub const CHANGE_CONFIRMATION: Duration = Duration::from_secs(3);

    pub fn new(condition: Condition, text: &str, target: Option<f64>, decimal: &str, groups_with_space: bool) -> Self {
        Tracker {
            condition,
            text: text.to_string(),
            target,
            decimal: decimal.to_string(),
            groups_with_space,
            baseline: None,
            changed_since: None,
            current: None,
            last_change: None,
            saw_change: false,
            start: None,
        }
    }

    /// Whether the rule can be met at all: a text to find or a number to
    /// reach must have been typed.
    /// The condition this tracker was made for.
    pub fn condition(&self) -> Condition {
        self.condition
    }

    pub fn is_ready(&self) -> bool {
        match self.condition {
            Condition::Changes | Condition::Settles => true,
            Condition::Contains => !normalized(&self.text).is_empty(),
            Condition::Reaches => self.target.is_some(),
        }
    }

    /// `signature` compares two readings; `reading` is the text as read,
    /// for finding words and numbers.
    pub fn observe(&mut self, signature: Option<&str>, reading: &str, now: Instant) -> Option<Outcome> {
        if !self.is_ready() {
            return None;
        }
        match self.condition {
            Condition::Changes => {
                let signature = signature?;
                let Some(baseline) = self.baseline.as_deref() else {
                    self.baseline = Some(signature.to_string());
                    return None;
                };
                if same_signature(Some(signature), Some(baseline)) {
                    self.changed_since = None;
                    return None;
                }
                // A change counts once it outlasts a few seconds of readings,
                // so a frame caught halfway through redrawing or a blinking
                // caret does not end the watch, while a reading that keeps
                // moving does.
                let Some(since) = self.changed_since else {
                    self.changed_since = Some(now);
                    return None;
                };
                (now.duration_since(since) >= Self::CHANGE_CONFIRMATION).then_some(Outcome::Changed)
            }
            Condition::Settles => {
                if let Some(signature) = signature
                    && !same_signature(Some(signature), self.current.as_deref())
                {
                    if self.current.is_some() {
                        self.saw_change = true;
                    }
                    self.current = Some(signature.to_string());
                    self.last_change = Some(now);
                    return None;
                }
                let last = self.last_change?;
                (self.saw_change && now.duration_since(last) >= Self::SETTLE_INTERVAL).then_some(Outcome::Settled)
            }
            Condition::Contains => {
                normalized(reading).contains(&normalized(&self.text)).then(|| Outcome::Shows(self.text.clone()))
            }
            Condition::Reaches => {
                let target = self.target?;
                let value = number(reading, &self.decimal, self.groups_with_space)?;
                let shown = || Outcome::Reached(headline(reading, self.groups_with_space));
                let Some(start) = self.start else {
                    self.start = Some(value);
                    return (value == target).then(shown);
                };
                if (start < target && value >= target) || (start > target && value <= target) {
                    return Some(shown());
                }
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headline_prefers_a_percentage_then_a_short_line() {
        assert_eq!(headline("Copying files\n42 % done", false), "42%");
        assert_eq!(headline("−1 250 %", true), "−1250%");
        // Spaced thousands join only where the region writes them so.
        assert_eq!(headline("Step 2 100%", false), "100%");
        assert_eq!(headline("Build succeeded\nmore", false), "Build succeeded");
        assert_eq!(headline("", false), "");
        assert_eq!(headline("  \n  \n", false), "");
    }

    #[test]
    fn long_lines_give_way_to_a_clock_an_amount_or_a_cut() {
        assert_eq!(headline("Estimated time remaining 12:05:30 left", false), "12:05:30");
        assert_eq!(headline("The download has transferred 512 MB so far", false), "512 MB");
        assert_eq!(headline("A sentence without any digits at all", false), "A sentence without an…");
        assert_eq!(length(&headline("A sentence without any digits at all", false)), HEADLINE_LENGTH);
        // Exactly 22 characters stays whole.
        assert_eq!(headline("abcdefghijklmnopqrstuv", false), "abcdefghijklmnopqrstuv");
    }

    #[test]
    fn a_pinned_line_shows_cut_or_falls_back_when_gone() {
        let text = "Status\nThis line is far too long for the strip\nok";
        assert_eq!(headline_for_line(text, Some(2), false), "ok");
        assert_eq!(headline_for_line(text, Some(1), false), "This line is far too …");
        assert_eq!(headline_for_line(text, Some(9), false), "Status");
        assert_eq!(headline_for_line(text, None, false), "Status");
    }

    #[test]
    fn lines_trim_and_skip_empty_ones() {
        assert_eq!(lines(" a \r\n\n b\u{2028}c "), ["a", "b", "c"]);
    }

    #[test]
    fn numbers_read_both_decimal_marks() {
        assert_eq!(parse("1,234.5", "."), Some(1234.5));
        assert_eq!(parse("1.234,5", ","), Some(1234.5));
        assert_eq!(parse("1.234.567", "."), Some(1_234_567.0));
        assert_eq!(parse("3,5", "."), Some(3.5));
        assert_eq!(parse("3.5", ","), Some(3.5));
    }

    #[test]
    fn a_lone_mark_before_three_digits_follows_the_region() {
        assert_eq!(parse("1,234", "."), Some(1234.0));
        assert_eq!(parse("1,234", ","), Some(1.234));
        assert_eq!(parse("1.000", ","), Some(1000.0));
        assert_eq!(parse("1.000", "."), Some(1.0));
        // Starting at zero it's always decimals.
        assert_eq!(parse("0,125", "."), Some(0.125));
    }

    #[test]
    fn signs_and_spaces() {
        assert_eq!(parse("-12", "."), Some(-12.0));
        assert_eq!(parse("−12,5", ","), Some(-12.5));
        assert_eq!(parse("+7", "."), Some(7.0));
        assert_eq!(parse("10 000", "."), Some(10_000.0));
        assert_eq!(parse("", "."), None);
        assert_eq!(parse("1.2.3,4.5", "."), None);
    }

    #[test]
    fn the_rule_number_is_a_percentage_first() {
        assert_eq!(number("Step 3 of 10 · 45%", ".", false), Some(45.0));
        assert_eq!(number("Step 3 of 10", ".", false), Some(3.0));
        assert_eq!(number("Score 10 000 points", ".", true), Some(10_000.0));
        assert_eq!(number("Score 10 000 points", ".", false), Some(10.0));
        assert_eq!(number("−1 250 %", ".", true), Some(-1250.0));
        assert_eq!(number("no digits", ".", false), None);
        assert_eq!(typed_number(" 1 000 ", ","), Some(1000.0));
        assert_eq!(typed_number("2,5", ","), Some(2.5));
    }

    #[test]
    fn normalizing_folds_case_accents_width_and_spaces() {
        // ł has no accent to drop, as on the Mac.
        assert_eq!(normalized("  Zażółć\tGĘŚLĄ\njaźń "), "zazołc gesla jazn");
        assert_eq!(normalized("ＤＯＮＥ！"), "done!");
        assert_eq!(normalized("Café   au\u{00A0}lait"), "cafe au lait");
        assert_eq!(normalized(" \n "), "");
        assert_eq!(normalized("Straße"), normalized("STRASSE"));
    }

    #[test]
    fn fingerprints_are_sixteen_by_sixteen_grey_levels() {
        let white = vec![255u8; 32 * 8 * 4];
        let fp = fingerprint(&white, 32, 8).unwrap();
        assert_eq!(fp.len(), 256);
        assert!(fp.iter().all(|&c| c == 15));
        // Smaller than the grid: cells share pixels and none is empty.
        assert_eq!(fingerprint(&[0, 0, 0, 255], 1, 1).unwrap(), vec![0; 256]);
        assert_eq!(fingerprint(&[], 0, 0), None);
    }

    #[test]
    fn a_few_nudged_cells_are_the_same_picture() {
        let a = vec![5u8; 256];
        let mut b = a.clone();
        b[0] = 6;
        b[1] = 4;
        b[2] = 6;
        assert!(same_fingerprint(Some(&a), Some(&b)));
        b[3] = 6;
        assert!(!same_fingerprint(Some(&a), Some(&b)), "four nudged cells");
        let mut c = a.clone();
        c[0] = 7;
        assert!(!same_fingerprint(Some(&a), Some(&c)), "one cell moved two levels");
        assert!(!same_fingerprint(None, Some(&a)));
        assert!(!same_fingerprint(Some(&a), Some(&a[..10])));
    }

    #[test]
    fn signatures_compare_text_or_pictures() {
        let fp = vec![3u8; 256];
        assert_eq!(signature(" Done ", Some(&fp)).as_deref(), Some("done"));
        let image = signature("", Some(&fp)).unwrap();
        assert!(image.starts_with("image:3.3."));
        let mut nudged = fp.clone();
        nudged[7] = 4;
        assert!(same_signature(Some(&image), signature("", Some(&nudged)).as_deref()));
        assert!(!same_signature(Some(&image), Some("done")));
        assert!(!same_signature(None, Some("done")));
        assert_eq!(signature("", None), None);
    }

    #[test]
    fn crops_scale_from_points_to_the_picture() {
        // A 200 × 100 pt window captured at 150 %: 300 × 150 px.
        let crop = Area { x: 10.0, y: 20.0, width: 50.0, height: 30.0 };
        assert_eq!(
            pixel_crop(crop, (200.0, 100.0), (300, 150)),
            Some(PixelRect { x: 15, y: 30, width: 75, height: 45 })
        );
        // Fractions round outward, then clamp to the picture.
        let edge = Area { x: 190.3, y: 90.0, width: 20.0, height: 20.0 };
        assert_eq!(pixel_crop(edge, (200.0, 100.0), (200, 100)), Some(PixelRect { x: 190, y: 90, width: 10, height: 10 }));
        // Left the window, or too thin to read.
        assert_eq!(pixel_crop(Area { x: 300.0, y: 0.0, width: 5.0, height: 5.0 }, (200.0, 100.0), (200, 100)), None);
        assert_eq!(pixel_crop(Area { x: 0.0, y: 0.0, width: 3.0, height: 50.0 }, (200.0, 100.0), (200, 100)), None);
        assert_eq!(pixel_crop(crop, (0.0, 100.0), (200, 100)), None);
    }

    #[test]
    fn a_window_crop_is_window_relative_and_converts_to_points() {
        // Physical pixels: window at (100, 50), 800 × 600; the area spills out on the left.
        let window = Area { x: 100.0, y: 50.0, width: 800.0, height: 600.0 };
        let area = Area { x: 80.0, y: 100.0, width: 200.0, height: 40.0 };
        let crop = window_crop(area, window).unwrap();
        assert_eq!(crop, Area { x: 0.0, y: 50.0, width: 180.0, height: 40.0 });
        // At 125 % those are points of 1.25 pixels, and read back on the
        // window's picture at that scale they are the same pixels again.
        let points = crop.in_points(1.25);
        assert_eq!(points, Area { x: 0.0, y: 40.0, width: 144.0, height: 32.0 });
        assert_eq!(
            pixel_crop(points, (800.0 / 1.25, 600.0 / 1.25), (800, 600)),
            Some(PixelRect { x: 0, y: 50, width: 180, height: 40 })
        );
        // Moved to a 200 % display, the same points cover twice the pixels.
        assert_eq!(
            pixel_crop(points, (640.0, 480.0), (1280, 960)),
            Some(PixelRect { x: 0, y: 80, width: 288, height: 64 })
        );
        // Middle outside the window, or a sliver inside it.
        assert_eq!(window_crop(Area { x: 0.0, y: 0.0, width: 50.0, height: 50.0 }, window), None);
        assert_eq!(window_crop(Area { x: 96.0, y: 60.0, width: 10.0, height: 40.0 }, window), None);
    }

    #[test]
    fn large_pictures_shrink_for_recognition() {
        assert_eq!(recognition_scale(900, 300), 1.0);
        assert_eq!(recognition_scale(1800, 400), 0.5);
        assert_eq!(recognition_scale(300, 3600), 0.25);
    }

    fn at(t0: Instant, seconds: f64) -> Instant {
        t0 + Duration::from_secs_f64(seconds)
    }

    #[test]
    fn changes_needs_three_seconds_of_difference() {
        let t0 = Instant::now();
        let mut tracker = Tracker::new(Condition::Changes, "", None, ".", false);
        assert_eq!(tracker.observe(Some("a"), "a", t0), None);
        assert_eq!(tracker.observe(Some("b"), "b", at(t0, 1.0)), None);
        // Back to the baseline: the pending change is forgotten.
        assert_eq!(tracker.observe(Some("a"), "a", at(t0, 2.0)), None);
        assert_eq!(tracker.observe(Some("b"), "b", at(t0, 3.0)), None);
        assert_eq!(tracker.observe(Some("c"), "c", at(t0, 5.9)), None);
        assert_eq!(tracker.observe(Some("c"), "c", at(t0, 6.0)), Some(Outcome::Changed));
        // No signature only lets time pass.
        let mut quiet = Tracker::new(Condition::Changes, "", None, ".", false);
        assert_eq!(quiet.observe(None, "", t0), None);
    }

    #[test]
    fn settles_after_a_change_and_thirty_quiet_seconds() {
        let t0 = Instant::now();
        let mut tracker = Tracker::new(Condition::Settles, "", None, ".", false);
        assert_eq!(tracker.observe(Some("a"), "a", t0), None);
        // Still without ever changing: never settles.
        assert_eq!(tracker.observe(Some("a"), "a", at(t0, 40.0)), None);
        assert_eq!(tracker.observe(Some("b"), "b", at(t0, 41.0)), None);
        assert_eq!(tracker.observe(Some("b"), "b", at(t0, 70.9)), None);
        assert_eq!(tracker.observe(None, "b", at(t0, 71.0)), Some(Outcome::Settled));
    }

    #[test]
    fn contains_finds_folded_words() {
        let mut tracker = Tracker::new(Condition::Contains, "  done ", None, ".", false);
        let now = Instant::now();
        assert_eq!(tracker.observe(Some("x"), "Working…", now), None);
        assert_eq!(tracker.observe(Some("x"), "Build DONÉ\nok", now), Some(Outcome::Shows("  done ".into())));
        let mut empty = Tracker::new(Condition::Contains, "  ", None, ".", false);
        assert!(!empty.is_ready());
        assert_eq!(empty.observe(Some("x"), "anything", now), None);
    }

    #[test]
    fn reaches_crosses_the_target_from_where_it_started() {
        let now = Instant::now();
        let mut up = Tracker::new(Condition::Reaches, "", Some(100.0), ".", false);
        assert_eq!(up.observe(Some("x"), "Progress 40%", now), None);
        assert_eq!(up.observe(Some("x"), "Progress 99,5%", now), None);
        assert_eq!(up.observe(Some("x"), "Progress 100%", now), Some(Outcome::Reached("100%".into())));
        let mut down = Tracker::new(Condition::Reaches, "", Some(0.0), ".", false);
        assert_eq!(down.observe(Some("x"), "Left: 12", now), None);
        assert_eq!(down.observe(Some("x"), "Left: 0", now), Some(Outcome::Reached("Left: 0".into())));
        // Equal at the start counts at once.
        let mut equal = Tracker::new(Condition::Reaches, "", Some(5.0), ".", false);
        assert_eq!(equal.observe(Some("x"), "5", now), Some(Outcome::Reached("5".into())));
        // Without a number nothing happens, and no target means not ready.
        assert_eq!(up.observe(Some("x"), "no digits", now), None);
        assert!(!Tracker::new(Condition::Reaches, "", None, ".", false).is_ready());
    }

    #[test]
    fn outcomes_serialize_with_a_kind() {
        assert_eq!(serde_json::to_value(Outcome::Changed).unwrap(), serde_json::json!({ "kind": "changed" }));
        assert_eq!(
            serde_json::to_value(Outcome::Shows("Done".into())).unwrap(),
            serde_json::json!({ "kind": "shows", "text": "Done" })
        );
        assert_eq!(serde_json::from_str::<Condition>("\"reaches\"").unwrap(), Condition::Reaches);
    }
}
