// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! "Create ZIP" (NotchFileToolsService.archive, NotchArchiveOperation; spec
//! 05 §6.2). Each input becomes its own Deflate ZIP: a folder keeps its own
//! name as the top entry, like `ditto --keepParent`. The archive is written
//! to a staged file beside the destination, then renamed into place without
//! ever replacing an existing file. A destination inside one of the inputs
//! is refused by file identity, not by comparing path spellings. Cancelling
//! sets a flag that is checked between entries and between chunks.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::identity;

/// Why an archive did not finish, for the page's failure line.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Failure {
    /// Cancelled by the person: no failure line, just "Cancelled".
    Cancelled,
    /// The destination would sit inside one of the inputs.
    Inside,
    /// Something already has the output's name.
    Exists { name: String },
    /// An input could not be read, or the archive could not be written.
    Io { name: String, detail: String },
}

/// `<name>.zip` in `dir`, or `<name> 2.zip`, `<name> 3.zip`… when taken.
/// The name keeps its own extension ("report.pdf.zip"), as the Mac's does.
pub fn unique_zip_path(dir: &Path, base: &str, reserved: &HashSet<PathBuf>) -> PathBuf {
    // Leave room for " 99999.zip" under Windows' 255-character name limit.
    let base: String = base.chars().take(240).collect();
    let free = |path: &PathBuf| !reserved.contains(path) && fs::symlink_metadata(path).is_err();
    let first = dir.join(format!("{base}.zip"));
    if free(&first) {
        return first;
    }
    (2..)
        .map(|n| dir.join(format!("{base} {n}.zip")))
        .find(free)
        .expect("an unbounded search finds a free name")
}

/// True when `destination`, or the folder it would be created in, is one of
/// the inputs or lies inside one. Every existing ancestor is compared by
/// identity, so a junction, a different case or a short 8.3 name can't hide
/// it. A missing input, or one that can't be identified, counts as unsafe.
pub fn inside_inputs(destination: &Path, inputs: &[PathBuf]) -> bool {
    let mut inputs_ids = Vec::new();
    for input in inputs {
        if input.parent().is_none() {
            return true;
        }
        match identity::of(input) {
            Some(id) => inputs_ids.push(id),
            None => return true,
        }
    }
    destination
        .ancestors()
        .filter_map(identity::of)
        .any(|id| inputs_ids.contains(&id))
}

/// Zips `input` into `output`. Never overwrites `output`.
pub fn archive(input: &Path, output: &Path, cancel: &AtomicBool) -> Result<(), Failure> {
    let input_name = name_of(input);
    let output_name = name_of(output);
    if cancel.load(Ordering::Relaxed) {
        return Err(Failure::Cancelled);
    }
    if inside_inputs(output, &[input.to_path_buf()]) {
        return Err(Failure::Inside);
    }
    if fs::symlink_metadata(output).is_ok() {
        return Err(Failure::Exists { name: output_name });
    }
    let meta = fs::metadata(input).map_err(|e| io_failure(&input_name, e))?;
    let dir = output.parent().unwrap_or(Path::new("."));
    let staged = dir.join(format!("{output_name}.{}.partial", &super::store::new_id()[..8]));
    let result = (|| {
        let file = File::create_new(&staged).map_err(|e| io_failure(&output_name, e))?;
        let mut zip = ZipWriter::new(BufWriter::new(file));
        if meta.is_dir() {
            add_dir(&mut zip, input, &input_name, cancel)?;
        } else {
            add_file(&mut zip, input, &input_name, cancel)?;
        }
        let writer = zip.finish().map_err(|e| io_failure(&output_name, e.into()))?;
        let file = writer.into_inner().map_err(|e| io_failure(&output_name, e.into_error()))?;
        file.sync_all().map_err(|e| io_failure(&output_name, e))?;
        drop(file);
        if cancel.load(Ordering::Relaxed) {
            return Err(Failure::Cancelled);
        }
        identity::rename_no_replace(&staged, output).map_err(|e| match e.kind() {
            io::ErrorKind::AlreadyExists => Failure::Exists { name: output_name.clone() },
            _ => io_failure(&output_name, e),
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staged);
    }
    result
}

type Zip = ZipWriter<BufWriter<File>>;

fn options(meta: &fs::Metadata) -> SimpleFileOptions {
    let mut options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .large_file(meta.len() >= u32::MAX as u64);
    if let Some(time) = meta.modified().ok().and_then(zip_time) {
        options = options.last_modified_time(time);
    }
    options
}

/// ZIP times are local wall-clock times, as Explorer writes and reads them.
fn zip_time(modified: std::time::SystemTime) -> Option<zip::DateTime> {
    let utc = time::OffsetDateTime::from(modified);
    let local = time::UtcOffset::current_local_offset()
        .map(|offset| utc.to_offset(offset))
        .unwrap_or(utc);
    zip::DateTime::try_from(time::PrimitiveDateTime::new(local.date(), local.time())).ok()
}

fn add_file(zip: &mut Zip, path: &Path, entry: &str, cancel: &AtomicBool) -> Result<(), Failure> {
    let name = name_of(path);
    let mut source = File::open(path).map_err(|e| io_failure(&name, e))?;
    let meta = source.metadata().map_err(|e| io_failure(&name, e))?;
    zip.start_file(entry, options(&meta)).map_err(|e| io_failure(&name, e.into()))?;
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(Failure::Cancelled);
        }
        let read = source.read(&mut buffer).map_err(|e| io_failure(&name, e))?;
        if read == 0 {
            return Ok(());
        }
        zip.write_all(&buffer[..read]).map_err(|e| io_failure(&name, e))?;
    }
}

