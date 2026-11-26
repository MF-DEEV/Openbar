// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Network traffic by app on Linux, where Windows reads a kernel trace (ETW)
//! through an approved helper: the kernel's socket diagnostics
//! (`NETLINK_SOCK_DIAG`, what `ss -ti` reads) give every TCP connection's
//! bytes received and acknowledged with no privileges, and `/proc/<pid>/fd`
//! tells which process holds each one. Only the person's own processes can be
//! looked into, and UDP keeps no byte counts there, so those are not counted
//! (QUIC in a browser among them); the System page says so.

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

/// Bytes a TCP connection has received and had acknowledged, by socket inode.
pub type Counts = HashMap<u64, (u64, u64)>;

const NETLINK_SOCK_DIAG: i32 = 4;
const SOCK_DIAG_BY_FAMILY: u16 = 20;
const NLM_F_REQUEST: u16 = 1;
const NLM_F_DUMP: u16 = 0x300;
const NLMSG_ERROR: u16 = 2;
const NLMSG_DONE: u16 = 3;
const IPPROTO_TCP: u8 = 6;
/// The `INET_DIAG_INFO` attribute: the connection's `struct tcp_info`.
const INET_DIAG_INFO: u16 = 2;
const TCP_LISTEN: u8 = 10;
/// `struct inet_diag_msg`: family, state, timer, retrans, the socket id (48
/// bytes), expires, rqueue, wqueue, uid, inode.
const DIAG_MSG: usize = 72;
const INODE_AT: usize = 68;
/// `tcpi_bytes_acked` and `tcpi_bytes_received` in `struct tcp_info`
/// (linux/tcp.h, since 4.1 and 4.2).
const BYTES_ACKED_AT: usize = 120;
const BYTES_RECEIVED_AT: usize = 128;

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_ne_bytes(b.get(at..at + 2)?.try_into().ok()?))
}
fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_ne_bytes(b.get(at..at + 4)?.try_into().ok()?))
}
fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_ne_bytes(b.get(at..at + 8)?.try_into().ok()?))
}
const fn align4(n: usize) -> usize {
    (n + 3) & !3
}

/// The request for every TCP socket of `family` with its `tcp_info`.
fn request(family: u8, seq: u32) -> Vec<u8> {
    let mut m = Vec::with_capacity(72);
    m.extend_from_slice(&72u32.to_ne_bytes());
    m.extend_from_slice(&SOCK_DIAG_BY_FAMILY.to_ne_bytes());
    m.extend_from_slice(&(NLM_F_REQUEST | NLM_F_DUMP).to_ne_bytes());
    m.extend_from_slice(&seq.to_ne_bytes());
    m.extend_from_slice(&0u32.to_ne_bytes());
    // struct inet_diag_req_v2
    m.extend_from_slice(&[family, IPPROTO_TCP, 1 << (INET_DIAG_INFO - 1), 0]);
    m.extend_from_slice(&u32::MAX.to_ne_bytes()); // every state
    m.extend_from_slice(&[0u8; 48]); // any socket id
    m
}

/// What one datagram of the dump says: its sockets go into `out`; true once
/// the dump is over (done, or an error).
fn parse(datagram: &[u8], out: &mut Counts) -> bool {
    let mut at = 0;
    while let (Some(len), Some(kind)) = (u32_at(datagram, at), u16_at(datagram, at + 4)) {
        let len = len as usize;
        if len < 16 || at + len > datagram.len() {
            return true;
        }
        if kind == NLMSG_DONE || kind == NLMSG_ERROR {
            return true;
        }
        if kind == SOCK_DIAG_BY_FAMILY {
            socket(&datagram[at + 16..at + len], out);
        }
        at += align4(len);
    }
    false
}

fn socket(msg: &[u8], out: &mut Counts) {
    if msg.len() < DIAG_MSG || msg[1] == TCP_LISTEN {
        return;
    }
    let Some(inode) = u32_at(msg, INODE_AT).filter(|i| *i != 0) else {
        return;
    };
    let mut at = DIAG_MSG;
    while let (Some(len), Some(kind)) = (u16_at(msg, at), u16_at(msg, at + 2)) {
        let len = len as usize;
        if len < 4 || at + len > msg.len() {
            return;
        }
        if kind == INET_DIAG_INFO {
            let info = &msg[at + 4..at + len];
            if let (Some(up), Some(down)) = (
                u64_at(info, BYTES_ACKED_AT),
                u64_at(info, BYTES_RECEIVED_AT),
            ) {
                out.insert(u64::from(inode), (down, up));
            }
            return;
        }
        at += align4(len);
    }
}

