// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The network detail's speed test (SpeedTest.swift on the Mac): latency,
//! then download, then upload, against Cloudflare's public speed endpoints
//! (the ones speed.cloudflare.com uses). Each transfer is time-boxed so the
//! test stays bounded on any connection (every wait in it times out when the
//! box closes), and the upload body is zeros, so no
//! user data leaves the PC. Started by the user only; there is no cancel, as
//! on the Mac.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[cfg_attr(not(windows), allow(dead_code))]
const HOST: &str = "speed.cloudflare.com";
/// Round trips timed for latency; the best one counts.
const PINGS: u32 = 5;
/// How long each transfer is measured for.
const SAMPLE: Duration = Duration::from_secs(5);
/// Cloudflare's `__down` caps a response just under 100 MB, so downloads ask
/// for less and loop back-to-back until the time box, keeping a fast link full.
const DOWNLOAD_BYTES: u64 = 90_000_000;
const UPLOAD_BYTES: u64 = 100_000_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    #[default]
    Idle,
    Latency,
    Download,
    Upload,
    Done,
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub phase: Phase,
    pub latency_ms: Option<f64>,
    pub download_mbps: Option<f64>,
    pub upload_mbps: Option<f64>,
    /// Why it failed, for the probe; the island only says that it did.
    pub error: Option<String>,
}

impl State {
    pub fn running(&self) -> bool {
        matches!(self.phase, Phase::Latency | Phase::Download | Phase::Upload)
    }
}

#[derive(Clone, Default)]
pub struct Shared(Arc<Mutex<State>>);

/// Why a request ended early.
#[derive(Debug, PartialEq)]
pub enum Failure {
    /// The server answered, but not with a 2xx: never measured, even after data.
    Rejected(u32),
    /// The connection broke; a transfer that already moved bytes keeps them.
    Network(String),
}

/// One run's connection to the speed endpoints. `more` gets each read or
/// written byte count and answers whether to keep going; a stop is an `Ok`.
/// A transfer gives up waiting at `until`, with a `Network` failure.
pub trait Link {
    fn ping(&mut self) -> Result<(), Failure>;
    fn download(
        &mut self,
        bytes: u64,
        until: Instant,
        more: &mut dyn FnMut(u64) -> bool,
    ) -> Result<(), Failure>;
    fn upload(
        &mut self,
        bytes: u64,
        until: Instant,
        more: &mut dyn FnMut(u64) -> bool,
    ) -> Result<(), Failure>;
}

#[derive(Clone, Copy, PartialEq)]
enum Transfer {
    Download,
    Upload,
}

/// Runs the whole test on `link`, handing every change to `report`.
pub fn run(link: &mut dyn Link, sample: Duration, report: &mut dyn FnMut(&State)) -> State {
    let mut state = State {
        phase: Phase::Latency,
        ..State::default()
    };
    report(&state);
    let mut best: Option<f64> = None;
    for _ in 0..PINGS {
        let started = Instant::now();
        if let Err(failure) = link.ping() {
            return failed(state, failure, report);
        }
        let rtt = started.elapsed().as_secs_f64() * 1000.0;
        best = Some(best.map_or(rtt, |b| b.min(rtt)));
    }
    state.latency_ms = best;

    for transfer in [Transfer::Download, Transfer::Upload] {
        state.phase = match transfer {
            Transfer::Download => Phase::Download,
            Transfer::Upload => Phase::Upload,
        };
        report(&state);
        match measure(link, transfer, sample) {
            Ok(mbps) if transfer == Transfer::Download => state.download_mbps = Some(mbps),
            Ok(mbps) => state.upload_mbps = Some(mbps),
            Err(failure) => return failed(state, failure, report),
        }
    }
    state.phase = Phase::Done;
    report(&state);
    state
}

fn failed(mut state: State, failure: Failure, report: &mut dyn FnMut(&State)) -> State {
    state.phase = Phase::Failed;
    state.error = Some(match failure {
        Failure::Rejected(status) => format!("HTTP {status}"),
        Failure::Network(message) => message,
    });
    report(&state);
    state
}

