// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! An owned, real-time SystemTraceProvider session. The app never requests
//! elevation, takes over the global kernel logger or writes a disk trace.
//! Starting a system logger still requires Windows tracing privileges; callers
//! must expose access denied rather than treating it as zero traffic.

use super::ProcessStatus;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;
use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_PRIVILEGE_NOT_HELD, GetLastError, WIN32_ERROR,
};
use windows::Win32::System::Diagnostics::Etw::*;
use windows::core::{GUID, PCWSTR, PWSTR};

const TCP: GUID = GUID::from_u128(0x9a280ac0_c8e0_11d1_84e2_00c04fb998a2);
const UDP: GUID = GUID::from_u128(0xbf3a50c5_a9c9_4988_a005_2df0b7c80f80);

#[derive(Default)]
pub(super) struct Bytes {
    pub down: u64,
    pub up: u64,
    /// FILETIME of the earliest event, to reject traffic from a recycled PID.
    pub first: i64,
}

#[derive(Default)]
struct Traffic {
    bytes: HashMap<u32, Bytes>,
    buffers: u64,
    ended: bool,
    overflow: bool,
    unsupported: bool,
}

impl Traffic {
    fn take_sample(
        &mut self,
        queried: bool,
        lost: u64,
        previous_lost: u64,
    ) -> (ProcessStatus, HashMap<u32, Bytes>) {
        let overflow = std::mem::take(&mut self.overflow);
        let status = if !queried || self.ended || self.unsupported {
            ProcessStatus::Unavailable
        } else if lost != previous_lost || overflow {
            ProcessStatus::Incomplete
        } else if self.buffers == 0 {
            ProcessStatus::Measuring
        } else {
            ProcessStatus::Ready
        };
        if status == ProcessStatus::Measuring {
            // A callback may be partway through its first buffer. Keep those
            // bytes and the startup interval until that buffer is complete.
            return (status, HashMap::new());
        }
        let mut bytes = std::mem::take(&mut self.bytes);
        if status != ProcessStatus::Ready {
            bytes.clear();
        }
        (status, bytes)
    }
}

// Aligned storage, with room for the session name as required by StartTrace.
#[repr(C)]
struct Properties {
    header: EVENT_TRACE_PROPERTIES,
    name: [u16; 128],
}

impl Properties {
    fn new() -> Self {
        let mut value = Self {
            header: EVENT_TRACE_PROPERTIES::default(),
            name: [0; 128],
        };
        value.header.Wnode.BufferSize = std::mem::size_of::<Self>() as u32;
        value.header.Wnode.Flags = WNODE_FLAG_TRACED_GUID;
        // QPC clock; ProcessTrace converts timestamps to FILETIME for consumers.
        value.header.Wnode.ClientContext = 1;
        value.header.LoggerNameOffset = std::mem::offset_of!(Self, name) as u32;
        value
    }
}

pub(super) struct NetworkTrace {
    session: CONTROLTRACE_HANDLE,
    consumer: PROCESSTRACE_HANDLE,
    worker: Option<JoinHandle<()>>,
    traffic: Arc<Mutex<Traffic>>,
    last: Instant,
    lost: u64,
    _name: Vec<u16>,
}

fn failure(status: WIN32_ERROR) -> ProcessStatus {
    if status == ERROR_ACCESS_DENIED || status == ERROR_PRIVILEGE_NOT_HELD {
        ProcessStatus::PermissionDenied
    } else {
        ProcessStatus::Unavailable
    }
}