/// Every TCP connection's counts now; None where the kernel won't say.
pub fn tcp_counts() -> Option<Counts> {
    // SAFETY: a socket of our own, read into a buffer we own, then closed.
    unsafe {
        let fd = libc::socket(
            libc::AF_NETLINK,
            libc::SOCK_DGRAM | libc::SOCK_CLOEXEC,
            NETLINK_SOCK_DIAG,
        );
        if fd < 0 {
            return None;
        }
        let timeout = libc::timeval {
            tv_sec: 1,
            tv_usec: 0,
        };
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            (&timeout as *const libc::timeval).cast(),
            std::mem::size_of::<libc::timeval>() as libc::socklen_t,
        );
        let mut out = Counts::new();
        let mut ok = true;
        let mut buffer = vec![0u8; 64 * 1024];
        for (seq, family) in [(1, libc::AF_INET as u8), (2, libc::AF_INET6 as u8)] {
            let message = request(family, seq);
            if libc::send(fd, message.as_ptr().cast(), message.len(), 0) < 0 {
                ok = false;
                break;
            }
            loop {
                let n = libc::recv(fd, buffer.as_mut_ptr().cast(), buffer.len(), 0);
                if n <= 0 {
                    ok = false;
                    break;
                }
                if parse(&buffer[..n as usize], &mut out) {
                    break;
                }
            }
        }
        libc::close(fd);
        ok.then_some(out)
    }
}

/// The process holding each socket, from the fds of the processes we may read.
pub fn socket_owners(proc: &Path) -> HashMap<u64, u32> {
    let mut owners = HashMap::new();
    let Ok(entries) = std::fs::read_dir(proc) else {
        return owners;
    };
    for entry in entries.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(fds) = std::fs::read_dir(entry.path().join("fd")) else {
            continue;
        };
        for fd in fds.flatten() {
            if let Some(inode) = std::fs::read_link(fd.path())
                .ok()
                .and_then(|target| socket_inode(&target.to_string_lossy()))
            {
                owners.entry(inode).or_insert(pid);
            }
        }
    }
    owners
}

fn socket_inode(link: &str) -> Option<u64> {
    link.strip_prefix("socket:[")?
        .strip_suffix(']')?
        .parse()
        .ok()
}

/// A reading of the traffic by process.
#[derive(Debug, PartialEq)]
pub enum Sample {
    /// The first reading: the baseline the next one counts from.
    Measuring,
    /// The kernel won't say (no sock_diag).
    Unavailable,
    /// Bytes per second down and up for each pid since the last reading.
    Ready(HashMap<u32, (f64, f64)>),
}

#[derive(Default)]
pub struct Rates {
    last: Option<(Instant, Counts)>,
}

impl Rates {
    pub fn sample(&mut self, proc: &Path) -> Sample {
        let now = Instant::now();
        let Some(counts) = tcp_counts() else {
            self.last = None;
            return Sample::Unavailable;
        };
        let Some((then, before)) = self.last.replace((now, counts)) else {
            return Sample::Measuring;
        };
        let after = &self.last.as_ref().expect("just set").1;
        let seconds = now.duration_since(then).as_secs_f64();
        Sample::Ready(per_process(&before, after, &socket_owners(proc), seconds))
    }
}