/// Megabits per second over the time box. Downloads repeat until the box
/// closes, also after a dropped connection; the upload is one body.
fn measure(link: &mut dyn Link, transfer: Transfer, sample: Duration) -> Result<f64, Failure> {
    let started = Instant::now();
    let until = started + sample;
    let moved = std::cell::Cell::new(0u64);
    let mut more = |n: u64| {
        moved.set(moved.get() + n);
        started.elapsed() < sample
    };
    loop {
        let result = match transfer {
            Transfer::Download => link.download(DOWNLOAD_BYTES, until, &mut more),
            Transfer::Upload => link.upload(UPLOAD_BYTES, until, &mut more),
        };
        let open = started.elapsed() < sample;
        match result {
            // The box closed on a wait: what moved counts, as when the Mac's time box cancels.
            Err(Failure::Network(_)) if !open => break,
            Err(Failure::Network(_)) if moved.get() > 0 && transfer == Transfer::Download => {
                continue;
            }
            Err(Failure::Network(_)) if moved.get() > 0 => break,
            Err(failure) => return Err(failure),
            Ok(()) if transfer == Transfer::Download && open => continue,
            Ok(()) => break,
        }
    }
    let elapsed = started.elapsed().as_secs_f64();
    Ok(if elapsed > 0.0 {
        moved.get() as f64 * 8.0 / elapsed / 1_000_000.0
    } else {
        0.0
    })
}

fn emit(app: &AppHandle, state: &State) {
    let _ = app.emit_to(crate::island::LABEL, "speedtest://state", state);
}

#[tauri::command]
pub fn speed_test_state(state: tauri::State<'_, Shared>) -> State {
    state.0.lock().unwrap().clone()
}

/// Starts a test unless one is running; results so far are cleared at once.
#[tauri::command]
pub fn speed_test_start(app: AppHandle, state: tauri::State<'_, Shared>) -> State {
    let shared = state.inner().clone();
    let first = {
        let mut current = shared.0.lock().unwrap();
        if current.running() {
            return current.clone();
        }
        *current = State {
            phase: Phase::Latency,
            ..State::default()
        };
        current.clone()
    };
    emit(&app, &first);
    std::thread::spawn(move || {
        run_online(&mut |s: &State| {
            *shared.0.lock().unwrap() = s.clone();
            emit(&app, s);
        })
    });
    first
}

/// Runs the test against Cloudflare.
fn run_online(report: &mut dyn FnMut(&State)) {
    match WinHttp::open() {
        Ok(mut link) => {
            run(&mut link, SAMPLE, report);
        }
        Err(failure) => {
            failed(State::default(), failure, report);
        }
    }
}

/// `Vorssaint.exe --speed-test-probe <file>`: runs a test, writing each step
/// and the result as JSON lines.
pub fn probe_main(path: &str) -> ! {
    let mut lines = Vec::new();
    run_online(&mut |s: &State| lines.push(serde_json::to_string(s).unwrap_or_default()));
    let _ = std::fs::write(path, lines.join("\n") + "\n");
    std::process::exit(0)
}

/// WinHTTP over one session, so the pings after the first reuse its
/// connection (as URLSession does on the Mac).
#[cfg(windows)]
struct WinHttp {
    // Fields drop in order: the connection closes before its session.
    connect: Handle,
    _session: Handle,
}

#[cfg(windows)]
struct Handle(*mut core::ffi::c_void);

/// A request, and when its waits must end.
#[cfg(windows)]
struct Request {
    handle: Handle,
    until: Option<Instant>,
}

#[cfg(windows)]
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = windows::Win32::Networking::WinHttp::WinHttpCloseHandle(self.0);
            }
        }
    }
}

#[cfg(windows)]
fn network(error: windows::core::Error) -> Failure {
    // WinHTTP's own errors (12xxx) have no system message, so the code goes too.
    let code = format!("{:#010x}", error.code().0);
    let message = error.message();
    Failure::Network(if message.is_empty() {
        code
    } else {
        format!("{message} ({code})")
    })
}

