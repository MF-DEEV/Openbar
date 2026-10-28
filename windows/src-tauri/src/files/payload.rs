// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! What the Windows clipboard formats in a drop hold, decoded from bytes so
//! the parsing can be tested anywhere: the file list (CF_HDROP's DROPFILES),
//! the names of virtual files (FileGroupDescriptorW, what browsers and mail
//! apps drag), wide strings (text, links), and a bitmap (CF_DIB) as PNG.

#![cfg_attr(not(windows), allow(dead_code))]

/// The paths in a DROPFILES block: an offset to a list of strings, each
/// ended by a nul, the list ended by an empty one.
pub fn dropfiles(bytes: &[u8]) -> Vec<String> {
    let (Some(offset), Some(wide)) = (u32_at(bytes, 0), u32_at(bytes, 16)) else {
        return Vec::new();
    };
    let list = bytes.get(offset as usize..).unwrap_or_default();
    let strings: Vec<String> = if wide != 0 {
        let units: Vec<u16> = list
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        units
            .split(|&u| u == 0)
            .map(String::from_utf16_lossy)
            .collect()
    } else {
        list.split(|&b| b == 0)
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect()
    };
    strings.into_iter().take_while(|s| !s.is_empty()).collect()
}

/// A nul-terminated UTF-16 string, as CF_UNICODETEXT and URL formats carry.
pub fn wide_string(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    String::from_utf16_lossy(&units[..end])
}

/// The UTF-16LE bytes, nul-terminated, for setting a wide string format.
pub fn wide_bytes(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// Size of FILEDESCRIPTORW, packed: flags, clsid, sizel, pointl,
/// attributes, three FILETIMEs, size high and low, then 260 name units.
pub const DESCRIPTOR_SIZE: usize = 4 + 16 + 8 + 8 + 4 + 24 + 8 + 520;
const NAME_OFFSET: usize = DESCRIPTOR_SIZE - 520;
const ATTRIBUTES_OFFSET: usize = 4 + 16 + 8 + 8;
const SIZE_OFFSET: usize = ATTRIBUTES_OFFSET + 4 + 24;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
/// dwFlags: which descriptor fields the source filled in.
const FD_ATTRIBUTES: u32 = 0x04;
const FD_FILESIZE: u32 = 0x40;

/// A virtual file in a FileGroupDescriptorW.
#[derive(Debug, PartialEq)]
pub struct VirtualFile {
    pub index: i32,
    /// A name safe to create.
    pub name: String,
    /// The size, when the source gave one: a block of memory may be longer.
    pub size: Option<u64>,
}

/// The virtual files in a FileGroupDescriptorW. Folders are left out;
/// their contents arrive as their own entries.
pub fn virtual_files(bytes: &[u8]) -> Vec<VirtualFile> {
    let count = u32_at(bytes, 0).unwrap_or(0) as usize;
    (0..count.min(4096))
        .filter_map(|i| {
            let at = 4 + i * DESCRIPTOR_SIZE;
            let descriptor = bytes.get(at..at + DESCRIPTOR_SIZE)?;
            let flags = u32_at(descriptor, 0)?;
            let attributes = u32_at(descriptor, ATTRIBUTES_OFFSET)?;
            if flags & FD_ATTRIBUTES != 0 && attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
                return None;
            }
            let size = (flags & FD_FILESIZE != 0).then(|| {
                let high = u32_at(descriptor, SIZE_OFFSET).unwrap_or(0) as u64;
                let low = u32_at(descriptor, SIZE_OFFSET + 4).unwrap_or(0) as u64;
                high << 32 | low
            });
            let name = safe_name(&wide_string(&descriptor[NAME_OFFSET..]))?;
            Some(VirtualFile {
                index: i as i32,
                name,
                size,
            })
        })
        .collect()
}

