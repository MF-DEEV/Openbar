// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Read Discord's saved keybind configuration and optional startup voice state.
//! Saved mute/deafen values are NOT live state and must not override controls.
//! Never opens the database as a writer or decodes other storage values.
//! Formats: github.com/google/leveldb/doc/{log_format,table_format}.md and
//! github.com/google/snappy/blob/main/format_description.txt.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const MAX_FILE: usize = 64 * 1024 * 1024;
const MAX_BLOCK: usize = 64 * 1024 * 1024;
const LOG_BLOCK: usize = 32768;
const MEDIA_KEYS: [&[u8]; 4] = [
    b"_https://discord.com\0\x01MediaEngineStore",
    b"_https://discordapp.com\0\x01MediaEngineStore",
    b"_https://canary.discord.com\0\x01MediaEngineStore",
    b"_https://ptb.discord.com\0\x01MediaEngineStore",
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceState {
    pub muted: bool,
    pub deafened: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Record<T = VoiceState> {
    sequence: u64,
    state: Option<T>,
}

struct Cached<T> {
    fingerprint: (u64, SystemTime),
    record: Option<Record<T>>,
}

pub struct Reader<T = VoiceState> {
    files: HashMap<PathBuf, Cached<T>>,
}
impl<T> Default for Reader<T> {
    fn default() -> Self {
        Self {
            files: HashMap::new(),
        }
    }
}

impl Reader {
    pub fn read(&mut self, directory: &Path) -> Option<VoiceState> {
        self.read_with(directory, &MEDIA_KEYS, decode_voice)
    }
}

impl Reader<Vec<crate::discord_keys::Binding>> {
    pub fn bindings(&mut self, directory: &Path) -> Option<Vec<crate::discord_keys::Binding>> {
        const KEYS: [&[u8]; 4] = [
            b"_https://discord.com\0\x01keybinds",
            b"_https://discordapp.com\0\x01keybinds",
            b"_https://canary.discord.com\0\x01keybinds",
            b"_https://ptb.discord.com\0\x01keybinds",
        ];
        self.read_with(directory, &KEYS, |key, value, kind| {
            KEYS.contains(&key).then(|| {
                if kind == 1 {
                    json_value(value).map(|v| crate::discord_keys::parse(&v))
                } else {
                    None
                }
            })
        })
    }
}

type Decode<T> = fn(&[u8], &[u8], u8) -> Option<Option<T>>;

impl<T: Clone> Reader<T> {
    fn read_with(&mut self, directory: &Path, keys: &[&[u8]], decode: Decode<T>) -> Option<T> {
        let mut latest = None;
        let mut seen = Vec::new();
        for entry in fs::read_dir(directory).ok()? {
            let entry = entry.ok()?;
            let path = entry.path();
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            if !matches!(ext, "log" | "ldb" | "sst") {
                continue;
            }
            // Database files are numeric; ignore the human-readable LOG files.
            if !path
                .file_stem()?
                .to_str()?
                .bytes()
                .all(|b| b.is_ascii_digit())
            {
                continue;
            }
            // Query the open handle: NTFS directory entries can report an old
            // size while Discord keeps the journal open for appending.
            let file = open_shared(&path)?;
            let metadata = file.metadata().ok()?;
            if metadata.len() > MAX_FILE as u64 {
                return None;
            }
            let fingerprint = (metadata.len(), metadata.modified().ok()?);
            if self
                .files
                .get(&path)
                .is_none_or(|v| v.fingerprint != fingerprint)
            {
                let bytes = read_shared(file)?;
                let record = if ext == "log" {
                    read_log_with(&bytes, decode)
                } else {
                    read_table_with(&bytes, keys, decode)?
                };
                self.files.insert(
                    path.clone(),
                    Cached {
                        fingerprint,
                        record,
                    },
                );
            }
            newer(&mut latest, self.files.get(&path)?.record.clone());
            seen.push(path);
        }
        self.files.retain(|path, _| seen.contains(path));
        latest?.state
    }
}

fn open_shared(path: &Path) -> Option<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Cooperate with Discord's open writer and file compaction/rotation.
        options.share_mode(1 | 2 | 4);
    }
    options.open(path).ok()
}

fn read_shared(file: File) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    file.take((MAX_FILE + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= MAX_FILE).then_some(bytes)
}