#[cfg(windows)]
impl WinHttp {
    fn open() -> Result<Self, Failure> {
        use windows::Win32::Networking::WinHttp::*;
        use windows::core::{HSTRING, PCWSTR};
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
                return Err(network(windows::core::Error::from_thread()));
            }
            // The Mac's 20 s request timeout, for the pings.
            WinHttpSetTimeouts(session.0, 20_000, 20_000, 20_000, 20_000).map_err(network)?;
            let connect = Handle(WinHttpConnect(
                session.0,
                &HSTRING::from(HOST),
                INTERNET_DEFAULT_HTTPS_PORT,
                0,
            ));
            if connect.0.is_null() {
                return Err(network(windows::core::Error::from_thread()));
            }
            Ok(Self {
                connect,
                _session: session,
            })
        }
    }

    /// An uncached, cookieless request, not yet sent.
    fn request(&self, verb: &str, path: &str, until: Option<Instant>) -> Result<Request, Failure> {
        use windows::Win32::Networking::WinHttp::*;
        use windows::core::{HSTRING, PCWSTR};
        unsafe {
            let request = Handle(WinHttpOpenRequest(
                self.connect.0,
                &HSTRING::from(verb),
                &HSTRING::from(path),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                WINHTTP_FLAG_SECURE | WINHTTP_FLAG_REFRESH,
            ));
            if request.0.is_null() {
                return Err(network(windows::core::Error::from_thread()));
            }
            let no_cookies = WINHTTP_DISABLE_COOKIES.to_le_bytes();
            WinHttpSetOption(
                Some(request.0),
                WINHTTP_OPTION_DISABLE_FEATURE,
                Some(&no_cookies),
            )
            .map_err(network)?;
            Ok(Request {
                handle: request,
                until,
            })
        }
    }
}

#[cfg(windows)]
impl Request {
    /// With `until`, the next wait times out then. WinHTTP's timeouts count
    /// per call, so this runs before each one: a stall ends with the box.
    fn bound(&self) -> Result<(), Failure> {
        use windows::Win32::Networking::WinHttp::WinHttpSetTimeouts;
        let Some(until) = self.until else {
            return Ok(());
        };
        // 0 would mean never; 1 ms times out at once.
        let left = until
            .saturating_duration_since(Instant::now())
            .as_millis()
            .clamp(1, 20_000) as i32;
        unsafe { WinHttpSetTimeouts(self.handle.0, left, left, left, left).map_err(network) }
    }

    fn send(&self, headers: Option<&[u16]>, total: u32) -> Result<(), Failure> {
        use windows::Win32::Networking::WinHttp::WinHttpSendRequest;
        self.bound()?;
        unsafe { WinHttpSendRequest(self.handle.0, headers, None, 0, total, 0).map_err(network) }
    }

    /// Waits for the answer; anything but a 2xx is a rejection.
    fn answer(&self) -> Result<(), Failure> {
        use windows::Win32::Networking::WinHttp::*;
        use windows::core::PCWSTR;
        self.bound()?;
        let request = &self.handle;
        unsafe {
            WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(network)?;
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
            .map_err(network)?;
            if (200..300).contains(&status) {
                Ok(())
            } else {
                Err(Failure::Rejected(status))
            }
        }
    }

    /// Reads the body, telling `more` each read; false stops it.
    fn read(&self, more: &mut dyn FnMut(u64) -> bool) -> Result<(), Failure> {
        use windows::Win32::Networking::WinHttp::WinHttpReadData;
        let mut buffer = vec![0u8; 64 << 10];
        loop {
            let mut read = 0u32;
            self.bound()?;
            unsafe {
                WinHttpReadData(
                    self.handle.0,
                    buffer.as_mut_ptr().cast(),
                    buffer.len() as u32,
                    &mut read,
                )
                .map_err(network)?;
            }
            if read == 0 || !more(u64::from(read)) {
                return Ok(());
            }
        }
    }
}