impl NetworkTrace {
    pub fn start() -> Result<Self, ProcessStatus> {
        let mut properties = Properties::new();
        properties.header.LogFileMode = EVENT_TRACE_REAL_TIME_MODE | EVENT_TRACE_SYSTEM_LOGGER_MODE;
        properties.header.EnableFlags = EVENT_TRACE_FLAG_NETWORK_TCPIP;
        properties.header.BufferSize = 64;
        properties.header.MinimumBuffers = 4;
        properties.header.MaximumBuffers = 32;
        properties.header.FlushTimer = 1;
        // Let ETW assign a unique session GUID. A unique name also avoids
        // attaching to or stopping another application's trace session.
        let mut name: Vec<u16> = format!("Vorssaint-Network-{}-{}", std::process::id(), unsafe {
            windows::Win32::System::SystemInformation::GetTickCount64()
        })
        .encode_utf16()
        .chain(Some(0))
        .collect();
        let mut session = CONTROLTRACE_HANDLE::default();
        let status =
            unsafe { StartTraceW(&mut session, PCWSTR(name.as_ptr()), &mut properties.header) };
        if status.0 != 0 {
            return Err(failure(status));
        }
        let traffic = Arc::new(Mutex::new(Traffic::default()));
        let mut logfile = EVENT_TRACE_LOGFILEW {
            LoggerName: PWSTR(name.as_mut_ptr()),
            Context: Arc::as_ptr(&traffic) as *mut _,
            BufferCallback: Some(buffer_callback),
            ..Default::default()
        };
        logfile.Anonymous1.ProcessTraceMode =
            PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
        logfile.Anonymous2.EventRecordCallback = Some(event_callback);
        let consumer = unsafe { OpenTraceW(&mut logfile) };
        if consumer.Value == u64::MAX {
            let error = failure(unsafe { GetLastError() });
            let _ = unsafe {
                ControlTraceW(
                    session,
                    PCWSTR::null(),
                    &mut properties.header,
                    EVENT_TRACE_CONTROL_STOP,
                )
            };
            return Err(error);
        }
        let context = traffic.clone();
        let worker = std::thread::Builder::new()
            .name("network-etw".into())
            .spawn(move || {
                // This Arc keeps the callback context alive until ProcessTrace ends.
                let _ = unsafe { ProcessTrace(&[consumer], None, None) };
                if let Ok(mut traffic) = context.lock() {
                    traffic.ended = true;
                }
            });
        let mut trace = Self {
            session,
            consumer,
            worker: None,
            traffic,
            last: Instant::now(),
            lost: 0,
            _name: name,
        };
        match worker {
            Ok(worker) => trace.worker = Some(worker),
            Err(_) => return Err(ProcessStatus::Unavailable), // Drop closes both handles.
        }
        Ok(trace)
    }

    pub fn sample(&mut self) -> (ProcessStatus, HashMap<u32, Bytes>, f64) {
        let elapsed = self.last.elapsed().as_secs_f64();
        let mut properties = Properties::new();
        let queried = unsafe {
            ControlTraceW(
                self.session,
                PCWSTR::null(),
                &mut properties.header,
                EVENT_TRACE_CONTROL_QUERY,
            )
        }
        .0 == 0;
        let lost = u64::from(properties.header.EventsLost)
            + u64::from(properties.header.RealTimeBuffersLost);
        let mut traffic = self.traffic.lock().unwrap_or_else(|e| e.into_inner());
        let (status, bytes) = traffic.take_sample(queried, lost, self.lost);
        if status != ProcessStatus::Measuring {
            self.last = Instant::now();
        }
        self.lost = lost;
        (status, bytes, elapsed)
    }
}

