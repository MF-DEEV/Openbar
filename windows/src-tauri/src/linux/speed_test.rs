// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The speed test's connection on Linux, where Windows has WinHTTP: libsoup,
//! which is also the web view's own network stack, so the system's proxy and
//! TLS settings apply. `speed_test.rs` includes this file as its `imp` and
//! uses `Soup` where Windows uses `WinHttp`. One session serves a whole run,
//! so the pings after the first reuse its connection.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use gtk::gio;
use gtk::glib;
use gtk::glib::translate::IntoGlib;
use soup::prelude::*;

use super::{Failure, HOST, Link};
use crate::linux::net::Deadline;

/// The Mac's 20 s request timeout, for the pings.
const PING_LIMIT: Duration = Duration::from_secs(20);

pub struct Soup {
    session: soup::Session,
    base: String,
}

fn network(error: glib::Error) -> Failure {
    Failure::Network(error.message().to_string())
}

impl Soup {
    pub fn open() -> Result<Self, Failure> {
        Ok(Self::at(&format!("https://{HOST}")))
    }

    /// A connection to the speed endpoints served from `base` (a test serves them itself).
    fn at(base: &str) -> Self {
        let session = soup::Session::builder()
            .timeout(20)
            .idle_timeout(20)
            .user_agent(concat!("Openbar/", env!("CARGO_PKG_VERSION")))
            .build();
        Soup {
            session,
            base: base.trim_end_matches('/').to_string(),
        }
    }

    /// An uncached request, not yet sent: no cookie jar is on the session, and
    /// the servers on the way are asked not to answer for it.
    fn message(&self, verb: &str, path: &str) -> Result<soup::Message, Failure> {
        let message = soup::Message::new(verb, &format!("{}{path}", self.base))
            .map_err(|error| Failure::Network(error.to_string()))?;
        if let Some(headers) = message.request_headers() {
            headers.append("Cache-Control", "no-cache");
        }
        Ok(message)
    }

    /// Sends the request and takes the answer's start; anything but a 2xx is a rejection.
    fn answer(
        &self,
        message: &soup::Message,
        cancellable: &gio::Cancellable,
    ) -> Result<gio::InputStream, Failure> {
        let stream = self
            .session
            .send(message, Some(cancellable))
            .map_err(network)?;
        Self::accepted(message)?;
        Ok(stream)
    }

    fn accepted(message: &soup::Message) -> Result<(), Failure> {
        let status = message.status().into_glib() as u32;
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(Failure::Rejected(status))
        }
    }

    /// Reads the body, telling `more` each read; false stops it.
    fn read(
        stream: &gio::InputStream,
        cancellable: &gio::Cancellable,
        more: &mut dyn FnMut(u64) -> bool,
    ) -> Result<(), Failure> {
        loop {
            let chunk = stream
                .read_bytes(64 << 10, Some(cancellable))
                .map_err(network)?;
            if chunk.is_empty() || !more(chunk.len() as u64) {
                return Ok(());
            }
        }
    }
}

impl Link for Soup {
    fn ping(&mut self) -> Result<(), Failure> {
        let cancel = gio::Cancellable::new();
        let _limit = Deadline::new(&cancel, Instant::now() + PING_LIMIT);
        let message = self.message("GET", "/__down?bytes=0")?;
        let stream = self.answer(&message, &cancel)?;
        Self::read(&stream, &cancel, &mut |_| true)
    }

    fn download(
        &mut self,
        bytes: u64,
        until: Instant,
        more: &mut dyn FnMut(u64) -> bool,
    ) -> Result<(), Failure> {
        let cancel = gio::Cancellable::new();
        let _box = Deadline::new(&cancel, until);
        let message = self.message("GET", &format!("/__down?bytes={bytes}"))?;
        let stream = self.answer(&message, &cancel)?;
        Self::read(&stream, &cancel, more)
    }