/// The last component of a name a drag source supplied, with characters
/// Windows forbids replaced; None when nothing usable is left.
pub fn safe_name(name: &str) -> Option<String> {
    let last = name.rsplit(['\\', '/']).next().unwrap_or(name);
    let cleaned: String = last
        .chars()
        .map(|c| {
            if c < ' ' || "<>:\"|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').to_string();
    (!cleaned.is_empty() && cleaned != "..").then_some(cleaned)
}

/// A CF_DIB (BITMAPINFO followed by the pixels) as PNG bytes.
pub fn dib_to_png(dib: &[u8]) -> Option<Vec<u8>> {
    let header = u32_at(dib, 0)? as usize;
    let bits = u16::from_le_bytes([*dib.get(14)?, *dib.get(15)?]);
    let compression = u32_at(dib, 16)?;
    let used = u32_at(dib, 32).unwrap_or(0) as usize;
    // BI_BITFIELDS with a plain BITMAPINFOHEADER puts three masks after it.
    let masks = if header == 40 && compression == 3 {
        12
    } else {
        0
    };
    let colors = if bits <= 8 && used == 0 {
        1 << bits
    } else {
        used
    };
    let palette = colors * 4;
    let offset = 14 + header + masks + palette;
    let mut file = Vec::with_capacity(14 + dib.len());
    file.extend_from_slice(b"BM");
    file.extend_from_slice(&((14 + dib.len()) as u32).to_le_bytes());
    file.extend_from_slice(&[0; 4]);
    file.extend_from_slice(&(offset as u32).to_le_bytes());
    file.extend_from_slice(dib);
    let mut image = image::load_from_memory_with_format(&file, image::ImageFormat::Bmp)
        .ok()?
        .to_rgba8();
    // Many apps leave the alpha byte of 32-bit DIBs at zero.
    if image.pixels().all(|p| p[3] == 0) {
        image.pixels_mut().for_each(|p| p[3] = 255);
    }
    crate::shell::encode_png(image.as_raw(), image.width(), image.height())
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_wide_dropfiles_list() {
        let mut bytes = vec![0u8; 20];
        bytes[0] = 20;
        bytes[16] = 1;
        bytes.extend(wide_bytes(r"C:\a\one.txt"));
        bytes.extend(wide_bytes(r"D:\b ż\two"));
        bytes.extend([0, 0]);
        assert_eq!(dropfiles(&bytes), [r"C:\a\one.txt", r"D:\b ż\two"]);
        assert!(dropfiles(&[1, 2]).is_empty());
    }

    #[test]
    fn reads_virtual_file_names_and_skips_folders() {
        let mut bytes = 4u32.to_le_bytes().to_vec();
        let entries = [
            ("photo.jpg", FD_FILESIZE, 0u32, 7u32),
            ("folder", FD_ATTRIBUTES, 0x10, 0),
            (r"sub\re:port?.pdf", 0, 0, 0),
            // Attributes the source didn't mark as given don't count.
            ("notes.txt", 0, 0x10, 0),
        ];
        for (name, flags, attributes, size) in entries {
            let mut d = vec![0u8; DESCRIPTOR_SIZE];
            d[0..4].copy_from_slice(&flags.to_le_bytes());
            d[ATTRIBUTES_OFFSET..ATTRIBUTES_OFFSET + 4].copy_from_slice(&attributes.to_le_bytes());
            d[SIZE_OFFSET + 4..SIZE_OFFSET + 8].copy_from_slice(&size.to_le_bytes());
            let wide = wide_bytes(name);
            d[NAME_OFFSET..NAME_OFFSET + wide.len()].copy_from_slice(&wide);
            bytes.extend(d);
        }
        let found = virtual_files(&bytes);
        let names: Vec<_> = found
            .iter()
            .map(|f| (f.index, f.name.as_str(), f.size))
            .collect();
        assert_eq!(
            names,
            [
                (0, "photo.jpg", Some(7)),
                (2, "re_port_.pdf", None),
                (3, "notes.txt", None)
            ]
        );
    }

    #[test]
    fn names_from_a_drag_source_are_made_safe() {
        assert_eq!(safe_name("a/b/c.txt").as_deref(), Some("c.txt"));
        assert_eq!(safe_name("..").as_deref(), None);
        assert_eq!(safe_name(" trailing. ").as_deref(), Some("trailing"));
    }

    #[test]
    fn wide_strings_stop_at_the_nul() {
        assert_eq!(
            wide_string(&wide_bytes("https://example.com")),
            "https://example.com"
        );
        let mut odd = wide_bytes("hi");
        odd.extend([b'x']);
        assert_eq!(wide_string(&odd), "hi");
    }

    #[test]
    fn a_dib_becomes_an_opaque_png() {
        // 1×1, 32-bit, alpha left at zero.
        let mut dib = vec![0u8; 40];
        dib[0] = 40;
        dib[4] = 1;
        dib[8] = 1;
        dib[12] = 1;
        dib[14] = 32;
        dib.extend([10, 20, 30, 0]);
        let png = dib_to_png(&dib).unwrap();
        let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(decoded.get_pixel(0, 0).0, [30, 20, 10, 255]);
    }
}
