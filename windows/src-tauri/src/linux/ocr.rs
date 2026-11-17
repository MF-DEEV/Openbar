// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Text in a picture on Linux, where Windows has its OCR engine: Tesseract,
//! through its command line, offline. The languages are the ones the person's
//! locale names that Tesseract has data for, with English beside them. A
//! small crop is scaled up first, as on Windows, and the lines come back in
//! reading order.

use image::{RgbaImage, imageops};

use crate::ocr::{OcrError, OcrLine, OcrResult, reading_order, scale_for, scaled_size};

/// Tesseract reads pictures up to this size on a side without trouble.
const MAX_DIMENSION: u32 = 8_000;

/// Tesseract's language codes for the languages people's locales name.
fn tesseract_language(code: &str) -> Option<&'static str> {
    Some(match code {
        "en" => "eng",
        "pl" => "pol",
        "de" => "deu",
        "fr" => "fra",
        "es" => "spa",
        "it" => "ita",
        "pt" => "por",
        "ru" => "rus",
        "uk" => "ukr",
        "cs" => "ces",
        "sk" => "slk",
        "sl" => "slv",
        "hr" => "hrv",
        "nl" => "nld",
        "sv" => "swe",
        "da" => "dan",
        "nb" | "nn" | "no" => "nor",
        "fi" => "fin",
        "hu" => "hun",
        "ro" => "ron",
        "bg" => "bul",
        "el" => "ell",
        "tr" => "tur",
        "he" => "heb",
        "ar" => "ara",
        "hi" => "hin",
        "ja" => "jpn",
        "ko" => "kor",
        "zh" => "chi_sim",
        "vi" => "vie",
        "th" => "tha",
        "id" => "ind",
        "ca" => "cat",
        "lt" => "lit",
        "lv" => "lav",
        "et" => "est",
        _ => return None,
    })
}

/// The languages to read with: those of the locale (`pl_PL.UTF-8`, `LANGUAGE`
/// lists such as `pl:en`) that are installed, then English if it is.
fn choose_languages(installed: &[String], locale: &str) -> Option<String> {
    let has = |code: &str| installed.iter().any(|i| i == code);
    let mut chosen: Vec<&str> = Vec::new();
    for part in locale.split(':') {
        let language = part.split(['_', '.', '@']).next().unwrap_or("");
        if let Some(code) = tesseract_language(language)
            && has(code)
            && !chosen.contains(&code)
        {
            chosen.push(code);
        }
    }
    if has("eng") && !chosen.contains(&"eng") {
        chosen.push("eng");
    }
    if chosen.is_empty() {
        // Whatever it has (but the orientation data, which reads no text).
        chosen.extend(installed.iter().map(String::as_str).find(|i| *i != "osd"));
    }
    (!chosen.is_empty()).then(|| chosen.join("+"))
}

/// The languages `tesseract --list-langs` prints, one on each line after its first.
fn installed_languages(listing: &str) -> Vec<String> {
    listing
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|l| !l.is_empty() && *l != "osd")
        .map(String::from)
        .collect()
}

fn locale() -> String {
    ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|v| !v.is_empty()))
        .unwrap_or_default()
}

/// A line being put together from its words.
struct Pending {
    /// Its block, paragraph and line numbers.
    key: (u32, u32, u32),
    words: Vec<String>,
    /// Left, top, right and bottom of the words so far.
    bounds: (f64, f64, f64, f64),
}

