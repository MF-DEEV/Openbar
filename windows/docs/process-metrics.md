# Windows process metrics

The System page keeps the existing `system_monitor.rs` sampler thread and
`system://snapshot` bridge. Only the open detail requests process rows. The UI
uses the same app names, shell icons, ten-row limit and descending ranking as
the CPU and memory details. As in the macOS `MetricDetailView` and
`ProcessUsageService`, processes with the same executable image are grouped.
CPU and memory retain their existing measurement paths.

## GPU

The existing English PDH wildcard counter
`\GPU Engine(*)\Utilization Percentage` supplies both the GPU card and its
process detail. Instance names carry a PID, adapter LUID, physical adapter and
engine index. Contributions from processes in one app are summed per physical
engine; the app's busiest engine gives its utilization, capped at 100%.
Different adapters and independent engines are not added together. The GPU card
uses the same busiest-engine rule across all processes.

The sampler requires two PDH readings and matches processes by PID **and creation
time** across samples. New/recycled PIDs wait for another sample; terminated
processes are omitted. Invalid PDH statuses and non-finite values are ignored.
Missing counters or unsupported drivers cannot produce process measurements.
Protected processes can still use their executable image as a label if Windows
does not allow querying their path or icon.

## Network

`network_process.rs` owns a real-time ETW SystemTraceProvider session with
`EVENT_TRACE_FLAG_NETWORK_TCPIP`. It starts only for the network detail and stops
on leaving it, closing the System page or normal application shutdown. It uses
a unique session name and an ETW-assigned GUID, never attaches to another
session and never stops the global NT Kernel Logger. It adds only a feature
flag to the existing `windows` dependency.

The documented version 2 TCP/UDP IPv4/IPv6 send and receive records contain a
payload PID and packet size. The payload PID identifies the originating process;
the event header PID can identify a system thread instead. Retransmit, connect
and copy events are excluded to avoid counting them as new app traffic.
Per-app receive and send bytes are divided by the elapsed sampling interval;
their sum ranks the rows. Event timestamps are checked against process creation
times to discard traffic belonging to recycled PIDs. Traffic for processes that
have exited is omitted.

Limitations:

- System ETW loggers require Windows tracing privileges, commonly an elevated
  token or privileges granted by policy. Ordinary desktop accounts may be denied.
  The app does not request elevation or change privileges. Access denied and
  missing privileges have a separate UI message; other failures show unavailable.
- ETW buffers events (configured for a one-second flush), so rates can lag and
  fluctuate across adjacent UI intervals. Initial buffer delivery shows measuring.
- These are TCP/UDP event byte rates, including virtual/loopback traffic, rather
  than physical interface octet counters. They need not sum to the network card,
  which filters to physical adapters. They are not payload-only internet speeds.
- Unknown packet schema versions are unavailable. ETW event/buffer loss and the
  bounded in-memory accumulator's overflow suppress the affected interval and
  show an incomplete-measurement message. No traffic is fabricated to fill gaps.
- Normal shutdown releases the owned trace. Forced termination or a crash can
  leave an ETW session active until it is stopped externally or Windows restarts,
  as documented for ETW sessions.

## Energy

Windows E3/SRUM provides attributed, estimated historical energy, with hardware
dependent measurement support. It is not a portable live per-process wattmeter.
This implementation does not substitute CPU load, battery discharge or an E3
estimate for measured app energy. The Power detail explicitly reports per-app
energy measurements as unavailable; the existing battery charge/discharge rate
remains unchanged.

## Validation

Run `npm test`, `npm run typecheck`, `npm run build` in `windows`, and
`cargo test --lib system_monitor` in `windows/src-tauri`. The Rust tests cover
GPU engine/PID parsing, CPU/memory regression arithmetic, ETW packet decoding,
payload attribution, direction accumulation and permission errors. UI tests
cover stale responses, unavailable/idle states and bytes/bits formatting.

For a host with tracing privileges, run
`cargo test --lib live_trace_stops_its_owned_session -- --ignored --nocapture`.
It sends local TCP/UDP traffic over IPv4 and IPv6, checks attribution and verifies
that dropping the collector removes its session. Missing tracing privileges fail
this explicit integration test rather than passing without collecting traffic.
Run `cargo test --lib live_network_sampler_rows -- --ignored --nocapture` to
verify per-app receive/send rates in the UI snapshot and collector reopening.
Run `cargo test --lib live_gpu_counters -- --ignored --nocapture` to sample the
host GPU and check the resulting process rows.
All three live tests can be run together from an administrator terminal using
`cargo test --lib live_ -- --ignored --nocapture --test-threads=1`.
Also check rapid detail switching, process exit, sleep/resume and standard-user denial.

Validation on the development host: 441 frontend tests and the full Rust suite
(328 passed, 5 ignored) passed, as did TypeScript checking, the production frontend build and
`cargo check`. The live GPU probe returned a valid reading with five process
rows. Standard-user ETW access returned permission denied. A subsequent elevated
run passed all three live tests: TCP/UDP IPv4/IPv6 reported the expected test
process and at least the transferred bytes in both directions; the sampler
emitted positive receive/send rates for that app, closed and reopened its
collector, and the owned ETW session was absent after shutdown. The application
was then restarted with an elevated token to make the Network process list
available. Future ordinary launches still require tracing privileges; no account
policy or automatic elevation was added.
Before submitting the PR, `cargo fmt --check` and
`cargo clippy --all-targets --locked -- -D warnings` also passed.

## Windows references

- [SystemTraceProvider sessions and permissions](https://learn.microsoft.com/en-us/windows/win32/etw/configuring-and-starting-a-systemtraceprovider-session)
- [StartTrace lifetime and system logger restrictions](https://learn.microsoft.com/en-us/windows/win32/api/evntrace/nf-evntrace-starttracew)
- [TCP/IP event types and PID attribution](https://learn.microsoft.com/en-us/windows/win32/etw/tcpip)
- [UDP/IP event types and PID attribution](https://learn.microsoft.com/en-us/windows/win32/etw/udpip)
- [TCP IPv4 send schema](https://learn.microsoft.com/en-us/windows/win32/etw/tcpip-sendipv4)
- [UDP IPv4 schema](https://learn.microsoft.com/en-us/windows/win32/etw/udpip-typegroup1), [UDP IPv6 schema](https://learn.microsoft.com/en-us/windows/win32/etw/udpip-typegroup2)
- [Energy Estimation Engine and measured-power distinction](https://learn.microsoft.com/en-us/shows/defrag-tools/157-energy-estimation-engine-e3)
