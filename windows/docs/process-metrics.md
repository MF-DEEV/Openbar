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
session and never stops the global NT Kernel Logger. It and the helper below
add only feature flags to the existing `windows` dependency.

The documented version 2 TCP/UDP IPv4/IPv6 send and receive records contain a
payload PID and packet size. The payload PID identifies the originating process;
the event header PID can identify a system thread instead. Retransmit, connect
and copy events are excluded to avoid counting them as new app traffic.
Per-app receive and send bytes are divided by the elapsed sampling interval;
their sum ranks the rows. Event timestamps are checked against process creation
times to discard traffic belonging to recycled PIDs. Traffic for processes that
have exited is omitted.

### Without running the island as administrator

On the Mac `nettop` needs no privileges. On Windows a system ETW logger does:
started from the island's usual (limited) token it is denied. Windows' own
per-app data usage (`GetAttributedNetworkUsageAsync`) needs no privileges but
is batched by the hour, too late for live rates. So when the island's own
trace is denied, `network_helper.rs` takes over:

- An administrator approves it once with the network detail's **Allow**
  button. Windows shows its UAC prompt, and the elevated
  `Vorssaint.exe --install-network-helper` copies the exe to
  `%ProgramFiles%\Vorssaint\Vorssaint Network.exe` and registers the hidden
  task `Vorssaint Network Activity <user SID>`. It runs that copy with
  `--network-helper` as the same user with the highest privileges, on demand
  only. Administrators and SYSTEM own the task; the user may read and run it
  but not change or delete it. Installing protects the copy's folder so that
  only administrators and SYSTEM may write to it (users may read and run it),
  whatever its parent or an existing folder allowed, so the task can't be used
  to run anything else elevated.
- From then on, opening the network detail runs the task without a prompt.
  The helper creates `\\.\pipe\vorssaint-network-<user SID>` (local clients
  only, read and write for that user at medium integrity), starts its own trace
  when the island connects, answers each sample request with every process's
  bytes since the previous one and stops the trace when the island hangs up.
  After 30 seconds without an island it exits. The island checks that the
  pipe's server is the Program Files copy before reading from it.
- Before starting the helper the island compares the copy with its own exe.
  After an update they differ, so the detail asks for approval again, which
  ends this account's older copy still waiting for an island (never another
  account's, which may be serving its island) and copies the current exe.
  The greeting also carries a protocol number, checked the same way.
- The prompt is started through PowerShell. With the sparse package identity
  (docs/notifications.md) Windows refuses to elevate a child of Vorssaint
  and reports "cannot access the specified device, path, or file" only after
  the user agreed. PowerShell has no package identity.
- Uninstalling (`--remove-network-helper`, from the installer's hook) asks for
  an administrator only when the task exists, deletes it and, when no other
  account on the PC still has one, the copy. A copy still running is deleted
  at the next restart; the island then offers **Allow** again.
- Standard accounts can't approve it (Windows would run the helper with their
  own, limited token) and see that it needs an administrator account. An
  island that already runs elevated, or with UAC off, starts its own trace.

Limitations:

- Without the approval, or on a standard account, there is no per-app list.
  Access denied and missing privileges have a separate UI message; other
  failures show unavailable.
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
The helper was checked end to end in the Windows 11 ARM64 test VM from an
island running with a limited token: Allow, declining the prompt, the list
filling with the right app, a restarted island and a reopened detail without
any prompt, the trace stopping when the island closes, the helper exiting
after 30 seconds and the unelevated uninstall step.
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
available. Ordinary launches now use the approved helper described above; no
account policy is changed.
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
