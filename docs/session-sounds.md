# Session event sounds

Settings → Sessions → Sounds offers a separate choice for permission requests,
questions, completed turns, terminal errors, plan limits, and session starts.
The three Openbar WAV sounds are identical on Windows, macOS, and Linux. The
available system sounds depend on the host. `None` disables an event.

## Existing preferences

The old `waitingSoundChoice` is read for both permission requests and questions.
The old `waitingSound: false` makes both silent. The other four events start
silent for existing installations, and session starts are also silent on new
installations. New per-event settings take precedence when present. Migration
is read-through: the old keys stay untouched, and a settings file is not
rewritten just because Openbar was upgraded. Volume defaults to 100%.

## Event source and suppression

The established session snapshot service supplies parent session rows. A
permission or question is identified by the request ID and waiting phase;
completed turns and terminal failures come from the integrations' turn status.
Plan limits require an agent-reported limit status. A failed tool inside an
active turn is not a terminal error. An unknown terminal cause is left
unclassified. Codex and Claude status data determine which events can be
classified; when an agent gives no structured limit reason, Openbar does not
guess from arbitrary output text.

Existing focused session checks apply to every event. On the island, the
session page must show the relevant session. For a terminal, its own tab or
pane must be focused. A visible unrelated Openbar window or another session's
terminal does not silence it. Repeated snapshots have no new event identity.
The first snapshot establishes a baseline so restored sessions stay quiet.
Sounds arriving within one second share one audio notification, with plan
limits and errors taking priority. Session details remain visible as usual.

The island context menu provides one hour and next local midnight mute options,
plus early unmute. The expiration is saved as an epoch timestamp, checked when
an event arrives, and survives restarts, sleep, and wall-clock changes without
a polling timer. Preview uses the selected volume and remains available while
temporarily muted.

## Playback

- Windows uses a dedicated WinRT MediaPlayer with a per-player volume. It does
  not change the master volume or other applications. System WAV sounds are
  read from the Windows Media folder.
- macOS uses a dedicated NSSound with per-instance volume. System sound files
  available on the Mac can be selected.
- Linux uses the existing desktop audio tools. PulseAudio (`paplay`), PipeWire
  (`pw-play`), and ffplay receive a per-stream volume. ALSA (`aplay`) receives
  a software-scaled WAV. The app tries another available player if one fails
  immediately. Without a usable audio device or player, playback fails quietly
  and session updates continue. Some theme sounds require an Ogg-capable
  player; unavailable selections fall back to the Openbar chime.

Windows verification can be performed locally. macOS and Linux builds and
live agent/audio-device behavior require those operating systems; a Windows
build cannot establish their runtime behavior.