#[cfg(windows)]
impl Link for WinHttp {
    fn ping(&mut self) -> Result<(), Failure> {
        let request = self.request("GET", "/__down?bytes=0", None)?;
        request.send(None, 0)?;
        request.answer()?;
        request.read(&mut |_| true)
    }

    fn download(
        &mut self,
        bytes: u64,
        until: Instant,
        more: &mut dyn FnMut(u64) -> bool,
    ) -> Result<(), Failure> {
        let request = self.request("GET", &format!("/__down?bytes={bytes}"), Some(until))?;
        request.send(None, 0)?;
        request.answer()?;
        request.read(more)
    }

    fn upload(
        &mut self,
        bytes: u64,
        until: Instant,
        more: &mut dyn FnMut(u64) -> bool,
    ) -> Result<(), Failure> {
        use windows::Win32::Networking::WinHttp::WinHttpWriteData;
        let request = self.request("POST", "/__up", Some(until))?;
        let headers: Vec<u16> = "Content-Type: application/octet-stream"
            .encode_utf16()
            .collect();
        let total = u32::try_from(bytes).unwrap_or(u32::MAX);
        request.send(Some(&headers), total)?;
        let zeros = vec![0u8; 64 << 10];
        let mut left = u64::from(total);
        while left > 0 {
            let chunk = left.min(zeros.len() as u64) as u32;
            let mut written = 0u32;
            request.bound()?;
            unsafe {
                WinHttpWriteData(
                    request.handle.0,
                    Some(zeros.as_ptr().cast()),
                    chunk,
                    &mut written,
                )
                .map_err(network)?;
            }
            left -= u64::from(written);
            // Stopped mid-body: dropping the request aborts it.
            if !more(u64::from(written)) {
                return Ok(());
            }
        }
        request.answer()?;
        request.read(&mut |_| true)
    }
}

/// Builds off Windows for the tests; it can never connect.
#[cfg(not(windows))]
struct WinHttp;

#[cfg(not(windows))]
impl WinHttp {
    fn open() -> Result<Self, Failure> {
        Err(Failure::Network("unsupported".into()))
    }
}

