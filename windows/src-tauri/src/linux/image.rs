// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Pictures that other programs hand over (a cover, a notification's
//! avatar) turned into data URLs for the page. Nothing that is not a picture
//! is ever passed on: the type is read from the bytes, not from a name.

use base64::Engine;
use gtk::gio::{self, prelude::*};

/// What the most a picture may be.
pub const LIMIT: usize = 4 * 1024 * 1024;

/// The image type of `bytes`, from what it starts with.
pub fn image_type(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        [b'G', b'I', b'F', b'8', ..] => Some("image/gif"),
        [b'B', b'M', ..] => Some("image/bmp"),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some("image/webp"),
        _ => None,
    }
}

pub fn data_url(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() || bytes.len() > LIMIT {
        return None;
    }
    Some(format!(
        "data:{};base64,{}",
        image_type(bytes)?,
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// A picture from a file, given as a path or a `file://` URL; not a
/// directory, nothing large, nothing that is not a picture.
pub fn from_path_or_uri(location: &str) -> Option<String> {
    use std::io::Read;
    let file = if location.starts_with("file://") {
        gio::File::for_uri(location)
    } else if location.starts_with('/') {
        gio::File::for_path(location)
    } else {
        return None;
    };
    let file = std::fs::File::open(file.path()?).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(LIMIT as u64 + 1).read_to_end(&mut bytes).ok()?;
    data_url(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_pictures_become_data_urls() {
        let png = [0x89, b'P', b'N', b'G', 1, 2, 3];
        assert!(
            data_url(&png)
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        assert_eq!(image_type(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(image_type(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert!(data_url(b"-----BEGIN OPENSSH PRIVATE KEY-----").is_none());
        assert!(data_url(&[]).is_none());
        assert!(data_url(&vec![0x89; LIMIT + 1]).is_none());
    }

    #[test]
    fn a_file_is_read_by_path_or_url_but_not_a_directory_or_a_text_file() {
        let dir = std::env::temp_dir().join(format!("openbar-image-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let picture = dir.join("a b.png");
        std::fs::write(&picture, [0x89, b'P', b'N', b'G', 9]).unwrap();
        let text = dir.join("secret.png");
        std::fs::write(&text, "not a picture").unwrap();
        let uri = |p: &std::path::Path| gio::File::for_path(p).uri().to_string();
        let path = |p: &std::path::Path| p.to_string_lossy().into_owned();
        assert!(
            from_path_or_uri(&uri(&picture))
                .unwrap()
                .starts_with("data:image/png")
        );
        assert!(
            from_path_or_uri(&path(&picture))
                .unwrap()
                .starts_with("data:image/png")
        );
        assert!(from_path_or_uri(&path(&text)).is_none());
        assert!(from_path_or_uri(&path(&dir)).is_none());
        for location in [
            "file:///nonexistent/cover.png",
            "http://example.com/a.png",
            "relative.png",
            "",
            "folder-icon",
        ] {
            assert!(from_path_or_uri(location).is_none(), "{location}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
