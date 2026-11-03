// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The speech model: whisper.cpp's multilingual base model, quantized
//! (ggml-base-q5_1, 60 MB). It is downloaded only when the person asks, in
//! Settings, after the size is shown; it is checked against its SHA-256
//! before it is kept, and can be deleted again. Nothing else is downloaded,
//! and no audio is ever sent.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, Runtime};

pub const FILE: &str = "ggml-base-q5_1.bin";
pub const SIZE: u64 = 59_707_625;
pub const SHA256: &str = "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898";
#[cfg_attr(not(windows), allow(dead_code))]
const HOST: &str = "huggingface.co";
#[cfg_attr(not(windows), allow(dead_code))]
const PATH: &str = "/ggerganov/whisper.cpp/resolve/main/ggml-base-q5_1.bin";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    #[default]
    Missing,
    Downloading,
    Ready,
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub phase: Phase,
    /// Bytes so far while downloading.
    pub downloaded: u64,
    /// What the model weighs, shown before it is downloaded.
    pub size: u64,
    pub error: Option<String>,
}

#[derive(Clone, Default)]
pub struct Shared(Arc<Mutex<Option<Status>>>);

/// Where the model lives: beside settings.json, in its own folder.
pub fn path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("models").join(FILE))
}

/// The same place without the app, for the probe.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn default_path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(
        PathBuf::from(base)
            .join("com.vorssaint.island")
            .join("models")
            .join(FILE),
    )
}

/// A model of the right size is taken as ready; its hash was checked when
/// it was saved, so it isn't read again at every press.
pub fn is_ready(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() == SIZE)
}

fn status_of(path: Option<&Path>) -> Status {
    Status {
        phase: if path.is_some_and(is_ready) {
            Phase::Ready
        } else {
            Phase::Missing
        },
        downloaded: 0,
        size: SIZE,
        error: None,
    }
}

#[tauri::command]
pub fn voice_model_status<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, super::Shared>,
) -> Status {
    if let Some(status) = state.model.0.lock().unwrap().clone() {
        return status;
    }
    status_of(path(&app).as_deref())
}

#[tauri::command]
pub fn voice_model_delete<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, super::Shared>,
) -> Status {
    let mut current = state.model.0.lock().unwrap();
    if current
        .as_ref()
        .is_some_and(|s| s.phase == Phase::Downloading)
    {
        return current.clone().unwrap();
    }
    if let Some(path) = path(&app) {
        let _ = std::fs::remove_file(&path);
    }
    *current = None;
    status_of(path(&app).as_deref())
}

/// Starts the download; progress comes as `voice://model`.
#[tauri::command]
pub fn voice_model_download<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, super::Shared>,
) -> Status {
    let shared = state.model.clone();
    let mut current = shared.0.lock().unwrap();
    if let Some(status) = current.as_ref().filter(|s| s.phase == Phase::Downloading) {
        return status.clone();
    }
    let Some(target) = path(&app) else {
        return Status {
            phase: Phase::Failed,
            size: SIZE,
            error: Some("no folder for the model".into()),
            ..Status::default()
        };
    };
    if is_ready(&target) {
        *current = None;
        return status_of(Some(&target));
    }
    let started = Status {
        phase: Phase::Downloading,
        size: SIZE,
        ..Status::default()
    };
    *current = Some(started.clone());
    drop(current);
    std::thread::spawn(move || {
        let report = |status: Status| {
            *shared.0.lock().unwrap() = Some(status.clone());
            let _ = app.emit("voice://model", status);
        };
        let outcome = download(&target, &mut |downloaded| {
            report(Status {
                phase: Phase::Downloading,
                downloaded,
                size: SIZE,
                error: None,
            })
        });
        match outcome {
            Ok(()) => {
                *shared.0.lock().unwrap() = None;
                let _ = app.emit("voice://model", status_of(Some(&target)));
            }
            Err(error) => report(Status {
                phase: Phase::Failed,
                size: SIZE,
                error: Some(error),
                ..Status::default()
            }),
        }
    });
    started
}

/// Into a `.part` file beside the model, which becomes the model only once
/// it is whole and its hash is right.
fn download(target: &Path, progress: &mut dyn FnMut(u64)) -> Result<(), String> {
    let folder = target.parent().ok_or("no folder")?;
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let part = target.with_extension("part");
    let outcome = fetch(&part, progress).and_then(|()| verify(&part));
    match outcome {
        Ok(()) => std::fs::rename(&part, target).map_err(|e| e.to_string()),
        Err(error) => {
            let _ = std::fs::remove_file(&part);
            Err(error)
        }
    }
}