fn newer<T>(current: &mut Option<Record<T>>, candidate: Option<Record<T>>) {
    if let Some(candidate) = candidate
        && current
            .as_ref()
            .is_none_or(|v| candidate.sequence > v.sequence)
    {
        *current = Some(candidate);
    }
}

fn media_key(key: &[u8]) -> bool {
    MEDIA_KEYS.contains(&key)
}

fn state(value: &[u8]) -> Option<VoiceState> {
    let json = json_value(value)?;
    let settings = json.get("default")?;
    let deafened = settings.get("deaf")?.as_bool()?;
    let muted = settings.get("mute")?.as_bool()? || deafened;
    Some(VoiceState { muted, deafened })
}

fn json_value(value: &[u8]) -> Option<serde_json::Value> {
    let (encoding, value) = value.split_first()?;
    let json = match encoding {
        1 => value.iter().map(|&b| char::from(b)).collect::<String>(),
        0 if value.len() % 2 == 0 => String::from_utf16(
            &value
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>(),
        )
        .ok()?,
        _ => return None,
    };
    serde_json::from_str(&json).ok()
}

fn decode_voice(key: &[u8], value: &[u8], kind: u8) -> Option<Option<VoiceState>> {
    media_key(key).then(|| if kind == 1 { state(value) } else { None })
}

fn consider<T>(
    current: &mut Option<Record<T>>,
    key: &[u8],
    value: &[u8],
    sequence: u64,
    kind: u8,
    decode: Decode<T>,
) {
    if let Some(state) = decode(key, value, kind) {
        newer(current, Some(Record { sequence, state }));
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.position.checked_add(count)?;
        let result = self.bytes.get(self.position..end)?;
        self.position = end;
        Some(result)
    }
    fn byte(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn varint(&mut self) -> Option<usize> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = self.byte()?;
            if shift == 63 && byte > 1 {
                return None;
            }
            value |= u64::from(byte & 127) << shift;
            if byte & 128 == 0 {
                return usize::try_from(value).ok();
            }
        }
        None
    }
    fn string(&mut self) -> Option<&'a [u8]> {
        let len = self.varint()?;
        self.take(len)
    }
}

fn u32le(bytes: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?))
}
fn u64le(bytes: &[u8]) -> Option<u64> {
    Some(u64::from_le_bytes(bytes.get(..8)?.try_into().ok()?))
}

fn checksum(kind: u8, bytes: &[u8], prefix_kind: bool) -> u32 {
    let mut crc = !0u32;
    let kind = [kind];
    let parts = if prefix_kind {
        [kind.as_slice(), bytes]
    } else {
        [bytes, kind.as_slice()]
    };
    for part in parts {
        for byte in part {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0x82f63b78u32.wrapping_mul(crc & 1));
            }
        }
    }
    (!crc).rotate_right(15).wrapping_add(0xa282ead8)
}

fn batch_with<T>(bytes: &[u8], decode: Decode<T>) -> Option<Record<T>> {
    let sequence = u64le(bytes)?;
    let count = u32le(bytes.get(8..)?)? as usize;
    if count > bytes.len() {
        return None;
    }
    let mut cursor = Cursor::new(bytes.get(12..)?);
    let mut latest = None;
    for index in 0..count {
        let kind = cursor.byte()?;
        let key = cursor.string()?;
        let value = match kind {
            0 => &[][..],
            1 => cursor.string()?,
            _ => return None,
        };
        consider(
            &mut latest,
            key,
            value,
            sequence.checked_add(index as u64)?,
            kind,
            decode,
        );
    }
    if cursor.position != cursor.bytes.len() {
        return None;
    }
    latest
}

#[cfg(test)]
fn read_log(bytes: &[u8]) -> Option<Record> {
    read_log_with(bytes, decode_voice)
}

