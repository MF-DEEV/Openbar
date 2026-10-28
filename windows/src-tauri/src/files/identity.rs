// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! A file's identity on disk, not its spelling: the volume serial and file
//! id on Windows (FILE_ID_INFO), device and inode elsewhere. The ZIP job
//! compares identities so an archive never lands inside one of its own
//! inputs, and the shelf keeps the id to find a file again after a move or
//! rename on the same volume (best effort; Windows has no bookmarks).

use std::io;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    pub volume: u64,
    pub file: u128,
}

impl Identity {
    /// The form kept in `items.json`.
    pub fn encode(&self) -> String {
        format!("{:x}:{:032x}", self.volume, self.file)
    }

    pub fn decode(text: &str) -> Option<Identity> {
        let (volume, file) = text.split_once(':')?;
        Some(Identity {
            volume: u64::from_str_radix(volume, 16).ok()?,
            file: u128::from_str_radix(file, 16).ok()?,
        })
    }
}

/// Renames without ever replacing what is at `to`.
pub fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    imp::rename_no_replace(from, to)
}

pub fn of(path: &Path) -> Option<Identity> {
    imp::identity(path)
}

/// The file with this identity, wherever it is now on the volume that held
/// `old_path`. None when it is gone or the volume isn't there.
pub fn refind(old_path: &str, id: &str) -> Option<String> {
    let id = Identity::decode(id)?;
    let found = imp::open_by_id(old_path, id)?;
    (of(Path::new(&found)) == Some(id)).then_some(found)
}

#[cfg(windows)]
mod imp {
    use super::Identity;
    use std::io;
    use std::path::Path;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, ExtendedFileIdType, FILE_FLAG_BACKUP_SEMANTICS, FILE_ID_128,
        FILE_ID_DESCRIPTOR, FILE_ID_DESCRIPTOR_0, FILE_ID_INFO, FILE_NAME_NORMALIZED,
        FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FileIdInfo,
        GETFINALPATHNAMEBYHANDLE_FLAGS, GetFileInformationByHandleEx, GetFinalPathNameByHandleW,
        MOVEFILE_WRITE_THROUGH, MoveFileExW, OPEN_EXISTING, OpenFileById, VOLUME_NAME_DOS,
    };
    use windows::core::HSTRING;

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    fn open(path: &Path) -> Option<Handle> {
        unsafe {
            CreateFileW(
                &HSTRING::from(path.as_os_str()),
                FILE_READ_ATTRIBUTES.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                // Directories open only with backup semantics.
                FILE_FLAG_BACKUP_SEMANTICS,
                None,
            )
            .ok()
            .map(Handle)
        }
    }

    fn identity_of(handle: &Handle) -> Option<Identity> {
        let mut info = FILE_ID_INFO::default();
        unsafe {
            GetFileInformationByHandleEx(
                handle.0,
                FileIdInfo,
                &mut info as *mut _ as *mut _,
                std::mem::size_of::<FILE_ID_INFO>() as u32,
            )
            .ok()?;
        }
        Some(Identity {
            volume: info.VolumeSerialNumber,
            file: u128::from_le_bytes(info.FileId.Identifier),
        })
    }

    pub fn identity(path: &Path) -> Option<Identity> {
        identity_of(&open(path)?)
    }

    pub fn open_by_id(old_path: &str, id: Identity) -> Option<String> {
        // Any handle on the volume will do as the hint; its root is the surest.
        let root = Path::new(old_path).ancestors().last()?;
        let volume = open(root)?;
        let descriptor = FILE_ID_DESCRIPTOR {
            dwSize: std::mem::size_of::<FILE_ID_DESCRIPTOR>() as u32,
            Type: ExtendedFileIdType,
            Anonymous: FILE_ID_DESCRIPTOR_0 {
                ExtendedFileId: FILE_ID_128 {
                    Identifier: id.file.to_le_bytes(),
                },
            },
        };
        let found = unsafe {
            OpenFileById(
                volume.0,
                &descriptor,
                FILE_READ_ATTRIBUTES.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                FILE_FLAG_BACKUP_SEMANTICS,
            )
            .ok()
            .map(Handle)?
        };
        let mut buffer = vec![0u16; 32768];
        let len = unsafe {
            GetFinalPathNameByHandleW(
                found.0,
                &mut buffer,
                GETFINALPATHNAMEBYHANDLE_FLAGS(FILE_NAME_NORMALIZED.0 | VOLUME_NAME_DOS.0),
            )
        };
        if len == 0 || len as usize >= buffer.len() {
            return None;
        }
        let path = String::from_utf16_lossy(&buffer[..len as usize]);
        Some(match path.strip_prefix(r"\\?\UNC\") {
            Some(unc) => format!(r"\\{unc}"),
            None => path.strip_prefix(r"\\?\").unwrap_or(&path).to_string(),
        })
    }

    pub fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
        // No MOVEFILE_REPLACE_EXISTING: an existing destination is an error.
        unsafe {
            MoveFileExW(
                &HSTRING::from(from.as_os_str()),
                &HSTRING::from(to.as_os_str()),
                MOVEFILE_WRITE_THROUGH,
            )
            .map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xFFFF))
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::Identity;
    use std::io;
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

    pub fn identity(path: &Path) -> Option<Identity> {
        let meta = std::fs::metadata(path).ok()?;
        Some(Identity {
            volume: meta.dev(),
            file: meta.ino() as u128,
        })
    }

    pub fn open_by_id(_old_path: &str, _id: Identity) -> Option<String> {
        None
    }

    pub fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
        // A hard link fails when `to` exists, unlike rename.
        std::fs::hard_link(from, to)?;
        std::fs::remove_file(from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_round_trip_through_text() {
        let id = Identity {
            volume: 0xABCD_1234,
            file: 0x0123_4567_89ab_cdef_0011_2233_4455_6677,
        };
        assert_eq!(Identity::decode(&id.encode()), Some(id));
        assert_eq!(Identity::decode("nonsense"), None);
    }

    #[test]
    fn rename_never_replaces() {
        let dir =
            std::env::temp_dir().join(format!("vors-ident-{}", crate::files::store::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("a"), dir.join("b"));
        std::fs::write(&a, "new").unwrap();
        std::fs::write(&b, "old").unwrap();
        assert!(rename_no_replace(&a, &b).is_err());
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "old");
        std::fs::remove_file(&b).unwrap();
        rename_no_replace(&a, &b).unwrap();
        assert!(!a.exists());
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "new");
        assert_eq!(of(&b), of(&b));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