/// Lines out of Tesseract's TSV: each word (level 5) belongs to a line by its
/// block, paragraph and line numbers; a line's box is its words' union.
fn lines_of(tsv: &str, scale: f64) -> Vec<OcrLine> {
    let mut lines: Vec<Pending> = Vec::new();
    for row in tsv.lines().skip(1) {
        let columns: Vec<&str> = row.split('\t').collect();
        if columns.len() < 12 || columns[0] != "5" {
            continue;
        }
        let text = columns[11].trim();
        let confidence: f64 = columns[10].parse().unwrap_or(-1.0);
        let number = |i: usize| columns[i].parse::<u32>().unwrap_or(0);
        let float = |i: usize| columns[i].parse::<f64>().unwrap_or(0.0);
        if text.is_empty() || confidence < 0.0 {
            continue;
        }
        let key = (number(2), number(3), number(4));
        let (left, top) = (float(6), float(7));
        let (right, bottom) = (left + float(8), top + float(9));
        match lines.last_mut() {
            Some(line) if line.key == key => {
                line.words.push(text.to_string());
                let b = line.bounds;
                line.bounds = (b.0.min(left), b.1.min(top), b.2.max(right), b.3.max(bottom));
            }
            _ => lines.push(Pending {
                key,
                words: vec![text.to_string()],
                bounds: (left, top, right, bottom),
            }),
        }
    }
    lines
        .into_iter()
        .map(|line| {
            let (left, top, right, bottom) = line.bounds;
            OcrLine {
                text: line.words.join(" "),
                x: left / scale,
                y: top / scale,
                width: (right - left) / scale,
                height: (bottom - top) / scale,
            }
        })
        .collect()
}

fn temporary_picture(png: &[u8]) -> Option<std::path::PathBuf> {
    static COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "openbar-ocr-{}-{}.png",
        std::process::id(),
        COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&path, png).ok()?;
    Some(path)
}

pub fn recognize(rgba: &[u8], width: u32, height: u32) -> Result<OcrResult, OcrError> {
    recognize_with("tesseract", &locale(), rgba, width, height)
}