/// Whether the file is the model this build knows.
pub fn verify(path: &Path) -> Result<(), String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    let mut total = 0u64;
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        total += read as u64;
        hash.update(&buffer[..read]);
    }
    let digest: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if total != SIZE || digest != SHA256 {
        return Err(format!("the download is damaged ({total} bytes, {digest})"));
    }
    Ok(())
}

#[cfg(windows)]
fn fetch(part: &Path, progress: &mut dyn FnMut(u64)) -> Result<(), String> {
    use std::io::Write;
    use windows::Win32::Networking::WinHttp::*;
    use windows::core::{HSTRING, PCWSTR};

    struct Handle(*mut core::ffi::c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    let _ = WinHttpCloseHandle(self.0);
                }
            }
        }
    }
    let failed = |e: windows::core::Error| format!("{} ({:#010x})", e.message(), e.code().0);
    let mut out = std::fs::File::create(part).map_err(|e| e.to_string())?;
    let agent = HSTRING::from(format!("Vorssaint/{}", env!("CARGO_PKG_VERSION")));
    unsafe {
        let session = Handle(WinHttpOpen(
            &agent,
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ));
        if session.0.is_null() {
            return Err(failed(windows::core::Error::from_thread()));
        }
        WinHttpSetTimeouts(session.0, 20_000, 20_000, 30_000, 30_000).map_err(failed)?;
        let connect = Handle(WinHttpConnect(
            session.0,
            &HSTRING::from(HOST),
            INTERNET_DEFAULT_HTTPS_PORT,
            0,
        ));
        if connect.0.is_null() {
            return Err(failed(windows::core::Error::from_thread()));
        }
        // Hugging Face answers with a redirect to its file store; WinHTTP follows it.
        let request = Handle(WinHttpOpenRequest(
            connect.0,
            &HSTRING::from("GET"),
            &HSTRING::from(PATH),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ));
        if request.0.is_null() {
            return Err(failed(windows::core::Error::from_thread()));
        }
        let no_cookies = WINHTTP_DISABLE_COOKIES.to_le_bytes();
        WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_DISABLE_FEATURE,
            Some(&no_cookies),
        )
        .map_err(failed)?;
        WinHttpSendRequest(request.0, None, None, 0, 0, 0).map_err(failed)?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(failed)?;
        let mut status = 0u32;
        let mut size = 4u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut size,
            std::ptr::null_mut(),
        )
        .map_err(failed)?;
        if !(200..300).contains(&status) {
            return Err(format!("the server answered {status}"));
        }
        let mut buffer = vec![0u8; 256 << 10];
        let mut total = 0u64;
        let mut reported = 0u64;
        loop {
            let mut read = 0u32;
            WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
            )
            .map_err(failed)?;
            if read == 0 {
                break;
            }
            out.write_all(&buffer[..read as usize])
                .map_err(|e| e.to_string())?;
            total += u64::from(read);
            if total > SIZE {
                return Err("the download is larger than the model".into());
            }
            if total - reported >= 1 << 20 {
                reported = total;
                progress(total);
            }
        }
        progress(total);
    }
    out.flush().map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn fetch(_part: &Path, _progress: &mut dyn FnMut(u64)) -> Result<(), String> {
    Err("the model is downloaded on Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_damaged_download_is_refused_and_removed() {
        let dir = std::env::temp_dir().join(format!("vors-voice-model-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let part = dir.join("model.part");
        std::fs::write(&part, b"not a model").unwrap();
        let error = verify(&part).unwrap_err();
        assert!(error.contains("11 bytes"), "{error}");
        assert!(verify(&dir.join("missing")).is_err());
        // Only a file of the model's size counts as ready.
        assert!(!is_ready(&part));
        assert!(!is_ready(&dir));
        let target = dir.join(FILE);
        assert_eq!(status_of(Some(&target)).phase, Phase::Missing);
        assert_eq!(status_of(None).size, SIZE);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn status_reads_as_the_settings_page_expects() {
        let json = serde_json::to_value(Status {
            phase: Phase::Downloading,
            downloaded: 5,
            size: SIZE,
            error: None,
        })
        .unwrap();
        assert_eq!(json["phase"], "downloading");
        assert_eq!(json["downloaded"], 5);
        assert_eq!(json["size"], SIZE);
    }
}
