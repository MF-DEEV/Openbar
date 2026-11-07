// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The daily price list on Linux, where Windows downloads it through WinHTTP:
//! one HTTPS GET of the same public file, through libsoup, the web view's own
//! network stack (so the system's proxy and TLS settings apply). No cookies,
//! no cache, 15 s per step, `MAXIMUM_SIZE` at most, status 200 only and no
//! redirects. `agent_usage/prices.rs` includes this file as its `imp`.

use std::time::{Duration, Instant};

use gtk::gio::{self, prelude::*};
use soup::prelude::*;

use super::super::pricing::MAXIMUM_SIZE;
use super::{HOST, PATH};
use crate::linux::net::Deadline;

/// Each step of the request may take this long, as on Windows.
const STEP: u32 = 15;
/// A server that sends a byte every few seconds does not keep the download open for ever.
const WHOLE: Duration = Duration::from_secs(60);

pub fn get() -> Option<Vec<u8>> {
    fetch(&format!("https://{HOST}{PATH}"), STEP, WHOLE)
}

/// One GET of `url`; None when any step fails or the answer is not a plain 200 of a list of a sensible size.
fn fetch(url: &str, step: u32, whole: Duration) -> Option<Vec<u8>> {
    let session = soup::Session::builder()
        .timeout(step)
        .idle_timeout(step)
        .user_agent(concat!("Openbar/", env!("CARGO_PKG_VERSION")))
        .build();
    let message = soup::Message::new("GET", url).ok()?;
    message.set_flags(soup::MessageFlags::NO_REDIRECT);
    if let Some(headers) = message.request_headers() {
        headers.append("Cache-Control", "no-cache");
    }
    let cancel = gio::Cancellable::new();
    let _whole = Deadline::new(&cancel, Instant::now() + whole);
    let stream = session.send(&message, Some(&cancel)).ok()?;
    if message.status() != soup::Status::Ok {
        return None;
    }
    let mut body = Vec::new();
    loop {
        let chunk = stream.read_bytes(16 << 10, Some(&cancel)).ok()?;
        if chunk.is_empty() {
            return Some(body);
        }
        body.extend_from_slice(&chunk);
        if body.len() > MAXIMUM_SIZE {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    enum Answer {
        Body(Vec<u8>),
        Status(u16),
        Redirect(String),
        /// Sets a cookie with a good body.
        Cookie(Vec<u8>),
        /// Promises a body, sends a little of it and goes quiet.
        Stalls,
    }

    /// The requests a stand-in server was sent, whole, headers included.
    type Seen = Arc<Mutex<Vec<String>>>;

    fn serve(answer: Answer) -> (u16, Seen) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen: Seen = Arc::default();
        let log = seen.clone();
        std::thread::spawn(move || {
            for connection in listener.incoming().flatten() {
                let (answer, log) = (answer.clone(), log.clone());
                std::thread::spawn(move || {
                    let _ = respond(connection, &answer, &log);
                });
            }
        });
        (port, seen)
    }

    fn respond(mut connection: TcpStream, answer: &Answer, log: &Seen) -> std::io::Result<()> {
        let mut reader = BufReader::new(connection.try_clone()?);
        let mut request = String::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 {
                return Ok(());
            }
            request.push_str(&line);
            if line.trim().is_empty() {
                break;
            }
        }
        log.lock().unwrap().push(request);
        let head = |status: &str, extra: &str, length: usize| {
            format!(
                "HTTP/1.1 {status}\r\n{extra}Content-Length: {length}\r\nConnection: close\r\n\r\n"
            )
        };
        match answer {
            Answer::Body(body) => {
                connection.write_all(head("200 OK", "", body.len()).as_bytes())?;
                connection.write_all(body)
            }
            Answer::Status(code) => {
                connection.write_all(head(&format!("{code} Nope"), "", 0).as_bytes())
            }
            Answer::Redirect(to) => connection
                .write_all(head("302 Found", &format!("Location: {to}\r\n"), 0).as_bytes()),
            Answer::Cookie(body) => {
                connection.write_all(
                    head("200 OK", "Set-Cookie: session=abc\r\n", body.len()).as_bytes(),
                )?;
                connection.write_all(body)
            }
            Answer::Stalls => {
                connection.write_all(head("200 OK", "", 1000).as_bytes())?;
                connection.write_all(b"{\"prices\":")?;
                std::thread::sleep(Duration::from_secs(5));
                Ok(())
            }
        }
    }

    fn url(port: u16) -> String {
        format!("http://127.0.0.1:{port}/agent-prices.json")
    }

    fn quick(url: &str) -> Option<Vec<u8>> {
        fetch(url, 2, Duration::from_secs(4))
    }

    #[test]
    fn a_plain_200_gives_its_body() {
        let (port, _) = serve(Answer::Body(b"{\"models\":[]}".to_vec()));
        assert_eq!(quick(&url(port)), Some(b"{\"models\":[]}".to_vec()));
    }

    #[test]
    fn any_other_answer_is_no_list() {
        for code in [201, 204, 404, 500, 503] {
            let (port, _) = serve(Answer::Status(code));
            assert_eq!(quick(&url(port)), None, "{code}");
        }
    }

    #[test]
    fn a_redirect_is_not_followed() {
        let (elsewhere, followed) = serve(Answer::Body(b"{}".to_vec()));
        let (port, _) = serve(Answer::Redirect(url(elsewhere)));
        assert_eq!(quick(&url(port)), None);
        assert!(
            followed.lock().unwrap().is_empty(),
            "the other server was never asked"
        );
    }

    #[test]
    fn a_list_over_the_limit_is_refused_and_one_at_it_is_kept() {
        let (port, _) = serve(Answer::Body(vec![b' '; MAXIMUM_SIZE + 1024]));
        assert_eq!(quick(&url(port)), None);
        let (port, _) = serve(Answer::Body(vec![b' '; MAXIMUM_SIZE]));
        assert_eq!(quick(&url(port)).map(|body| body.len()), Some(MAXIMUM_SIZE));
    }

    #[test]
    fn nothing_listening_is_no_list() {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        assert_eq!(quick(&url(port)), None);
    }

    #[test]
    fn a_server_that_goes_quiet_is_given_up_on() {
        let (port, _) = serve(Answer::Stalls);
        let started = Instant::now();
        assert_eq!(fetch(&url(port), 1, Duration::from_secs(3)), None);
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn nothing_is_cached_no_cookie_is_kept_and_the_agent_says_who_asks() {
        let (port, seen) = serve(Answer::Cookie(b"{}".to_vec()));
        assert!(quick(&url(port)).is_some());
        assert!(quick(&url(port)).is_some());
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 2);
        for request in &seen {
            let lower = request.to_ascii_lowercase();
            assert!(lower.contains("cache-control: no-cache"), "{request}");
            assert!(lower.contains("user-agent: openbar/"), "{request}");
        }
        // Each call has a session of its own, and none keeps a cookie jar.
        assert!(
            seen.iter()
                .all(|r| !r.to_ascii_lowercase().contains("cookie:")),
            "{seen:?}"
        );
    }

    #[test]
    #[ignore = "uses the network: fetches the real list once; OPENBAR_LIVE_PRICES=1"]
    fn live_the_real_list_is_fetched_and_understood() {
        assert_eq!(std::env::var("OPENBAR_LIVE_PRICES").as_deref(), Ok("1"));
        let bytes = get().expect("the list from GitHub");
        assert!(super::super::super::pricing::PriceList::decode(&bytes).is_some());
        eprintln!("{} bytes", bytes.len());
    }
}