fn read_log_with<T>(bytes: &[u8], decode: Decode<T>) -> Option<Record<T>> {
    let mut latest = None;
    let mut fragment = None::<Vec<u8>>;
    let mut offset = 0usize;
    while offset.checked_add(7).is_some_and(|end| end <= bytes.len()) {
        let remaining = LOG_BLOCK - offset % LOG_BLOCK;
        if remaining < 7 {
            offset += remaining;
            continue;
        }
        let header = &bytes[offset..offset + 7];
        let len = u16::from_le_bytes([header[4], header[5]]) as usize;
        let kind = header[6];
        if kind == 0 && len == 0 {
            offset += remaining;
            fragment = None;
            continue;
        }
        if len + 7 > remaining {
            break;
        }
        offset += 7;
        let Some(payload) = bytes.get(offset..offset + len) else {
            break;
        };
        offset += len;
        if checksum(kind, payload, true) != u32le(header).unwrap_or(0) {
            fragment = None;
            continue;
        }
        match kind {
            1 => {
                fragment = None;
                newer(&mut latest, batch_with(payload, decode));
            }
            2 => {
                fragment = Some(payload.to_vec());
            }
            3 | 4 => {
                if let Some(parts) = &mut fragment {
                    if parts.len() + payload.len() > MAX_BLOCK {
                        fragment = None;
                        continue;
                    }
                    parts.extend_from_slice(payload);
                    if kind == 4 {
                        newer(&mut latest, batch_with(parts, decode));
                        fragment = None;
                    }
                }
            }
            _ => {
                fragment = None;
            }
        }
    }
    latest
}

fn snappy(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut cursor = Cursor::new(bytes);
    let size = cursor.varint()?;
    if size > MAX_BLOCK {
        return None;
    }
    let mut output = Vec::with_capacity(size);
    while cursor.position < bytes.len() {
        let tag = cursor.byte()?;
        if tag & 3 == 0 {
            let code = (tag >> 2) as usize;
            let length = if code < 60 {
                code + 1
            } else {
                let mut n = 0u32;
                for (index, &byte) in cursor.take(code - 59)?.iter().enumerate() {
                    n |= u32::from(byte) << (index * 8);
                }
                usize::try_from(n).ok()?.checked_add(1)?
            };
            if output.len().checked_add(length)? > size {
                return None;
            }
            output.extend_from_slice(cursor.take(length)?);
        } else {
            let (length, distance) = match tag & 3 {
                1 => (
                    4 + ((tag >> 2) & 7) as usize,
                    ((tag as usize & 224) << 3) | cursor.byte()? as usize,
                ),
                2 => (
                    1 + (tag >> 2) as usize,
                    u16::from_le_bytes(cursor.take(2)?.try_into().ok()?) as usize,
                ),
                _ => (
                    1 + (tag >> 2) as usize,
                    usize::try_from(u32le(cursor.take(4)?)?).ok()?,
                ),
            };
            if distance == 0 || distance > output.len() || output.len().checked_add(length)? > size
            {
                return None;
            }
            for _ in 0..length {
                output.push(output[output.len() - distance]);
            }
        }
    }
    (output.len() == size).then_some(output)
}

fn table_block(bytes: &[u8], handle: &mut Cursor<'_>) -> Option<Vec<u8>> {
    let offset = handle.varint()?;
    let length = handle.varint()?;
    if length > MAX_BLOCK {
        return None;
    }
    let end = offset.checked_add(length)?;
    let payload = bytes.get(offset..end)?;
    let kind = *bytes.get(end)?;
    if checksum(kind, payload, false) != u32le(bytes.get(end.checked_add(1)?..)?)? {
        return None;
    }
    match kind {
        0 => Some(payload.to_vec()),
        1 => snappy(payload),
        _ => None,
    }
}

fn entries(bytes: &[u8], mut visit: impl FnMut(&[u8], &[u8]) -> Option<()>) -> Option<()> {
    let restart_count = u32le(bytes.get(bytes.len().checked_sub(4)?..)?)? as usize;
    let end = bytes
        .len()
        .checked_sub(4usize.checked_add(restart_count.checked_mul(4)?)?)?;
    let mut cursor = Cursor::new(bytes.get(..end)?);
    let mut key = Vec::new();
    while cursor.position < end {
        let shared = cursor.varint()?;
        let unique = cursor.varint()?;
        let value_length = cursor.varint()?;
        if shared > key.len() || shared.checked_add(unique)? > MAX_BLOCK {
            return None;
        }
        key.truncate(shared);
        key.extend_from_slice(cursor.take(unique)?);
        visit(&key, cursor.take(value_length)?)?;
    }
    Some(())
}