/// The folder itself, then everything under it. Linked folders (symlinks,
/// junctions) are not followed, so a loop can't grow the archive forever.
fn add_dir(zip: &mut Zip, dir: &Path, entry: &str, cancel: &AtomicBool) -> Result<(), Failure> {
    if cancel.load(Ordering::Relaxed) {
        return Err(Failure::Cancelled);
    }
    let name = name_of(dir);
    let meta = fs::metadata(dir).map_err(|e| io_failure(&name, e))?;
    zip.add_directory(format!("{entry}/"), options(&meta))
        .map_err(|e| io_failure(&name, e.into()))?;
    let mut children: Vec<_> = fs::read_dir(dir)
        .map_err(|e| io_failure(&name, e))?
        .collect::<Result<_, _>>()
        .map_err(|e| io_failure(&name, e))?;
    children.sort_by_key(|c| c.file_name());
    for child in children {
        let path = child.path();
        let child_entry = format!("{entry}/{}", child.file_name().to_string_lossy());
        let kind = fs::symlink_metadata(&path).map_err(|e| io_failure(&name_of(&path), e))?;
        if kind.is_dir() && !is_link(&kind) {
            add_dir(zip, &path, &child_entry, cancel)?;
        } else if fs::metadata(&path).is_ok_and(|m| m.is_file()) {
            add_file(zip, &path, &child_entry, cancel)?;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn is_link(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    // FILE_ATTRIBUTE_REPARSE_POINT: symlinks and junctions alike.
    meta.file_type().is_symlink() || meta.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_link(meta: &fs::Metadata) -> bool {
    meta.file_type().is_symlink()
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn io_failure(name: &str, error: io::Error) -> Failure {
    Failure::Io { name: name.to_string(), detail: error.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vors-zip-{}", super::super::store::new_id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn names_are_unique_and_keep_the_extension() {
        let dir = temp();
        let none = HashSet::new();
        assert_eq!(unique_zip_path(&dir, "report.pdf", &none), dir.join("report.pdf.zip"));
        fs::write(dir.join("report.pdf.zip"), "").unwrap();
        assert_eq!(unique_zip_path(&dir, "report.pdf", &none), dir.join("report.pdf 2.zip"));
        let reserved = HashSet::from([dir.join("report.pdf 2.zip")]);
        assert_eq!(unique_zip_path(&dir, "report.pdf", &reserved), dir.join("report.pdf 3.zip"));
        assert_eq!(unique_zip_path(&dir, "Folder", &none), dir.join("Folder.zip"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_destination_inside_an_input_is_refused() {
        let dir = temp();
        let input = dir.join("Project");
        fs::create_dir_all(input.join("sub")).unwrap();
        fs::write(dir.join("loose.txt"), "x").unwrap();
        let inputs = [input.clone()];
        assert!(inside_inputs(&input.join("Project.zip"), &inputs));
        assert!(inside_inputs(&input.join("sub/deeper/out.zip"), &inputs));
        assert!(inside_inputs(&input, &inputs));
        assert!(!inside_inputs(&dir.join("Project.zip"), &inputs));
        // A file input: only the file itself is "inside".
        let file = [dir.join("loose.txt")];
        assert!(inside_inputs(&dir.join("loose.txt"), &file));
        assert!(!inside_inputs(&dir.join("loose.txt.zip"), &file));
        // A missing input can't be checked, so it is refused.
        assert!(inside_inputs(&dir.join("x.zip"), &[dir.join("missing")]));
        #[cfg(unix)]
        {
            // Another spelling of the same folder is caught by identity.
            std::os::unix::fs::symlink(&input, dir.join("alias")).unwrap();
            assert!(inside_inputs(&dir.join("alias/out.zip"), &inputs));
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_folder_is_zipped_under_its_own_name_and_never_overwrites() {
        let dir = temp();
        let input = dir.join("Docs");
        fs::create_dir_all(input.join("empty")).unwrap();
        fs::write(input.join("a.txt"), "hello").unwrap();
        let output = dir.join("Docs.zip");
        let cancel = AtomicBool::new(false);
        archive(&input, &output, &cancel).unwrap();
        let mut zip = zip::ZipArchive::new(File::open(&output).unwrap()).unwrap();
        let mut names: Vec<_> = zip.file_names().map(str::to_string).collect();
        names.sort();
        assert_eq!(names, ["Docs/", "Docs/a.txt", "Docs/empty/"]);
        let mut text = String::new();
        zip.by_name("Docs/a.txt").unwrap().read_to_string(&mut text).unwrap();
        assert_eq!(text, "hello");
        assert_eq!(archive(&input, &output, &cancel), Err(Failure::Exists { name: "Docs.zip".into() }));
        assert_eq!(archive(&input, &input.join("in.zip"), &cancel), Err(Failure::Inside));
        // No staged leftovers.
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_cancelled_job_leaves_nothing_behind() {
        let dir = temp();
        fs::write(dir.join("a.txt"), "hello").unwrap();
        let cancel = AtomicBool::new(true);
        assert_eq!(archive(&dir.join("a.txt"), &dir.join("a.txt.zip"), &cancel), Err(Failure::Cancelled));
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }
}