    fn upload(
        &mut self,
        bytes: u64,
        until: Instant,
        more: &mut dyn FnMut(u64) -> bool,
    ) -> Result<(), Failure> {
        let cancel = gio::Cancellable::new();
        let _box = Deadline::new(&cancel, until);
        let message = self.message("POST", "/__up")?;
        // Zeros, so no data of the person's leaves the PC. They are taken from the
        // system lazily and never written, so the body costs no memory.
        let body = glib::Bytes::from_owned(vec![0u8; usize::try_from(bytes).unwrap_or(usize::MAX)]);
        message.set_request_body_from_bytes(Some("application/octet-stream"), Some(&body));
        let written = Arc::new(AtomicU64::new(0));
        let counter = written.clone();
        message.connect_wrote_body_data(move |_, chunk| {
            counter.fetch_add(u64::from(chunk), Ordering::Relaxed);
        });
        let sent = self.session.send(&message, Some(&cancel));
        // What went out is told once the send ends, and the box closing is a stop.
        let going = more(written.load(Ordering::Relaxed));
        match sent {
            Err(_) if !going => Ok(()),
            Err(error) => Err(network(error)),
            Ok(stream) => {
                Self::accepted(&message)?;
                Self::read(&stream, &cancel, &mut |_| true)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Mutex;

    /// What the stand-in server does with a request.
    #[derive(Clone, Copy)]
    enum Behaviour {
        /// Answers every request as the speed endpoints do.
        Plain,
        /// Answers every request with this status and nothing else.
        Status(u16),
        /// Starts a download and then goes quiet; reads an upload only this far.
        Stalls(usize),
    }

    struct Server {
        port: u16,
        requests: Arc<Mutex<Vec<String>>>,
    }

    fn serve(behaviour: Behaviour) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let requests: Arc<Mutex<Vec<String>>> = Arc::default();
        let seen = requests.clone();
        std::thread::spawn(move || {
            for connection in listener.incoming().flatten() {
                let seen = seen.clone();
                std::thread::spawn(move || {
                    let _ = handle(connection, behaviour, &seen);
                });
            }
        });
        Server { port, requests }
    }

    fn handle(
        mut connection: TcpStream,
        behaviour: Behaviour,
        seen: &Mutex<Vec<String>>,
    ) -> std::io::Result<()> {
        let mut reader = BufReader::new(connection.try_clone()?);
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 {
                return Ok(());
            }
            let request = line.trim().to_string();
            let mut length = 0usize;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header)?;
                if header.trim().is_empty() {
                    break;
                }
                if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
            seen.lock().unwrap().push(request.clone());
            if let Behaviour::Status(code) = behaviour {
                connection.write_all(
                    format!("HTTP/1.1 {code} Nope\r\nContent-Length: 0\r\n\r\n").as_bytes(),
                )?;
                continue;
            }
            if request.starts_with("POST") {
                let readable = match behaviour {
                    Behaviour::Stalls(limit) => limit.min(length),
                    _ => length,
                };
                let mut buffer = vec![0u8; 16 << 10];
                let mut taken = 0;
                while taken < readable {
                    let want = buffer.len().min(readable - taken);
                    let n = reader.read(&mut buffer[..want])?;
                    if n == 0 {
                        return Ok(());
                    }
                    taken += n;
                }
                if matches!(behaviour, Behaviour::Stalls(_)) {
                    std::thread::sleep(Duration::from_secs(4));
                    return Ok(());
                }
                connection.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")?;
                continue;
            }
            let bytes: usize = request
                .split("bytes=")
                .nth(1)
                .and_then(|n| n.split(' ').next())
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            connection.write_all(
                format!("HTTP/1.1 200 OK\r\nContent-Length: {bytes}\r\n\r\n").as_bytes(),
            )?;
            let chunk = vec![0u8; 16 << 10];
            let mut sent = 0;
            while sent < bytes {
                if matches!(behaviour, Behaviour::Stalls(limit) if sent >= limit) {
                    std::thread::sleep(Duration::from_secs(4));
                    return Ok(());
                }
                let n = chunk.len().min(bytes - sent);
                connection.write_all(&chunk[..n])?;
                sent += n;
            }
        }
    }

    fn link(server: &Server) -> Soup {
        Soup::at(&format!("http://127.0.0.1:{}", server.port))
    }

    fn soon(milliseconds: u64) -> Instant {
        Instant::now() + Duration::from_millis(milliseconds)
    }