#[cfg(test)]
fn read_table(bytes: &[u8]) -> Option<Option<Record>> {
    read_table_with(bytes, &MEDIA_KEYS, decode_voice)
}

fn read_table_with<T>(
    bytes: &[u8],
    keys: &[&[u8]],
    decode: Decode<T>,
) -> Option<Option<Record<T>>> {
    if bytes.get(bytes.len().checked_sub(8)?..)? != [0x57, 0xfb, 0x80, 0x8b, 0x24, 0x75, 0x47, 0xdb]
    {
        return None;
    }
    let mut footer = Cursor::new(bytes.get(bytes.len().checked_sub(48)?..)?);
    footer.varint()?;
    footer.varint()?; // metaindex block handle
    let index = table_block(bytes, &mut footer)?;
    let mut latest = None;
    let mut previous_upper = Vec::new();
    entries(&index, |separator, handle| {
        // Sorted index separators let us skip unrelated stores, including very
        // large blocks. Only decompress blocks whose range can contain this key.
        let upper = separator.get(..separator.len().checked_sub(8)?)?;
        let relevant = keys.iter().any(|target| {
            *target <= upper && (previous_upper.is_empty() || *target >= previous_upper.as_slice())
        });
        previous_upper = upper.to_vec();
        if !relevant {
            return Some(());
        }
        let data = table_block(bytes, &mut Cursor::new(handle))?;
        entries(&data, |key, value| {
            let split = key.len().checked_sub(8)?;
            let tag = u64le(&key[split..])?;
            consider(
                &mut latest,
                &key[..split],
                value,
                tag >> 8,
                (tag & 255) as u8,
                decode,
            );
            Some(())
        })
    })?;
    Some(latest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "diagnostic: requires a local Discord profile"]
    fn reads_local_discord_profile() {
        let root = std::env::var_os("APPDATA").unwrap();
        let directory = PathBuf::from(root).join("discord/Local Storage/leveldb");
        let mut reader = Reader::default();
        println!("Local Discord voice state: {:?}", reader.read(&directory));
        assert!(reader.read(&directory).is_some());
        let mut reader = Reader::<Vec<crate::discord_keys::Binding>>::default();
        let bindings = reader.bindings(&directory).expect("read keybind settings");
        println!("Configured mute/deafen bindings: {}", bindings.len());
        assert!(!bindings.is_empty());
    }

    const KEY: &[u8] = b"_https://discord.com\0\x01MediaEngineStore";
    fn varint(mut n: usize, out: &mut Vec<u8>) {
        while n >= 128 {
            out.push((n as u8 & 127) | 128);
            n >>= 7;
        }
        out.push(n as u8);
    }
    fn value(muted: bool, deafened: bool) -> Vec<u8> {
        let mut out = vec![1];
        out.extend_from_slice(format!(r#"{{"default":{{"mute":{muted},"deaf":{deafened}}},"stream":{{"mute":true,"deaf":true}}}}"#).as_bytes());
        out
    }
    fn write_batch(sequence: u64, operations: &[(&[u8], Option<&[u8]>)]) -> Vec<u8> {
        let mut out = sequence.to_le_bytes().to_vec();
        out.extend_from_slice(&(operations.len() as u32).to_le_bytes());
        for (key, value) in operations {
            out.push(u8::from(value.is_some()));
            varint(key.len(), &mut out);
            out.extend_from_slice(key);
            if let Some(value) = value {
                varint(value.len(), &mut out);
                out.extend_from_slice(value);
            }
        }
        out
    }
    fn append_log(out: &mut Vec<u8>, mut batch: &[u8]) {
        let mut first = true;
        while !batch.is_empty() {
            let mut left = LOG_BLOCK - out.len() % LOG_BLOCK;
            if left < 7 {
                out.resize(out.len() + left, 0);
                left = LOG_BLOCK;
            }
            let len = batch.len().min(left - 7);
            let last = len == batch.len();
            let kind = match (first, last) {
                (true, true) => 1,
                (true, false) => 2,
                (false, true) => 4,
                _ => 3,
            };
            let payload = &batch[..len];
            out.extend_from_slice(&checksum(kind, payload, true).to_le_bytes());
            out.extend_from_slice(&(len as u16).to_le_bytes());
            out.push(kind);
            out.extend_from_slice(payload);
            batch = &batch[len..];
            first = false;
        }
    }
    fn log(sequence: u64, muted: bool, deafened: bool) -> Vec<u8> {
        let mut out = Vec::new();
        append_log(
            &mut out,
            &write_batch(sequence, &[(KEY, Some(&value(muted, deafened)))]),
        );
        out
    }
    fn literal(bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        varint(bytes.len(), &mut out);
        if bytes.len() < 61 {
            out.push(((bytes.len() - 1) << 2) as u8);
        } else {
            out.push(61 << 2);
            out.extend_from_slice(&((bytes.len() - 1) as u16).to_le_bytes());
        }
        out.extend_from_slice(bytes);
        out
    }
    fn entry(key: &[u8], value: &[u8]) -> Vec<u8> {
        let mut out = vec![0];
        varint(key.len(), &mut out);
        varint(value.len(), &mut out);
        out.extend_from_slice(key);
        out.extend_from_slice(value);
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out
    }
    fn append_block(out: &mut Vec<u8>, bytes: &[u8], compressed: bool) -> Vec<u8> {
        let mut handle = Vec::new();
        varint(out.len(), &mut handle);
        let bytes = if compressed {
            literal(bytes)
        } else {
            bytes.to_vec()
        };
        varint(bytes.len(), &mut handle);
        out.extend_from_slice(&bytes);
        out.push(u8::from(compressed));
        out.extend_from_slice(&checksum(u8::from(compressed), &bytes, false).to_le_bytes());
        handle
    }
    fn table(sequence: u64, muted: bool, deafened: bool, compressed: bool) -> Vec<u8> {
        let mut out = Vec::new();
        let mut key = KEY.to_vec();
        key.extend_from_slice(&((sequence << 8) | 1).to_le_bytes());
        let data = append_block(&mut out, &entry(&key, &value(muted, deafened)), compressed);
        let index = append_block(&mut out, &entry(&key, &data), false);
        let mut footer = vec![0, 0];
        footer.extend_from_slice(&index);
        footer.resize(40, 0);
        out.extend_from_slice(&footer);
        out.extend_from_slice(&[0x57, 0xfb, 0x80, 0x8b, 0x24, 0x75, 0x47, 0xdb]);
        out
    }

    #[test]
    fn only_exact_media_store_and_default_context_determine_mute() {
        assert!(media_key(KEY));
        assert!(!media_key(b"_https://discord.com\0\x01token"));
        assert!(!media_key(b"_https://example.com\0\x01MediaEngineStore"));
        assert_eq!(
            state(&value(false, false)),
            Some(VoiceState {
                muted: false,
                deafened: false
            })
        );
        assert_eq!(
            state(&value(false, true)),
            Some(VoiceState {
                muted: true,
                deafened: true
            })
        );
        assert_eq!(state(b"\x01{\"default\":{\"mute\":false}}"), None);
        let json = String::from_utf8(value(true, false)[1..].to_vec()).unwrap();
        let mut utf16 = vec![0];
        for unit in json.encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(state(&utf16), state(&value(true, false)));
    }

    #[test]
    fn log_reads_changes_and_ignores_partial_or_corrupt_uncommitted_tail() {
        let mut bytes = log(10, false, false);
        append_log(
            &mut bytes,
            &write_batch(11, &[(KEY, Some(&value(true, false)))]),
        );
        let expected = Some(Record {
            sequence: 11,
            state: Some(VoiceState {
                muted: true,
                deafened: false,
            }),
        });
        assert_eq!(read_log(&bytes), expected);
        let mut tail = log(12, false, false);
        tail.pop();
        bytes.extend_from_slice(&tail);
        assert_eq!(read_log(&bytes), expected);
        let mut corrupt = log(13, false, false);
        corrupt[0] ^= 1;
        let mut bytes = log(11, true, false);
        bytes.extend_from_slice(&corrupt);
        assert_eq!(read_log(&bytes), expected);
    }

    #[test]
    fn fragmented_batches_and_unrelated_values_do_not_hide_mute_changes() {
        let mut bytes = Vec::new();
        let unrelated = vec![b'x'; 40000];
        append_log(
            &mut bytes,
            &write_batch(
                20,
                &[
                    (b"unrelated", Some(&unrelated)),
                    (KEY, Some(&value(false, true))),
                ],
            ),
        );
        assert_eq!(
            read_log(&bytes),
            Some(Record {
                sequence: 21,
                state: Some(VoiceState {
                    muted: true,
                    deafened: true
                })
            })
        );
        let mut deletion = Vec::new();
        append_log(&mut deletion, &write_batch(22, &[(KEY, None)]));
        assert_eq!(
            read_log(&deletion),
            Some(Record {
                sequence: 22,
                state: None
            })
        );
    }

    #[test]
    fn tables_support_compression_and_validate_checksums() {
        for compressed in [false, true] {
            let bytes = table(8, true, false, compressed);
            assert_eq!(
                read_table(&bytes),
                Some(Some(Record {
                    sequence: 8,
                    state: Some(VoiceState {
                        muted: true,
                        deafened: false
                    })
                }))
            );
            let mut corrupt = bytes;
            corrupt[0] ^= 1;
            assert_eq!(read_table(&corrupt), None);
        }
        assert_eq!(
            checksum(b'9', b"12345678", false),
            0xe3069283u32.rotate_right(15).wrapping_add(0xa282ead8)
        );
    }

    #[test]
    fn snappy_checks_bounds_and_all_copy_encodings() {
        for bytes in [
            vec![6, 0, b'a', 5, 1],
            vec![6, 0, b'a', 18, 1, 0],
            vec![6, 0, b'a', 19, 1, 0, 0, 0],
        ] {
            assert_eq!(snappy(&bytes), Some(b"aaaaaa".to_vec()));
        }
        assert_eq!(snappy(&[6, 0, b'a', 5, 0]), None);
        for bytes in [
            table(5, false, true, true),
            log(5, true, false),
            literal(b"test"),
        ] {
            for end in 0..bytes.len() {
                let _ = read_table(&bytes[..end]);
                let _ = read_log(&bytes[..end]);
                let _ = snappy(&bytes[..end]);
            }
        }
    }

    #[test]
    fn cached_reader_follows_live_log_updates_and_compaction_without_writing_database() {
        let directory = std::env::temp_dir().join(format!(
            "vorssaint-discord-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        let base = directory.join("000001.ldb");
        let journal = directory.join("000002.log");
        fs::write(&base, table(1, false, false, true)).unwrap();
        fs::write(&journal, log(2, true, false)).unwrap();
        let mut reader = Reader::default();
        assert_eq!(
            reader.read(&directory),
            Some(VoiceState {
                muted: true,
                deafened: false
            })
        );
        let mut bytes = log(2, true, false);
        append_log(
            &mut bytes,
            &write_batch(3, &[(KEY, Some(&value(false, false)))]),
        );
        fs::write(&journal, &bytes).unwrap();
        assert_eq!(
            reader.read(&directory),
            Some(VoiceState {
                muted: false,
                deafened: false
            })
        );
        fs::write(&base, table(3, false, false, true)).unwrap();
        fs::remove_file(&journal).unwrap();
        assert_eq!(
            reader.read(&directory),
            Some(VoiceState {
                muted: false,
                deafened: false
            })
        );
        let config = b"\x01{\"_state\":{\"test\":{\"enabled\":true,\"action\":\"TOGGLE_MUTE\",\"shortcut\":[[0,74,1]]}}}";
        let mut bytes = Vec::new();
        append_log(
            &mut bytes,
            &write_batch(4, &[(b"_https://discord.com\0\x01keybinds", Some(config))]),
        );
        fs::write(&journal, bytes).unwrap();
        let mut bindings = Reader::<Vec<crate::discord_keys::Binding>>::default();
        assert_eq!(bindings.bindings(&directory).unwrap()[0].keys, vec![74]);
        let disabled = b"\x01{\"_state\":{\"test\":{\"enabled\":false,\"action\":\"TOGGLE_MUTE\",\"shortcut\":[[0,74,1]]}}}";
        let mut bytes = Vec::new();
        append_log(
            &mut bytes,
            &write_batch(
                5,
                &[(b"_https://discord.com\0\x01keybinds", Some(disabled))],
            ),
        );
        fs::write(&journal, bytes).unwrap();
        assert!(bindings.bindings(&directory).unwrap().is_empty());
        fs::remove_file(&journal).unwrap();
        fs::remove_file(&base).unwrap();
        fs::remove_dir(&directory).unwrap();
    }
}
