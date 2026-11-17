# Record the island shortcut on Windows

In **Settings → Behavior → Global shortcut**, click **Change shortcut**, press
a combination, then release the main key. The keys above the button show the
last shortcut that Windows accepted and the app saved. The default is
**Ctrl + Alt + Space**.

Escape, clicking outside the recorder, leaving the Settings page, losing window
focus or closing the window cancels recording. The existing On/Off switch releases
the global shortcut and restores the saved combination when enabled again.

Use Ctrl or Alt, optionally with Shift, and a supported key. Plain keys,
Shift-only combinations, AltGr, Windows-key combinations, F12, system combinations
such as Alt+Tab or Ctrl+Alt+Delete, and the Sessions approval keys are unavailable.
The current sound output switcher shortcut is also protected when enabled.

Windows registration conflicts appear beside the setting. A refused shortcut is
not saved and the previously configured shortcut is restored after recording.
If another application acquires the old shortcut while recording has temporarily
released it, restoring it can also fail; the row reports that failure and another
combination can be recorded. Clicking the island always remains available.

## Implementation

`src-tauri/src/island_shortcut.rs` owns the island's registration through the existing
Tauri global-shortcut plugin. It validates candidates, registers the candidate
before releasing the previous key, rolls back if persistence fails, and keeps the
event handler's dispatch independent of registration locks. Preference updates
serialize registration and saving. No additional shortcut plugin or dependency is
introduced. On restart, the same existing `island.shortcut` preference is read.

The existing shared `ShortcutField` records real keyboard events. It waits for
native shortcut suspension before accepting keys and waits for key release before
saving, avoiding repeat-triggered island toggles. Recorder ownership and native
window focus/destruction cleanup also protect against a closing page failing to
run JavaScript cleanup. The audio output switcher shares this recording lifecycle
and reserves the configured island shortcut rather than its former hardcoded key.
Preference saves and recording transitions run on blocking workers; native event
handlers never wait for registration locks on the Windows event-loop thread.

## Changed files for issue #39

- Backend: `src-tauri/src/island_shortcut.rs`, `lib.rs`, `prefs.rs`, `settings.rs`,
  `audio/mod.rs`, `audio/switcher.rs`.
- Settings: `src/settings/IslandShortcut.tsx`, `BehaviorSettings.tsx`,
  `ControlsSettings.tsx`, `state.ts`, `state.test.ts`, `strings.ts`, `settings.css`.
- Shared controls: `src/shared/ui/ShortcutField.tsx`, `src/shared/shortcut.ts`,
  `shortcut.test.ts`, `bridge.ts`.
- Mixer integration: `src/mixer/switcherShortcut.ts`, `MixerOptionsControls.tsx`.
- Browser regression: `scripts/island-shortcut-cases.mjs`,
  `scripts/check-island-layout.mjs`, `package.json`.
- Documentation: this file.

## Verification

From `windows/`:

```sh
npm run typecheck
npm run test
npm run test:island-shortcut
npm run build
```

From `windows/src-tauri/`:

```sh
cargo check --offline
cargo test --offline --lib
cargo fmt --check
```

The browser regression covers native browser keyboard events, key-release timing,
readable display, preference saving, Escape, invalid input, focus loss, registration
error feedback, disabling/re-enabling, and removing listeners on page changes. It
saves `build/island-shortcut-settings.png` at the repository root. Rust tests cover
validation, default/saved/disabled preferences, feature conflicts, replacing a key,
failed registration, failed unregistration, persistence rollback and duplicate
registration prevention.

Global behavior additionally needs a running Windows app: test with Settings
unfocused, verify the former shortcut no longer toggles, restart with a custom
shortcut, and check a combination already registered by a different process.

These integration checks were exercised locally using the installed Tauri
WebView, browser-generated key events for recording, Windows keyboard injection
for global toggles, and a separate `RegisterHotKey` process for conflicts. Escape
and closing the recording window also restored the saved registration.
Restarting with `Ctrl+Shift+X` saved restored that registration successfully.
The macOS implementation is unchanged.
