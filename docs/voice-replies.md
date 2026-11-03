# Voice replies

Speak a reply to a session instead of typing it (issue #138). A microphone sits in the reply field of a session's page, small and enlarged, and an optional hold-to-talk shortcut does the same from any app. The words fill the field as you speak; the reply goes out by the same path as a typed one, so it works wherever typed replies do (Claude Code, Codex, OpenCode and the terminals in [terminal-hosts.md](terminal-hosts.md)). Where replies can't reach a session, the microphone isn't shown either.

Speech is recognized on the device, on both platforms. There is no cloud fallback.

## How it behaves

- Click the microphone, or hold the shortcut, and speak. Five bars show the level; with Reduce Motion (Mac) or `prefers-reduced-motion` (Windows) they light up instead of moving.
- Recording stops on a second click, when the shortcut is let go, after 1.5 s of quiet *following* speech (quiet before you start talking never ends it), or at 60 s.
- The words stay in the field so they can be corrected, and ⌘↩ / Ctrl+Enter sends them. With **Send when I stop talking** on, a click or shortcut stop and silence send at once; the 60 s cap never sends.
- Escape cancels and gives the field back what it held before.
- The session ending stops recording and nothing is written or sent. The island closing, or moving to another page, stops it and keeps the words unsent. A press waiting on a permission prompt is kept, since the prompt itself closes the island.
- Recognition uses the system's default input in shared mode. A call or Discord keeps its microphone, and the input device never changes. Audio is held in memory only while recording and dropped when recognition ends.

The rules are one state machine per platform, `VoiceReplyMachine` (Mac, `Sources/Openbar/Services/ClaudeSessions/VoiceReplySupport.swift`) and `windows/src/sessions/voice.ts`, with the same tests (`Tests/VoiceReplyTests.swift`, `voice.test.ts`).

Settings → Dynamic Island → Content → Sessions → **Voice replies**: on/off (off by default), language (follows the app's), hold-to-talk shortcut (off by default; ⌃⌥⌘A on the Mac, Ctrl+Alt+V on Windows once turned on), and Send when I stop talking (off by default).

## Mac: Apple's on-device recognizer

`SFSpeechRecognizer` with `requiresOnDeviceRecognition = true`, fed from `AVAudioEngine`'s input tap. Before starting, the island checks `supportsOnDeviceRecognition` for the language; when it is missing, the field says so and opens Keyboard settings, where Dictation downloads languages. It never asks Apple's servers.

Facts found while building it:
- **Dictation must be on.** With Keyboard → Dictation off, the on-device recognizer refuses to start even though `supportsOnDeviceRecognition` is true (`kLSRErrorDomain` 201, "Siri and Dictation are disabled"). The field says to turn Dictation on and opens Keyboard settings.
- The microphone and Speech Recognition prompts come only on the first press, never at launch (`NSMicrophoneUsageDescription`, `NSSpeechRecognitionUsageDescription`).
- The prompt takes focus and closes the island; the press waits for the answer and the island comes back on the session.
- `--voice-reply-file <wav>` replays a file in place of the microphone through the same recognizer, for end-to-end runs.

## Windows: whisper.cpp, not Windows' dictation

**Decision.** `Windows.Media.SpeechRecognition` free dictation (`SpeechRecognitionTopicConstraint`) is a web-service grammar. It needs Settings → Privacy → Speech → Online speech recognition, and sends the audio to Microsoft. Only list and SRGS grammars run on the device, and they can't take free dictation. The older SAPI desktop dictation is deprecated and covers only a few of the app's 15 languages. Neither keeps the promise, so voice replies use **whisper.cpp** through `whisper-rs`, linked into the app.

- Model: whisper.cpp's multilingual base model, quantized (`ggml-base-q5_1.bin`, 60 MB, SHA-256 `422f1ae4…8898`). It is downloaded once, when the person presses Download in Settings (the size is shown first), from `huggingface.co/ggerganov/whisper.cpp`. It is checked against its hash before it is kept, saved to `%APPDATA%\com.openbar.island\models\`, and can be deleted from Settings. No audio is ever sent.
- Capture: WASAPI shared mode on the default communications input, converted by Windows to 16 kHz mono float.
- While recording, what was heard is transcribed again about every 1.2 s for live text, and once more on stop. Audio quieter than about -44 dBFS is never transcribed, because whisper makes words up from a quiet room. A hard cap of 75 s on the Rust side backs up the island's 60 s.
- On an ARM64 VM with 8 cores, a 3.7 s sentence takes about 0.8 s.
- Microphone privacy off for desktop apps (`E_ACCESSDENIED`) opens `ms-settings:privacy-microphone`; no input device opens `ms-settings:sound`.
- `Openbar.exe --voice-probe <wav> <out> [language]` runs the recognizer on a 16 kHz WAV. `--voice-reply-file <wav>` replays a file in place of the microphone, as on the Mac.

### Building from a Mac

whisper-rs-sys decides some things by the build machine, not the target. Cross-building with cargo-xwin goes through `windows/tools/xwin-cargo.sh`, which points CMake at `windows/src-tauri/cmake/xwin-<target>.cmake`:
- the C runtime is linked statically (`/MT`), like the rest of the app, so no Visual C++ Redistributable is needed;
- an empty `ggml-blas.lib` stands in for the BLAS library the build script asks for on a Mac;
- `advapi32` comes from the app's `build.rs`.

```sh
windows/tools/xwin-cargo.sh cargo xwin build --release --target aarch64-pc-windows-msvc   # what tools/vm/deploy.sh runs
cd windows && tools/xwin-cargo.sh bunx tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc
```

It needs `cmake` and `ninja` (`brew install cmake ninja`). A build on Windows needs none of this.

## Verified

- Mac: a throwaway `claude --model haiku` session got the replayed sentence through the on-device recognizer, live, and answered it; the Dictation-off message and the island reopening after the prompt were found and fixed in that run. Not yet run with the real microphone or the hold-to-talk shortcut.
- Windows (ARM64 VM): the model download from Settings, live text, silence, Escape, hold-to-talk with send-on-stop, and the session ending mid-recording, against a real Codex session. The VM has no input device, so the real microphone and the privacy-denied path are not verified there; "no microphone" is.