#[cfg(not(windows))]
impl Link for WinHttp {
    fn ping(&mut self) -> Result<(), Failure> {
        unreachable!()
    }
    fn download(
        &mut self,
        _: u64,
        _: Instant,
        _: &mut dyn FnMut(u64) -> bool,
    ) -> Result<(), Failure> {
        unreachable!()
    }
    fn upload(
        &mut self,
        _: u64,
        _: Instant,
        _: &mut dyn FnMut(u64) -> bool,
    ) -> Result<(), Failure> {
        unreachable!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A link that answers from a script: each request takes the next
    /// outcome, and a transfer delivers `chunk` bytes per call until stopped
    /// or its limit of calls runs out (`limit` downloading, `upload_limit`
    /// uploading). With `stall`, it then hangs until its wait times out.
    struct Scripted {
        outcomes: Vec<Result<(), Failure>>,
        requests: Vec<&'static str>,
        chunk: u64,
        limit: usize,
        upload_limit: usize,
        stall: bool,
    }

    impl Scripted {
        fn new(outcomes: Vec<Result<(), Failure>>) -> Self {
            Self {
                outcomes,
                requests: Vec::new(),
                chunk: 1_000_000,
                limit: usize::MAX,
                upload_limit: usize::MAX,
                stall: false,
            }
        }

        fn next(&mut self, kind: &'static str) -> Result<(), Failure> {
            self.requests.push(kind);
            if self.outcomes.is_empty() {
                Ok(())
            } else {
                self.outcomes.remove(0)
            }
        }

        fn transfer(
            &mut self,
            kind: &'static str,
            until: Instant,
            more: &mut dyn FnMut(u64) -> bool,
        ) -> Result<(), Failure> {
            let outcome = self.next(kind);
            if let Err(Failure::Rejected(_)) = outcome {
                return outcome;
            }
            let limit = if kind == "upload" {
                self.upload_limit
            } else {
                self.limit
            };
            for _ in 0..limit {
                std::thread::sleep(Duration::from_millis(2));
                if !more(self.chunk) {
                    return Ok(());
                }
            }
            if self.stall {
                std::thread::sleep(until.saturating_duration_since(Instant::now()));
                return Err(Failure::Network("timed out".into()));
            }
            outcome
        }
    }

    impl Link for Scripted {
        fn ping(&mut self) -> Result<(), Failure> {
            self.next("ping")
        }
        fn download(
            &mut self,
            _: u64,
            until: Instant,
            more: &mut dyn FnMut(u64) -> bool,
        ) -> Result<(), Failure> {
            self.transfer("download", until, more)
        }
        fn upload(
            &mut self,
            _: u64,
            until: Instant,
            more: &mut dyn FnMut(u64) -> bool,
        ) -> Result<(), Failure> {
            self.transfer("upload", until, more)
        }
    }

    const QUICK: Duration = Duration::from_millis(40);

    fn pings() -> Vec<Result<(), Failure>> {
        (0..PINGS).map(|_| Ok(())).collect()
    }

    fn phases(link: &mut Scripted) -> (State, Vec<Phase>) {
        let mut seen = Vec::new();
        let end = run(link, QUICK, &mut |s| seen.push(s.phase));
        (end, seen)
    }

    #[test]
    fn a_clean_run_measures_every_phase_in_order() {
        let mut link = Scripted::new(pings());
        let (end, seen) = phases(&mut link);
        assert_eq!(
            seen,
            [Phase::Latency, Phase::Download, Phase::Upload, Phase::Done]
        );
        assert!(end.latency_ms.is_some());
        assert!(end.download_mbps.unwrap() > 0.0);
        assert!(end.upload_mbps.unwrap() > 0.0);
        assert_eq!(&link.requests[..PINGS as usize], ["ping"; PINGS as usize]);
    }

    #[test]
    fn a_rejected_ping_fails_before_any_transfer() {
        let mut link = Scripted::new(vec![Err(Failure::Rejected(500))]);
        let (end, seen) = phases(&mut link);
        assert_eq!(end.phase, Phase::Failed);
        assert_eq!(seen, [Phase::Latency, Phase::Failed]);
        assert_eq!(end.latency_ms, None);
        assert_eq!(end.error.as_deref(), Some("HTTP 500"));
        assert_eq!(link.requests, ["ping"]);
    }

    #[test]
    fn a_broken_ping_fails_too() {
        let mut link = Scripted::new(vec![Ok(()), Err(Failure::Network("reset".into()))]);
        let (end, _) = phases(&mut link);
        assert_eq!(end.phase, Phase::Failed);
        assert_eq!(end.latency_ms, None);
    }

    #[test]
    fn a_rejected_download_counts_nothing() {
        let mut outcomes = pings();
        outcomes.push(Err(Failure::Rejected(404)));
        let mut link = Scripted::new(outcomes);
        let (end, _) = phases(&mut link);
        assert_eq!(end.phase, Phase::Failed);
        assert!(end.latency_ms.is_some());
        assert_eq!(end.download_mbps, None);
        assert_eq!(end.upload_mbps, None);
    }

    #[test]
    fn a_rejection_after_download_data_still_fails() {
        // The first chunk ends early with data; the next is answered 503.
        let mut outcomes = pings();
        outcomes.extend([Ok(()), Err(Failure::Rejected(503))]);
        let mut link = Scripted::new(outcomes);
        link.limit = 1;
        let (end, _) = phases(&mut link);
        assert_eq!(end.phase, Phase::Failed);
        assert_eq!(end.download_mbps, None);
        assert_eq!(
            link.requests.iter().filter(|r| **r == "download").count(),
            2
        );
    }

    #[test]
    fn a_rejected_upload_keeps_the_download() {
        let mut outcomes = pings();
        outcomes.extend([Ok(()), Err(Failure::Rejected(503))]);
        let mut link = Scripted::new(outcomes);
        let (end, _) = phases(&mut link);
        assert_eq!(end.phase, Phase::Failed);
        assert!(end.download_mbps.unwrap() > 0.0);
        assert_eq!(end.upload_mbps, None);
    }

    #[test]
    fn a_connection_lost_after_data_still_measures() {
        let mut outcomes = pings();
        outcomes.push(Err(Failure::Network("reset".into())));
        let mut link = Scripted::new(outcomes);
        link.limit = 3;
        let (end, _) = phases(&mut link);
        assert_eq!(end.phase, Phase::Done);
        assert!(end.download_mbps.unwrap() > 0.0);
        // As on the Mac, the box is filled with a new request.
        assert!(link.requests.iter().filter(|r| **r == "download").count() > 1);
    }

    #[test]
    fn an_upload_lost_after_data_still_measures() {
        let mut outcomes = pings();
        outcomes.extend([Ok(()), Err(Failure::Network("reset".into()))]);
        let mut link = Scripted::new(outcomes);
        link.upload_limit = 3;
        let (end, _) = phases(&mut link);
        assert_eq!(end.phase, Phase::Done);
        assert!(end.upload_mbps.unwrap() > 0.0);
        assert_eq!(link.requests.iter().filter(|r| **r == "upload").count(), 1);
    }

    #[test]
    fn a_stall_before_any_data_reads_zero_when_the_box_closes() {
        let mut link = Scripted::new(pings());
        link.limit = 0;
        link.upload_limit = 0;
        link.stall = true;
        let started = Instant::now();
        let (end, _) = phases(&mut link);
        assert_eq!(end.phase, Phase::Done);
        assert_eq!(end.download_mbps, Some(0.0));
        assert_eq!(end.upload_mbps, Some(0.0));
        assert!(started.elapsed() < QUICK * 4, "{:?}", started.elapsed());
    }

    #[test]
    fn a_stall_after_data_ends_with_the_box() {
        let mut link = Scripted::new(pings());
        link.limit = 3;
        link.chunk = 125_000;
        link.stall = true;
        let started = Instant::now();
        let (end, _) = phases(&mut link);
        assert_eq!(end.phase, Phase::Done);
        // 3 Mb over a ~40 ms box, not over a longer stall.
        let down = end.download_mbps.unwrap();
        assert!(down > 30.0 && down < 100.0, "{down}");
        assert!(started.elapsed() < QUICK * 4, "{:?}", started.elapsed());
    }

    #[test]
    fn a_connection_lost_before_data_fails() {
        let mut outcomes = pings();
        outcomes.push(Err(Failure::Network("refused".into())));
        let mut link = Scripted::new(outcomes);
        link.limit = 0;
        let (end, _) = phases(&mut link);
        assert_eq!(end.phase, Phase::Failed);
        assert_eq!(end.download_mbps, None);
    }

    #[test]
    fn downloads_repeat_until_the_time_box_closes() {
        let mut link = Scripted::new(pings());
        link.limit = 2;
        let (end, _) = phases(&mut link);
        assert_eq!(end.phase, Phase::Done);
        assert!(link.requests.iter().filter(|r| **r == "download").count() > 1);
        // The upload is one body, whatever is left of the box.
        let uploads = link.requests.iter().filter(|r| **r == "upload").count();
        assert_eq!(uploads, 1);
    }

    #[test]
    fn the_rate_is_megabits_per_second() {
        let mut link = Scripted::new(pings());
        link.chunk = 125_000; // 1 Mb every ~2 ms: ~500 Mbps, well under 10 Gbps
        let (end, _) = phases(&mut link);
        let down = end.download_mbps.unwrap();
        assert!(down > 50.0 && down < 1_000.0, "{down}");
    }

    #[test]
    fn the_state_reads_camel_case() {
        let json = serde_json::to_value(State {
            phase: Phase::Download,
            latency_ms: Some(12.5),
            download_mbps: None,
            upload_mbps: None,
            error: None,
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "phase": "download",
                "latencyMs": 12.5,
                "downloadMbps": null,
                "uploadMbps": null,
                "error": null,
            })
        );
    }
}
