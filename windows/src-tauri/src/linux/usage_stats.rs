// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The usage ping's one POST on Linux, through libsoup like the price list
//! (`linux/prices.rs`): no cookies, no cache, no redirects, 15 s per step and
//! a minute in all. `usage_stats/mod.rs` includes this file as its `http`.

use std::time::{Duration, Instant};

use gtk::gio;
use gtk::glib;
use soup::prelude::*;

use super::Target;
use crate::linux::net::Deadline;

const STEP: u32 = 15;
const WHOLE: Duration = Duration::from_secs(60);

pub fn post(target: &Target, agent: &str, body: &[u8]) -> Option<u16> {
    let scheme = if target.secure { "https" } else { "http" };
    let url = format!("{scheme}://{}:{}{}", target.host, target.port, target.path);
    let session = soup::Session::builder()
        .timeout(STEP)
        .idle_timeout(STEP)
        .user_agent(agent)
        .build();
    let message = soup::Message::new("POST", &url).ok()?;
    message.set_flags(soup::MessageFlags::NO_REDIRECT);
    if let Some(headers) = message.request_headers() {
        headers.append("Cache-Control", "no-store");
    }
    let bytes = glib::Bytes::from(body);
    message.set_request_body_from_bytes(Some("application/json"), Some(&bytes));
    let cancel = gio::Cancellable::new();
    let _whole = Deadline::new(&cancel, Instant::now() + WHOLE);
    session.send_and_read(&message, Some(&cancel)).ok()?;
    u16::try_from(message.status_code()).ok().filter(|s| *s > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    /// A stand-in dashboard: answers `status` and hands back what it was sent.
    fn serve(status: &'static str) -> (u16, mpsc::Receiver<(String, Vec<u8>)>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for mut connection in listener.incoming().flatten() {
                let mut reader = BufReader::new(connection.try_clone().unwrap());
                let mut head = String::new();
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                    head.push_str(&line);
                    if line.trim().is_empty() {
                        break;
                    }
                }
                let mut body = vec![0u8; length];
                let _ = reader.read_exact(&mut body);
                let _ = tx.send((head, body));
                let _ = connection.write_all(
                    format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .as_bytes(),
                );
            }
        });
        (port, rx)
    }

    fn local(port: u16) -> Target {
        Target {
            secure: false,
            host: "127.0.0.1".into(),
            port,
            path: "/api/v1/ping".into(),
        }
    }

    #[test]
    fn posts_the_json_body_and_reads_the_status() {
        let (port, seen) = serve("204 No Content");
        let status = post(&local(port), "Openbar/1.1.0", br#"{"v":1}"#);
        assert_eq!(status, Some(204));
        let (head, body) = seen.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(head.starts_with("POST /api/v1/ping HTTP/1.1"), "{head}");
        let lower = head.to_ascii_lowercase();
        assert!(lower.contains("content-type: application/json"), "{head}");
        assert!(lower.contains("user-agent: openbar/1.1.0"), "{head}");
        assert!(!lower.contains("cookie"), "{head}");
        assert_eq!(body, br#"{"v":1}"#);
    }

    #[test]
    fn a_redirect_is_not_followed() {
        let (port, seen) =
            serve("307 Temporary Redirect\r\nLocation: http://127.0.0.1:9/elsewhere");
        assert_eq!(post(&local(port), "Openbar/1.1.0", b"{}"), Some(307));
        seen.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            seen.recv_timeout(Duration::from_millis(300)).is_err(),
            "one request only"
        );
    }

    #[test]
    fn nothing_listening_is_a_failure() {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        assert_eq!(post(&local(port), "Openbar/1.1.0", b"{}"), None);
    }
}
