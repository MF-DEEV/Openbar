# Windows live music equalizer (#43)

The equalizer reads real render audio. There is no simulated animation when
capture is unsupported, silent, or unavailable, and no microphone is opened.
Audio is never written to disk or sent to the webview: only seven normalized
frequency levels, a source ID, and capture mode are emitted.

## Capture and selection

`music_levels` follows the source published by the existing `media::Shared`. It
uses the existing audio engine's WASAPI session list and the Music volume
control's executable matcher to find the selected player's processes. Ancestor
and descendant sessions are reduced to nonoverlapping process trees.

On compatible systems, `ActivateAudioInterfaceAsync` opens WASAPI process
loopback with `PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE`. Microsoft
documents Windows build 20348 as the minimum for this API:
<https://learn.microsoft.com/en-us/samples/microsoft/windows-classic-samples/applicationloopbackaudio-sample/>.
Availability is probed at runtime; the app still supports older Windows versions.

If no process can be identified or process capture fails, shared-mode loopback
reads the default multimedia render device using its mix format. This fallback
hears **all audio on that output**, including unrelated apps. The frontend exposes
`data-audio-capture="system"` and the tooltip “System audio (player isolation
unavailable)”; it never labels that mode as isolated. The selected player's
playback state still gates monitoring. Browser isolation is application/process
tree isolation, not isolation of one tab or SMTC session within that application.

Process IDs and the default output are checked once a second while needed.
Changes rebuild affected capture streams. System fallback periodically retries
process capture (30 seconds, sooner after process changes). Lost devices and
failed streams release their COM resources and retry; missing data rests as dots.
Protected/exclusive audio and unusual unsupported mix formats can be unavailable.

## Processing and lifecycle

Each capture keeps a 2048-sample mono ring and runs a Hann-windowed radix-2 FFT.
The seven bands match macOS: 50–120, 120–250, 250–500, 500–1000, 1000–2000,
2000–4500, and 4500–12000 Hz. Integrated band RMS energy, compensated for Hann
window power, maps from a fixed -72 dBFS floor to 0–1. Integrating energy avoids
diluting claps and cymbals across the much larger number of treble bins. Unlike
automatic peak normalization, this preserves loud/quiet differences.
Multiple process trees contribute the maximum energy in each band. Sustained
levels use a power curve (exponent 0.85) that lifts quiet player output;
positive changes above 0.025 receive
a transient boost of 1.2 times that change. This makes actual hits pop without
adding motion to steady audio. Attack is 0.9 and decay 0.28 per update, with
near-zero values snapped to silence.
Silent packets and a 100 ms packet timeout clear stale samples.

At most 30 compact `music://levels` events are sent each second. Unchanged
levels send only a 200 ms heartbeat. React shares one listener, updates bar DOM
styles directly, samples bands evenly for smaller equalizers, and interleaves
their positions to spread bass/mids/treble around the meter. The arrangement is
stable, with bass inside rather than always at the left edge. Height rises
over 30 ms and falls over 80 ms. Missing events settle the bars after 300 ms.
Source checks reject outdated player data.

Capture requires both actual backend playback and a visible, moving equalizer.
Unmount, pause, page hiding, or reduced motion ends demand. A shared frontend
lease renews every 5 seconds and expires after 15 seconds if a webview disappears
without cleanup. Pausing or changing the backend source drops the streams;
`Drop` stops each WASAPI client before releasing it.

## Verification

Automated tests cover band separation, amplitude response, silence/nonfinite
samples, smoothing/decay, monitoring leases, bar bounds, shared event listeners,
late listener registration, registration failure, stale events, and cleanup.

Run from `windows`:

```text
npm run typecheck
npm test
npm run build
node scripts/check-island-layout.mjs --music-equalizer
```

Run from `windows/src-tauri`:

```text
cargo check --lib --offline
cargo test --lib --offline
cargo test --lib --offline music_levels::capture::tests::real_ -- --ignored --nocapture --test-threads=1
cargo test --lib --offline real_music_player_resolves -- --ignored --nocapture
```

The opt-in native tests play brief 750 Hz tones through the actual Windows
render device and read them through each loopback path. They compare loud,
quiet, silent, and stopped playback. The system test requires other apps to be
silent; process capture should pass even while another application plays.

Interactive acceptance still includes watching the island with a real player,
switching selected players, unplugging/changing output devices, and profiling
the complete running application's CPU/memory. Synthetic test tones are only
test inputs to real WASAPI; production levels always come from captured audio.

Implementation verification on Windows against `main`: TypeScript, production
build, 559 frontend unit tests, 434 Rust tests (10 opt-in tests ignored in the
regular run), Rust compilation/format checks, and the browser equalizer test
passed. The real process-loopback test passed:
750 Hz measured approximately 0.875 loud, 0.451 quiet, and 0 silent, including
after stopping the renderer. The system-loopback test captured the test tone
but failed its silence assertion with other audio still in the system mix.
The existing-player capture test also passed against a real playing
`Spotify.exe` session with process isolation. Additional tests confirm a quiet
750 Hz tone at amplitude 0.003 stays above 25% visual height, louder audio
still rises further, and silence/sub-floor noise settles completely.
Device switching and full-app resource profiling remain
interactive checks; these results do not claim they were performed.

Files added or changed for #43 (paths relative to `windows`):

- `src-tauri/src/music_levels.rs`
- `src-tauri/src/music_levels/capture.rs`
- `src-tauri/src/music_levels/spectrum.rs`
- `src-tauri/src/audio/engine.rs`
- `src-tauri/src/audio/mod.rs`
- `src-tauri/src/media_volume.rs`
- `src-tauri/src/lib.rs`
- `src/music/EqualizerBars.tsx`
- `src/music/audioLevels.ts`
- `src/music/audioLevels.test.ts`
- `src/shared/bridge.ts`
- `scripts/check-island-layout.mjs`
- `scripts/music-equalizer-cases.mjs`
- `docs/music-equalizer.md`