fn recognize_with(
    program: &str,
    locale: &str,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<OcrResult, OcrError> {
    let listing = crate::linux::command::run(program, &[], &["--list-langs"], None)
        .ok_or(OcrError::NoLanguage)?;
    let languages =
        choose_languages(&installed_languages(&listing), locale).ok_or(OcrError::NoLanguage)?;
    let scale = scale_for(width, height, MAX_DIMENSION);
    let (w, h) = scaled_size(width, height, scale);
    let pixels = rgba[..width as usize * height as usize * 4].to_vec();
    let source =
        RgbaImage::from_raw(width, height, pixels).ok_or(OcrError::Failed("bad picture".into()))?;
    let scaled = if (w, h) == (width, height) {
        source
    } else {
        let filter = if scale > 1.0 {
            imageops::FilterType::CatmullRom
        } else {
            imageops::FilterType::Triangle
        };
        imageops::resize(&source, w, h, filter)
    };
    let png = crate::shell::encode_png(scaled.as_raw(), w, h)
        .ok_or(OcrError::Failed("bad picture".into()))?;
    let path =
        temporary_picture(&png).ok_or(OcrError::Failed("no place for the picture".into()))?;
    let output = crate::linux::command::run(
        program,
        &[],
        &[
            &path.to_string_lossy(),
            "stdout",
            "-l",
            &languages,
            "--psm",
            "6",
            "tsv",
        ],
        None,
    );
    let _ = std::fs::remove_file(&path);
    let tsv = output.ok_or_else(|| OcrError::Failed("Tesseract failed".into()))?;
    Ok(OcrResult {
        lines: reading_order(lines_of(&tsv, scale)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_locales_languages_are_read_with_english_beside_them() {
        let langs = installed(&["eng", "pol", "deu"]);
        assert_eq!(
            choose_languages(&langs, "pl_PL.UTF-8").as_deref(),
            Some("pol+eng")
        );
        assert_eq!(
            choose_languages(&langs, "en_US.UTF-8").as_deref(),
            Some("eng")
        );
        assert_eq!(
            choose_languages(&langs, "de:pl:en").as_deref(),
            Some("deu+pol+eng")
        );
        assert_eq!(choose_languages(&langs, "fr_FR").as_deref(), Some("eng"));
        assert_eq!(choose_languages(&langs, "C.UTF-8").as_deref(), Some("eng"));
        // No English: what there is. Nothing installed: nothing to read with.
        assert_eq!(
            choose_languages(&installed(&["pol"]), "fr_FR").as_deref(),
            Some("pol")
        );
        assert_eq!(choose_languages(&installed(&[]), "pl_PL"), None);
        assert_eq!(choose_languages(&installed(&["osd"]), "pl_PL"), None);
    }

    #[test]
    fn the_language_listing_has_a_header_and_the_orientation_data() {
        let listing =
            "List of available languages in \"/usr/share/tessdata/\" (3):\neng\nosd\npol\n";
        assert_eq!(installed_languages(listing), ["eng", "pol"]);
        assert!(installed_languages("").is_empty());
    }

    const TSV: &str = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n1\t1\t0\t0\t0\t0\t0\t0\t460\t120\t-1\t\n4\t1\t1\t1\t1\t0\t23\t19\t259\t27\t-1\t\n5\t1\t1\t1\t1\t1\t23\t19\t67\t21\t91.0\tHello\n5\t1\t1\t1\t1\t2\t102\t19\t120\t27\t91.8\tOpenbar\n5\t1\t1\t1\t2\t1\t22\t69\t98\t21\t92.4\tSecond\n5\t1\t1\t1\t2\t2\t135\t69\t46\t21\t-1\tnoise\n5\t1\t1\t1\t2\t3\t194\t69\t59\t21\t90.0\t \n";

    #[test]
    fn words_become_lines_with_their_union_box_in_the_pictures_pixels() {
        let lines = lines_of(TSV, 2.0);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "Hello Openbar");
        // From the first word's left to the last word's right, over the tallest, halved.
        assert_eq!((lines[0].x, lines[0].y), (11.5, 9.5));
        assert_eq!(lines[0].width, (102.0 + 120.0 - 23.0) / 2.0);
        assert_eq!(lines[0].height, (46.0 - 19.0) / 2.0);
        // A word with no confidence or no text is no word.
        assert_eq!(lines[1].text, "Second");
        assert!(lines_of("", 1.0).is_empty());
        assert!(lines_of("level\n5\tshort\n", 1.0).is_empty());
    }

    #[test]
    fn a_missing_program_or_no_language_is_no_language() {
        assert_eq!(
            recognize_with("/nonexistent/tesseract", "en_US", &[0u8; 16], 2, 2),
            Err(OcrError::NoLanguage)
        );
    }

    #[test]
    fn tesseract_reads_a_picture_of_two_lines_in_order() {
        let have = crate::linux::command::run("tesseract", &[], &["--list-langs"], None)
            .is_some_and(|l| installed_languages(&l).iter().any(|i| i == "eng"));
        if !have {
            eprintln!("skipped: tesseract with English data is not installed");
            return;
        }
        let bytes = include_bytes!("../capture/fixtures/ocr-two-lines.png");
        let picture = image::load_from_memory(bytes).unwrap().to_rgba8();
        let (w, h) = picture.dimensions();
        let result = recognize_with("tesseract", "en_US.UTF-8", picture.as_raw(), w, h).unwrap();
        assert_eq!(result.text(), "Hello Openbar 123\nSecond line here");
        // The first line sits above the second, and has a box of its own.
        assert!(result.lines[0].y < result.lines[1].y);
        assert!(result.lines[0].width > 100.0 && result.lines[0].height > 5.0);
        // A small crop of one line is scaled up and still read.
        let crop = image::imageops::crop_imm(&picture, 15, 10, 280, 45).to_image();
        let (cw, ch) = crop.dimensions();
        let one = recognize_with("tesseract", "en_US.UTF-8", crop.as_raw(), cw, ch).unwrap();
        assert_eq!(one.text(), "Hello Openbar 123");
    }
}