    #[test]
    fn a_ping_is_a_download_of_no_bytes_whose_answer_is_read() {
        let server = serve(Behaviour::Plain);
        let mut soup = link(&server);
        soup.ping().unwrap();
        soup.ping().unwrap();
        let requests = server.requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 2);
        assert!(
            requests
                .iter()
                .all(|r| r.starts_with("GET /__down?bytes=0 ")),
            "{requests:?}"
        );
    }

    #[test]
    fn a_download_counts_what_arrived_and_stops_when_told_to() {
        let server = serve(Behaviour::Plain);
        let mut soup = link(&server);
        let mut moved = 0u64;
        soup.download(1_000_000, soon(5000), &mut |n| {
            moved += n;
            true
        })
        .unwrap();
        assert_eq!(moved, 1_000_000);

        let mut taken = 0u64;
        soup.download(50_000_000, soon(5000), &mut |n| {
            taken += n;
            taken < 200_000
        })
        .unwrap();
        assert!((200_000..2_000_000).contains(&taken), "{taken}");
    }

    #[test]
    fn an_answer_that_is_not_a_success_is_never_a_measurement() {
        let server = serve(Behaviour::Status(503));
        let mut soup = link(&server);
        assert_eq!(soup.ping(), Err(Failure::Rejected(503)));
        assert_eq!(
            soup.download(1_000, soon(2000), &mut |_| true),
            Err(Failure::Rejected(503))
        );
        assert_eq!(
            soup.upload(1_000, soon(2000), &mut |_| true),
            Err(Failure::Rejected(503))
        );
    }

    #[test]
    fn a_server_that_is_not_there_is_a_network_failure_with_a_reason() {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let mut soup = Soup::at(&format!("http://127.0.0.1:{port}"));
        match soup.ping() {
            Err(Failure::Network(reason)) => assert!(!reason.is_empty()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_download_that_stalls_ends_with_the_box_and_keeps_what_moved() {
        let server = serve(Behaviour::Stalls(64 << 10));
        let mut soup = link(&server);
        let started = Instant::now();
        let mut moved = 0u64;
        let result = soup.download(10_000_000, soon(400), &mut |n| {
            moved += n;
            true
        });
        assert!(matches!(result, Err(Failure::Network(_))), "{result:?}");
        assert!(moved >= 64 << 10, "{moved}");
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn an_upload_is_answered_and_counts_the_bytes_that_went_out() {
        let server = serve(Behaviour::Plain);
        let mut soup = link(&server);
        let mut moved = 0u64;
        soup.upload(300_000, soon(5000), &mut |n| {
            moved += n;
            true
        })
        .unwrap();
        assert_eq!(moved, 300_000);
        let requests = server.requests.lock().unwrap().clone();
        assert!(requests[0].starts_with("POST /__up "), "{requests:?}");
    }

    #[test]
    fn an_upload_the_server_stops_reading_ends_with_the_box_as_a_stop() {
        let server = serve(Behaviour::Stalls(64 << 10));
        let mut soup = link(&server);
        let until = soon(500);
        let started = Instant::now();
        let mut moved = 0u64;
        let result = soup.upload(50_000_000, until, &mut |n| {
            moved += n;
            Instant::now() < until
        });
        assert_eq!(
            result,
            Ok(()),
            "the box closing mid-body is a stop, not a failure"
        );
        assert!(moved > 0 && moved < 50_000_000, "{moved}");
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_whole_run_goes_through_the_phases_to_numbers() {
        let server = serve(Behaviour::Plain);
        let mut soup = link(&server);
        let mut phases = Vec::new();
        let state = super::super::run(&mut soup, Duration::from_millis(300), &mut |s| {
            phases.push(s.phase);
        });
        assert_eq!(state.phase, super::super::Phase::Done, "{:?}", state.error);
        assert!(state.latency_ms.is_some_and(|ms| ms > 0.0));
        assert!(
            state.download_mbps.is_some_and(|mbps| mbps > 1.0),
            "{:?}",
            state.download_mbps
        );
        assert!(
            state.upload_mbps.is_some_and(|mbps| mbps > 0.0),
            "{:?}",
            state.upload_mbps
        );
        assert_eq!(phases.first(), Some(&super::super::Phase::Latency));
        assert_eq!(phases.last(), Some(&super::super::Phase::Done));
    }

    #[test]
    #[ignore = "uses the network: runs the real test against Cloudflare once, as the island does; OPENBAR_LIVE_SPEEDTEST=1"]
    fn live_the_real_speed_test_gives_a_latency_and_two_speeds() {
        assert_eq!(std::env::var("OPENBAR_LIVE_SPEEDTEST").as_deref(), Ok("1"));
        let mut soup = Soup::open().unwrap();
        let state = super::super::run(&mut soup, Duration::from_secs(3), &mut |s| {
            eprintln!(
                "{:?} {:?} {:?} {:?}",
                s.phase, s.latency_ms, s.download_mbps, s.upload_mbps
            );
        });
        assert_eq!(state.phase, super::super::Phase::Done, "{:?}", state.error);
        assert!(
            state
                .latency_ms
                .is_some_and(|ms| (1.0..2000.0).contains(&ms))
        );
        assert!(state.download_mbps.is_some_and(|mbps| mbps > 0.1));
        assert!(state.upload_mbps.is_some_and(|mbps| mbps > 0.1));
    }
}