impl Drop for NetworkTrace {
    fn drop(&mut self) {
        let mut properties = Properties::new();
        unsafe {
            let _ = ControlTraceW(
                self.session,
                PCWSTR::null(),
                &mut properties.header,
                EVENT_TRACE_CONTROL_STOP,
            );
            let _ = CloseTrace(self.consumer);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// The documented v2 TCP/UDP IPv4/IPv6 send/receive schemas all begin
/// with uint32 PID, uint32 size. Header.ProcessId is NOT the originating PID.
fn packet(provider: GUID, version: u8, opcode: u8, data: &[u8]) -> Option<(u32, u32, bool)> {
    if (provider != TCP && provider != UDP) || version != 2 || data.len() < 8 {
        return None;
    }
    let receive = match opcode {
        10 | 26 => false,
        11 | 27 => true,
        _ => return None, // No retransmits, connects or other double counting.
    };
    let pid = u32::from_le_bytes(data[..4].try_into().ok()?);
    let size = u32::from_le_bytes(data[4..8].try_into().ok()?);
    (pid != 0 && size != 0).then_some((pid, size, receive))
}

unsafe extern "system" fn event_callback(record: *mut EVENT_RECORD) {
    let Some(record) = (unsafe { record.as_ref() }) else {
        return;
    };
    if record.UserContext.is_null() || record.UserData.is_null() {
        return;
    }
    let data = unsafe {
        std::slice::from_raw_parts(record.UserData.cast(), record.UserDataLength as usize)
    };
    let header = &record.EventHeader;
    if (header.ProviderId == TCP || header.ProviderId == UDP)
        && matches!(header.EventDescriptor.Opcode, 10 | 11 | 26 | 27)
        && (header.EventDescriptor.Version != 2 || data.len() < 8)
    {
        let context = unsafe { &*record.UserContext.cast::<Mutex<Traffic>>() };
        if let Ok(mut traffic) = context.lock() {
            traffic.unsupported = true;
        }
        return;
    }
    let Some((pid, size, receive)) = packet(
        header.ProviderId,
        header.EventDescriptor.Version,
        header.EventDescriptor.Opcode,
        data,
    ) else {
        return;
    };
    let context = unsafe { &*record.UserContext.cast::<Mutex<Traffic>>() };
    let Ok(mut traffic) = context.lock() else {
        return;
    };
    // Bound memory even under rapid process churn between UI samples.
    if traffic.bytes.len() >= 16_384 && !traffic.bytes.contains_key(&pid) {
        traffic.overflow = true;
        return;
    }
    let bytes = traffic.bytes.entry(pid).or_insert_with(|| Bytes {
        first: header.TimeStamp,
        ..Default::default()
    });
    bytes.first = bytes.first.min(header.TimeStamp);
    if receive {
        bytes.down = bytes.down.saturating_add(u64::from(size));
    } else {
        bytes.up = bytes.up.saturating_add(u64::from(size));
    }
}

unsafe extern "system" fn buffer_callback(logfile: *mut EVENT_TRACE_LOGFILEW) -> u32 {
    if let Some(logfile) = unsafe { logfile.as_ref() }
        && !logfile.Context.is_null()
    {
        let context = unsafe { &*logfile.Context.cast::<Mutex<Traffic>>() };
        if let Ok(mut traffic) = context.lock() {
            traffic.buffers += 1;
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_only_documented_packet_events_and_payload_pid() {
        let data = [42, 0, 0, 0, 0, 4, 0, 0];
        for provider in [TCP, UDP] {
            for opcode in [10, 26] {
                assert_eq!(packet(provider, 2, opcode, &data), Some((42, 1024, false)));
            }
            for opcode in [11, 27] {
                assert_eq!(packet(provider, 2, opcode, &data), Some((42, 1024, true)));
            }
        }
        assert_eq!(packet(TCP, 2, 14, &data), None);
        assert_eq!(packet(TCP, 1, 10, &data), None);
        assert_eq!(packet(GUID::zeroed(), 2, 10, &data), None);
        assert_eq!(packet(TCP, 2, 10, &data[..7]), None);
        assert_eq!(packet(TCP, 2, 10, &[0; 8]), None);
    }

    #[test]
    fn callbacks_accumulate_directions_using_payload_pid_and_oldest_timestamp() {
        let context = Mutex::new(Traffic::default());
        let mut data: [u8; 8] = [42, 0, 0, 0, 0, 4, 0, 0];
        let mut record = EVENT_RECORD {
            UserContext: (&context as *const Mutex<Traffic>).cast_mut().cast(),
            UserData: data.as_mut_ptr().cast(),
            UserDataLength: data.len() as u16,
            ..Default::default()
        };
        record.EventHeader.ProviderId = TCP;
        record.EventHeader.ProcessId = 999; // Often a system thread, not the app.
        record.EventHeader.EventDescriptor.Version = 2;
        record.EventHeader.EventDescriptor.Opcode = 10;
        record.EventHeader.TimeStamp = 200;
        unsafe { event_callback(&mut record) };
        record.EventHeader.ProviderId = UDP;
        record.EventHeader.EventDescriptor.Opcode = 27;
        record.EventHeader.TimeStamp = 100; // ETW can deliver out of order.
        unsafe { event_callback(&mut record) };
        let traffic = context.lock().unwrap();
        assert_eq!(traffic.bytes.len(), 1);
        let bytes = &traffic.bytes[&42];
        assert_eq!((bytes.down, bytes.up, bytes.first), (1024, 1024, 100));
    }

    #[test]
    fn permissions_are_distinct_from_other_etw_failures() {
        assert_eq!(
            failure(ERROR_ACCESS_DENIED),
            ProcessStatus::PermissionDenied
        );
        assert_eq!(
            failure(ERROR_PRIVILEGE_NOT_HELD),
            ProcessStatus::PermissionDenied
        );
        assert_eq!(failure(WIN32_ERROR(87)), ProcessStatus::Unavailable);
    }

    #[test]
    fn incomplete_intervals_are_discarded_and_can_recover() {
        let mut traffic = Traffic::default();
        traffic.bytes.insert(
            42,
            Bytes {
                down: 512,
                ..Default::default()
            },
        );
        assert_eq!(traffic.take_sample(true, 0, 0).0, ProcessStatus::Measuring);
        assert_eq!(traffic.bytes[&42].down, 512);
        traffic.buffers = 1;
        traffic.bytes.insert(
            42,
            Bytes {
                down: 1024,
                ..Default::default()
            },
        );
        let (status, bytes) = traffic.take_sample(true, 1, 0);
        assert_eq!(status, ProcessStatus::Incomplete);
        assert!(bytes.is_empty());
        assert!(traffic.bytes.is_empty());
        assert_eq!(traffic.take_sample(true, 1, 1).0, ProcessStatus::Ready);
        traffic.overflow = true;
        assert_eq!(traffic.take_sample(true, 1, 1).0, ProcessStatus::Incomplete);
        assert_eq!(traffic.take_sample(true, 1, 1).0, ProcessStatus::Ready);
        assert_eq!(
            traffic.take_sample(false, 1, 1).0,
            ProcessStatus::Unavailable
        );
        traffic.unsupported = true;
        assert_eq!(
            traffic.take_sample(true, 1, 1).0,
            ProcessStatus::Unavailable
        );
    }

    #[test]
    #[ignore = "starts a real system logger; requires Windows tracing privileges"]
    fn live_trace_stops_its_owned_session() {
        let mut trace = NetworkTrace::start().unwrap_or_else(|status| {
            panic!("ETW could not start: {status:?}; run this test with tracing privileges")
        });
        // Local traffic requires neither internet nor an external server.
        for address in ["127.0.0.1:0", "[::1]:0"] {
            for tcp in [false, true] {
                let transferred = if tcp {
                    use std::io::{Read, Write};
                    let listener = std::net::TcpListener::bind(address).unwrap();
                    let mut sender =
                        std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
                    let (mut receiver, _) = listener.accept().unwrap();
                    receiver
                        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                        .unwrap();
                    sender.write_all(&[0; 16_384]).unwrap();
                    receiver.read_exact(&mut [0; 16_384]).unwrap();
                    16_384
                } else {
                    let receiver = std::net::UdpSocket::bind(address).unwrap();
                    receiver
                        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                        .unwrap();
                    let sender = std::net::UdpSocket::bind(address).unwrap();
                    sender
                        .send_to(&[0; 1024], receiver.local_addr().unwrap())
                        .unwrap();
                    receiver.recv_from(&mut [0; 2048]).unwrap();
                    1024
                };
                std::thread::sleep(std::time::Duration::from_secs(3));
                let (status, bytes, elapsed) = trace.sample();
                assert_eq!(status, ProcessStatus::Ready);
                let own = bytes.get(&std::process::id()).expect("own network traffic");
                eprintln!(
                    "{} {address}: PID {}, down {}, up {}, interval {elapsed:.2}s",
                    if tcp { "TCP" } else { "UDP" },
                    std::process::id(),
                    own.down,
                    own.up
                );
                assert!(own.down >= transferred && own.up >= transferred);
                assert!(elapsed > 0.0 && elapsed.is_finite());
            }
        }
        let name = trace._name.clone();
        drop(trace);
        let mut properties = Properties::new();
        let status = unsafe {
            ControlTraceW(
                CONTROLTRACE_HANDLE::default(),
                PCWSTR(name.as_ptr()),
                &mut properties.header,
                EVENT_TRACE_CONTROL_QUERY,
            )
        };
        assert_eq!(
            status,
            windows::Win32::Foundation::ERROR_WMI_INSTANCE_NOT_FOUND
        );
        eprintln!("Owned ETW session removed after collector shutdown");
    }
}