/// What each pid's connections moved between two readings, per second. A
/// connection opened since counts from zero; one whose counts went back (its
/// inode reused) counts from its new start.
fn per_process(
    before: &Counts,
    after: &Counts,
    owners: &HashMap<u64, u32>,
    seconds: f64,
) -> HashMap<u32, (f64, f64)> {
    let mut rates: HashMap<u32, (f64, f64)> = HashMap::new();
    if seconds <= 0.0 {
        return rates;
    }
    for (inode, &(down, up)) in after {
        let Some(&pid) = owners.get(inode) else {
            continue;
        };
        let (was_down, was_up) = before.get(inode).copied().unwrap_or((0, 0));
        let delta = |now: u64, was: u64| if now >= was { now - was } else { now };
        let (d, u) = (delta(down, was_down), delta(up, was_up));
        if d == 0 && u == 0 {
            continue;
        }
        let slot = rates.entry(pid).or_default();
        slot.0 += d as f64 / seconds;
        slot.1 += u as f64 / seconds;
    }
    rates
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A datagram of the dump: one socket with its `tcp_info`, then the end.
    fn datagram(state: u8, inode: u32, received: u64, acked: u64) -> Vec<u8> {
        let mut msg = vec![0u8; DIAG_MSG];
        msg[0] = libc::AF_INET as u8;
        msg[1] = state;
        msg[INODE_AT..INODE_AT + 4].copy_from_slice(&inode.to_ne_bytes());
        let mut info = vec![0u8; 232];
        info[BYTES_ACKED_AT..BYTES_ACKED_AT + 8].copy_from_slice(&acked.to_ne_bytes());
        info[BYTES_RECEIVED_AT..BYTES_RECEIVED_AT + 8].copy_from_slice(&received.to_ne_bytes());
        // Another attribute first, as the kernel may send (INET_DIAG_SHUTDOWN).
        msg.extend_from_slice(&5u16.to_ne_bytes());
        msg.extend_from_slice(&8u16.to_ne_bytes());
        msg.extend_from_slice(&[0, 0, 0, 0]);
        msg.extend_from_slice(&((info.len() + 4) as u16).to_ne_bytes());
        msg.extend_from_slice(&INET_DIAG_INFO.to_ne_bytes());
        msg.extend_from_slice(&info);
        let mut out = Vec::new();
        out.extend_from_slice(&((16 + msg.len()) as u32).to_ne_bytes());
        out.extend_from_slice(&SOCK_DIAG_BY_FAMILY.to_ne_bytes());
        out.extend_from_slice(&[0u8; 10]);
        out.extend_from_slice(&msg);
        out
    }

    fn done() -> Vec<u8> {
        let mut out = 20u32.to_ne_bytes().to_vec();
        out.extend_from_slice(&NLMSG_DONE.to_ne_bytes());
        out.extend_from_slice(&[0u8; 14]);
        out
    }

    #[test]
    fn a_connections_bytes_come_from_its_tcp_info() {
        let mut out = Counts::new();
        let mut both = datagram(1, 77, 5_000, 300);
        both.extend(datagram(TCP_LISTEN, 78, 9, 9));
        assert!(!parse(&both, &mut out));
        assert_eq!(out, Counts::from([(77, (5_000, 300))]));
        assert!(parse(&done(), &mut out));
        // A cut message ends the dump rather than reading past it.
        assert!(parse(&datagram(1, 79, 1, 1)[..40], &mut out));
    }

    #[test]
    fn rates_are_per_process_and_a_new_connection_counts_from_zero() {
        let before = Counts::from([(1, (1_000, 100)), (2, (50, 50))]);
        let after = Counts::from([(1, (3_000, 300)), (2, (50, 50)), (3, (400, 0)), (4, (9, 9))]);
        let owners = HashMap::from([(1, 10), (2, 10), (3, 20)]);
        let rates = per_process(&before, &after, &owners, 2.0);
        assert_eq!(rates.get(&10), Some(&(1_000.0, 100.0)));
        assert_eq!(rates.get(&20), Some(&(200.0, 0.0)));
        // Nobody we may look into holds socket 4.
        assert_eq!(rates.len(), 2);
        // An inode reused by a fresh connection.
        let rates = per_process(&after, &Counts::from([(1, (10, 0))]), &owners, 1.0);
        assert_eq!(rates.get(&10), Some(&(10.0, 0.0)));
    }

    #[test]
    fn a_socket_fd_names_its_inode() {
        assert_eq!(socket_inode("socket:[123456]"), Some(123_456));
        assert_eq!(socket_inode("pipe:[5]"), None);
        assert_eq!(socket_inode("/dev/null"), None);
    }

    #[test]
    #[ignore = "reads this machine's connections"]
    fn live_tcp_counts() {
        let counts = tcp_counts().expect("sock_diag");
        let owners = socket_owners(Path::new("/proc"));
        let mine = counts.keys().filter(|i| owners.contains_key(i)).count();
        println!(
            "{} connections, {mine} of this user's processes",
            counts.len()
        );
    }
}
