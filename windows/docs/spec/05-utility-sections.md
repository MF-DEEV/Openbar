# 05 — Utility sections: behavior spec and Windows backend map

This spec covers these island pages and surfaces: **Controls** (home page with levels and shortcut tiles), **Mixer**, **Clipboard**, **Captures** (recent captures plus the post-screenshot preview), **Capture controls** (the island during a screen selection), **Files** (shelf plus file tools), **Tools** (the quick panel and its hosted Quick Toggles), the **Quick Access** side buttons, **Camera**, **Mirror** (copies of the closed island on other displays), **Scratchpad**, **Mascot** (the companion), and **Accessory** notices. The island shell, geometry, top-row "band", notices and music are in the other specs. Here they appear only where a section plugs into them.

Each section lists what it shows, how it behaves, the preferences it reads, the Mac service APIs it calls, the **minimal** Windows backend the island version needs, and the Windows 11 API for each capability. ⚠️ marks a capability with no good Windows equivalent, or one that needs an undocumented API.

Path abbreviations used in `file:line` refs (all under `Sources/Vorssaint/`):

| Prefix | Path |
|---|---|
| `NCV` | `UI/Notch/NotchControlsView.swift` |
| `NMV` | `UI/Notch/NotchMixerView.swift` |
| `NCBV` | `UI/Notch/NotchClipboardView.swift` |
| `NCCV` | `UI/Notch/NotchCaptureControlsView.swift` |
| `NFV` | `UI/Notch/NotchFilesView.swift` |
| `NQAV` | `UI/Notch/NotchQuickAccessView.swift` |
| `NCamV` | `UI/Notch/NotchCameraView.swift` |
| `NMirV` | `UI/Notch/NotchMirrorView.swift` |
| `NSPV` | `UI/Notch/NotchScratchpadView.swift` |
| `NMasV` | `UI/Notch/NotchMascotView.swift` |
| `NV` | `UI/Notch/NotchView.swift` |
| `NComp` | `UI/Notch/NotchComponents.swift` |
| `NSup` | `Services/Notch/NotchSupport.swift` |
| `NSvc` | `Services/Notch/NotchService.swift` |
| `NMS` | `Services/Notch/NotchMascotSupport.swift` |
| `NFTS` / `NFTSup` | `Services/Notch/NotchFileToolsService.swift` / `NotchFileToolsSupport.swift` |
| `NAcc` / `NAccSup` | `Services/Notch/NotchAccessoryService.swift` / `NotchAccessorySupport.swift` |
| `QLV` / `QLS` | `UI/QuickLauncher/QuickLauncherView.swift` / `Services/QuickTools/QuickLauncherService.swift` |
| `QTS` / `QTSec` | `Services/QuickTools/QuickTogglesService.swift` / `UI/MenuPanel/QuickTogglesSection.swift` |
| `RCV` | `UI/MenuPanel/PanelRecentCapturesView.swift` (contains `RecentCapturesView`) |
| `RCStore` | `Services/QuickTools/RecentCaptureStore.swift` |
| `SQP` | `Services/QuickTools/ScreenshotQuickPreviewController.swift` |
| `SCS` / `SSup` | `Services/QuickTools/ScreenCaptureService.swift` / `ScreenshotSupport.swift` |
| `STV` / `SS` / `SSupp` | `UI/Shelf/ShelfTilesView.swift` / `Services/Shelf/ShelfService.swift` / `ShelfSupport.swift` |
| `CPS` / `CPV` / `CPSup` | `Services/QuickTools/CameraPreviewService.swift` / `UI/CameraPreview/CameraPreviewView.swift` / `CameraPreviewSupport.swift` |
| `AVM` / `MRS` | `Services/Audio/AppVolumeMixer.swift` / `MixerRoutingSupport.swift` |
| `CHS` / `CHSup` | `Services/Clipboard/ClipboardHistoryService.swift` / `ClipboardHistorySupport.swift` |
| `CCS` | `Core/NotchClipboardSizeStrings.swift` (contains `NotchClipboardCardSize`) |
| `SPSup` | `Services/QuickTools/ScratchpadSupport.swift` |
| `DEF` | `Core/Defaults.swift` |

---

## 0. Summary and cross-cutting rules

* **There is no camera housing on Windows.** Every page's top row ("band") takes the no-camera branch: `NotchBand.besideCamera == false` (`NSup:223-262`). The band's leading and trailing slots span the whole row. Every "beside the camera" variant below can be skipped. That includes the narrow menu label, the levels split one per side, the capture-controls two-column header and the mascot hiding behind the camera. Each variant is still noted, in case someone later adds a "notch" silhouette at the top edge. The mascot always runs on its **capsule** track (`hidden == nil`, §12).
* **Shared visual atoms** (from `NComp`), reused by every section here:
  * `NotchButtonStyle` (`NComp:10-35`): under the pointer, a white 0.09 wash fills the shape. Pressed: opacity 0.8, scale 0.965. Hovered: scale 1.022 when `lifts`. Disabled: opacity 0.4. Press spring 0.28 s / damping 0.7, hover spring 0.32 s / 0.75. Reduce Motion disables the scaling.
  * `NotchIconButton` (`NComp:144-170`): 28×28, glyph 12 pt medium, white 0.55 (white when selected), selected background white 0.12, radius 9. The tooltip is the title.
  * `NotchEmptyView` (`NComp:183-230`): a 56×56 tile (white 0.05, radius 18) holding a 26 pt light glyph (white 0.65), then a 12 pt message.
  * `NotchControlSurface` (`NComp:687-725`): a card fill of white 0.065 (0.12 when selected), or a hairline glass variant. Radius is set per use.
  * `NotchPillButton` (`NComp:232-250`): a capsule with a 12 pt label (semibold and white 0.14 fill when prominent).
  * `NotchRail` (`NComp:290-360`): items fill the rows the height allows, then continue sideways and scroll horizontally. Gaps shrink down to `leastSpacing` before the rail starts to scroll (`NSup:494-545`).
  * Text is white. Secondary text is SwiftUI `.secondary`, which on the always-dark island reads as roughly white at 0.55. Tertiary is roughly white at 0.25.
* **Escape layering.** A page that opens an inner layer (mixer options, clipboard search) registers a closer with `setPageLayer(module, close:)` (`NSvc:2027-2032`). Escape runs that closer first, then closes the island (`stepBack`, `NSvc:2014-2025`).
* **`service.perform { … }`** collapses the island, then runs the action once the window has settled (`NSvc:2289-2293`). Every "open something else" action uses it, so keep the same ordering on Windows (collapse, wait for the resize animation, then act).
* **Confirmations "above the island".** These are `NSAlert.confirmAboveIsland` and `NotchIslandAlert.run`. On Windows they become in-webview modal sheets, or a Tauri `dialog::ask` parented to the island window. The island window is topmost, so a dialog with no parent would open behind it.
* **Keyboard modifiers.** Every ⌘ shortcut maps to Ctrl, and the "⌘1" style badges render as "Ctrl+1" (see Open questions).

---

## 1. Controls (home page)

### 1.1 What it shows
Source: `NCV:7-118`, layout math `NSup:608-640`, item list `NSup:1283-1360`, `NSup:1984-1993`.

The page has up to three rows, top to bottom, with `NotchLayout.rowSpacing = 10` between them (`NSup:278`):
1. **Levels**: `volume`, `brightness`, `keyboardLight`. Without a camera, all levels sit in the band's **leading** side, each at least `bandLevelMinimumWidth = 76` wide with `bandLevelSpacing = 12` between them, as long as they fit beside the sections button (`bandLevelCount`, `NSup:575-583`). The check is all or nothing: if they don't all fit, **every** level moves to a `NotchLevelRow` 28 pt tall below the band (`NCV:31`, `NSup:287`). Each level is 28 pt tall.
2. **Music card** (when `music` is among the items): `NotchMusicControlsView` at `cardRow` height, 68 to 80 pt (`NSup:279-281`). It is specified in the music spec.
3. **Shortcut rail**: tiles of 76×74 (`shortcutWidth`, `shortcutHeight`), spacing 8, collapsing to 0 before scrolling (`NSup:288-295`). There are as many rows as fit. A rail that is a few points short of a further row may tighten its row gap down to 6 to gain that row (`NSup:619-627`).

When no items remain, the page shows `NotchEmptyView(symbol: "slider.horizontal.3", message: notch.empty)` (`NCV:26-27`).

**Level: Volume** (`NotchAudioControls`, `NCV:142-217`). This is an HStack with spacing 6, 28 pt tall:
* A mute button with an 18×18 hit area, 12 pt medium glyph, `speaker.wave.2.fill` (white) or `speaker.slash.fill` (red). The glyph changes with a symbol-replace transition, 0.24 s smooth. It is disabled while the mute state is unknown.
* A horizontal `NotchLevelSlider`, 24 pt tall with an 8 pt track, that eases external changes. When the level is unknown, the slider is replaced by `mixerOutputUnavailable` at 10 pt secondary.
* `NotchLevelChooser`: a `chevron.down` at 9 pt semibold secondary in a 16×24 frame, opening a menu. The menu lists one checked item per output device that can be default, then, if the mixer page is enabled, a separator and "Mixer" (`NCV:205-216`).
* The tooltip is the output switch error, or else the device name.

The level shown is `systemOutputMuted ? 0 : systemOutputVolume`. Each adjustment first calls `notch.noteOwnLevelAdjustment(.volume)` so the top-row volume notice doesn't repeat it (`NCV:165-168`).

**Level: Brightness** (`NCV:279-331`). The row is a sun glyph (12 pt medium, 18 wide), a slider, and a display chooser that appears only when there is more than one display. The slider targets `focusedDisplayID` (the display the brightness keys last moved), else the built-in display, else the first display (`NCV:285-287`). Brightness is set with `announces: false`. When `brightnessControlEnabled` is false, the slot shows a setup row instead: sun + "Brightness" + a "Settings…" button, or sun + "Settings…" when narrow (`NCV:67-92`).

**Level: Keyboard light** (`NCV:335-384`). The glyph button is `light.max` or `light.min` and toggles the light on or off. The slider sets the level. When unsupported, the slot shows `notchEditor.keyboardLightUnavailable`.

**Shortcut tile** (`NotchActionTile`, `NCV:412-483`):
* A 40×40 circle. Inactive: white 0.075 fill, glyph white 0.85. Active: `accent.fill` with `accent.glyph`. The glyph is 17 pt medium.
* Below it, a 28 pt tall label area. A title alone is 11 pt medium white, 2 lines, centered, scaling down to 0.85. A tile with a **reading** shows the reading at 11 pt semibold monospaced digits on the first line, with rolling-digit animation (`NotchRollingDigits`) and a scale floor of 0.7. The title follows on the second line at 11 pt white 0.7, falling back to `compactTitle` when it doesn't fit.
* Horizontal padding 4. State changes animate with a 0.26 s smooth. The whole tile uses `NotchButtonStyle(cornerRadius: 14)`.
* Accents (`NCV:389-407`): `selection` = white fill / black glyph, `alert` = red / white, `awake` = yellow / black, `timer` = orange / black.

### 1.2 Items, order and preferences
`NotchControlItem` (`NSup:1283-1286`): `volume, brightness, keyboardLight, music, mixer, keepAwake, timer, calendar, microphone, screenshot, recording, speedTest, panel, commandBar, scratchpad`.

* Order comes from `notchControlOrder` (CSV of raw values, default `""`), followed by every remaining case in declaration order. Items listed in `notchHiddenControls` are hidden; its default is `keyboardLight,microphone,screenshot,recording,speedTest,panel,commandBar,scratchpad` (`NSup:1285`, `DEF:1436-1439`). An item also needs its feature to be available (`NSup:1336-1355`).
* `brightnessControlEnabled` (default false, `DEF:1371`) switches the brightness slot between the slider and the setup row.

| Item | Symbol (SF) | Title source | Action |
|---|---|---|---|
| keepAwake | `cup.and.saucer` / `.fill` when active, accent awake | `s.keepAwakeTitle` | `KeepAwakeManager.toggle()`; starts the last-picked duration (`defaultDuration`, 0 = indefinite) |
| microphone | `mic.fill` / `mic.slash.fill`, accent alert when muted | `s.micMuteName` / `s.micUnmuteName` | `MicMuteService.toggle()` |
| screenshot | `camera.viewfinder` | `recentCaptures.screenshot` | `perform { ScreenshotService.capture() }` |
| recording | `record.circle` / `stop.circle.fill`, accent alert while recording | `recorder.pageTitle` | `perform { recorder.toggle() }` |
| speedTest | `speedometer` | `s.speedTestRun` | `showMetric(.network)` (system spec) |
| panel | `bubble.middle.top` | `notch.panel` | `openAppPanel()` (the menu-bar panel inside the island) |
| mixer | `speaker.wave.2` | `s.mixerSection` | `select(.mixer)` |
| commandBar | `command` | `commandBar.pageTitle` | `perform { CommandBarService.show() }` |
| scratchpad | `note.text` | `scratchpad.pageTitle` | `openScratchpad()`: the island page if routed, else the floating pad |
| timer | `timer`; with a session: `checkmark` / `pause.fill` / `stopwatch` / `timer`, accent timer | phase name, compact = module title; reading = clock | `select(.timer)`; ticks every 1 s while running (`NCV:488-523`) |
| calendar | `calendar` | next event's title (empty = `notchCalendar.untitled`) | reading = start text, refreshed every minute (`NCV:527-547`) |

### 1.3 Minimal Windows backend

| Capability | Windows 11 API |
|---|---|
| Output volume / mute: get, set, change events | `IMMDeviceEnumerator::GetDefaultAudioEndpoint(eRender, eMultimedia)` → `IAudioEndpointVolume` (`GetMasterVolumeLevelScalar`, `SetMasterVolumeLevelScalar`, `GetMute`, `SetMute`) plus `IAudioEndpointVolumeCallback::OnNotify` for changes. Pass our own event-context GUID so the island can recognize changes it made itself (the equivalent of `noteOwnLevelAdjustment`). |
| Output device list and default changes | `IMMDeviceEnumerator::EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)`, `PKEY_Device_FriendlyName`, `IMMNotificationClient` (`OnDefaultDeviceChanged`, `OnDeviceStateChanged`) |
| Switch the default output ⚠️ | **Undocumented** `IPolicyConfig::SetDefaultEndpoint(id, role)` (CLSID `870af99c-171d-4f9e-af0d-e63df40c2bc9`, IID `f8679f50-850a-41cf-9c72-430f290290c8`; verify against EarTrumpet or SoundSwitch). Call it once each for `eConsole`, `eMultimedia` and `eCommunications`. Microsoft has no documented alternative. |
| Mic mute (all inputs) | Run `IAudioEndpointVolume::SetMute(TRUE)` on every active `eCapture` endpoint. Persist the IDs we muted so we unmute only those (the Mac keeps `micMuteMutedDevices`). Watch `IMMNotificationClient` for devices arriving while muted. |
| Keep awake | `PowerCreateRequest` + `PowerSetRequest(PowerRequestSystemRequired / DisplayRequired)`, or `SetThreadExecutionState(ES_CONTINUOUS \| ES_SYSTEM_REQUIRED \| ES_DISPLAY_REQUIRED)` from a dedicated thread. Timed sessions use a Rust timer. |
| Brightness, built-in panel | WMI `root\WMI`: `WmiMonitorBrightness.CurrentBrightness` to read and `WmiMonitorBrightnessMethods.WmiSetBrightness(timeout, level)` to set. Change events: `WmiMonitorBrightnessEvent`. |
| Brightness, external displays | DDC/CI through `dxva2`: `GetPhysicalMonitorsFromHMONITOR`, `GetMonitorBrightness`, `SetMonitorBrightness`. Calls are slow (50–200 ms), so throttle slider writes to about 10 Hz on a worker thread. Display names come from `DisplayConfigGetDeviceInfo(DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME)`. |
| "Focused display" (the display the brightness keys last moved) ⚠️ | Windows reports no event for which display a brightness key changed. Use the WMI brightness event for the internal panel, and otherwise the display the island is on. |
| Keyboard light ⚠️ | No generic API; it is vendor-specific (Dell, Lenovo and others). Make `keyboardLightIsSupported()` return false, which hides the item and the level. |
| Timer, calendar, music, speed test, panel, command bar tiles | Owned by other specs or features. On Windows, hide `panel` and `commandBar` unless those features are ported (their `isAvailable` returns false). |

Rust command surface (suggested): `audio_output_state() -> {volume, muted, devices[{id,name,isDefault}], defaultId}`, `audio_set_output(volume?, muted?)`, `audio_set_default_output(id)`, event `audio://output-changed`; `mic_mute_toggle()`, event `mic://changed`; `keep_awake_toggle()`, event `keepawake://changed`; `brightness_displays()`, `brightness_set(id, value)`.

---

## 2. Mixer (per-app volume)

### 2.1 What it shows
Source: `NMV:13-155` (page), `NMV:193-293` (master and mic faders), `NMV:341-479` (options), `NMV:485-671` (app column).

* **Band.** The leading side holds the output-device menu (`NotchOutputDeviceMenu(inBand: true)`), a one-line label naming the current device or the switch error. The trailing side holds the options button: `slider.horizontal.3`, or `xmark` while the options are open, with title `s.keepAwakeOptions` or `s.menuClose` (`NMV:66-70`).
* **Desk** (an HStack, spacing 10, top aligned, `NMV:50-60`):
  * **Master fader**, 72 wide (`NMV:193-243`): a 32×32 mute button (15 pt glyph, red when muted), a vertical slider 40 wide with a 28 pt track, and an editable percent (28 pt tall, max 100). Track height is `min(160, max(32, pageHeight − 32 − 28 − 12))` (`NMV:296-300`). The fader height is `max(104, page height)` (`NSup:369`).
  * **Microphone fader**, 72 wide, shown only when the input reports a level (`NMV:246-293`). The mic-mute button exists only when the Mic Mute feature is available. The slider and percent are disabled while the mic is muted.
  * A 1 pt divider, white 0.12.
  * **App rail**: `NotchRail` with one row, 96-wide columns, width `page − leadingWidth − 21` (`NMV:101-123`).
* **App column** (`NotchAppFader`, `NMV:485-577`), top to bottom with spacing 6:
  * The icon (28×28 inside a 32 pt row). When the app is playing, a 7 pt green dot with a 1.2 pt black ring sits bottom-right, offset (1,1). When the app is routed to a device of its own, a 12 pt black circle badge sits bottom-left, offset (−2,1): `arrow.triangle.branch` in white, or `exclamationmark.triangle.fill` in orange when that device is unavailable. A pinned app shows a 9 pt `pin.fill` on the leading edge. An `ellipsis` actions menu (24×28, white 0.7) sits on the trailing edge.
  * The vertical slider, range 0…`maxVolume` = 2.0 (`AVM:80`), with a tick marker at 1.0. The tint is orange when the volume is above 100%, else white. The value label reads "n%".
  * The editable percent (max 200). A `bolt.fill` 7 pt glyph appears when boosting. A reset button (`arrow.counterclockwise`) is overlaid on the left and is hidden at 100%. A mute button is overlaid on the right; it is red when the volume is ≤ 0.001.
  * A **bypassed** app (conferencing or pro-audio apps on the Mac) shows only `s.mixerBypassedCaption` at 9 pt in place of the fader.
* **Editable percent** (`NMV:159-189`): text "n%", 12 pt when tall and 10 pt otherwise, medium weight, monospaced digits, with a numeric-text transition (0.2 s). A click turns it into a text field accepting a typed number, clamped to `maximum`.
* **Empty and error states** (`NMV:91-155`): when unsupported, `s.mixerUnavailable`. When permission is needed, a 56 pt `waveform.badge.exclamationmark` tile, `s.mixerPermissionBody`, and an "Open Settings" button. When there are no apps, `s.mixerEmpty`.
* **Options layer** (`NMV:341-479`) replaces the desk, with a 0.18 s easeOut. It is a scroll view of 22 pt rows (icon 11 pt, title 11 pt secondary, a menu on the trailing edge, 200 wide): System output (with "no devices" and error messages at 9.5 pt), Sound effects output, Microphone (device menu with "Default" plus the devices; a mic slider row; messages for no devices, fallback, or error). Below a divider sit the panel's `MixerOptionsControls` (`MS:556-571`): hide inactive apps, lower the volume on headphone disconnect, the precise volume roller, the output switcher (`SoundOutputSwitcherControls`), the Audio priority disclosure (`AudioPriorityDisclosure`) and the visible apps.

### 2.2 Interactions and preferences
* App order: `mixerAppArrangement` (string-serialized pinned set plus order, default `""`, `DEF:1317`). Pinned apps come first. Command-drag reorders, but only within the same pinned or unpinned group (`NMV:108-121`). The actions menu and context menu offer Pin first / Unpin, Move left, Move right, Output ▸ (Default, each device with a "(current)" suffix, an "unavailable" row, and an AirPlay chooser), Reset to 100%, and Hide from list (`NMV:623-660`).
* `mixerHideInactiveApps` (default false, `DEF:1316`): when on, an app shows only if it is playing, its volume isn't 100%, or it has its own route (`MRS:361-367`).
* Options toggles: `mixerLowerVolumeOnHeadphonesDisconnect` (default false) with `mixerHeadphonesDisconnectVolumePercent` (default 25), and `preciseVolumeRollerEnabled` (default false) (`DEF:1318-1321`, `DEF:2701`). The switcher keeps `soundOutputSwitcherEnabled` (default false), `soundOutputSwitcherShortcut` (Mac ⌃⌥⌘S; Windows `Ctrl+Alt+Shift+KeyS`, stored as an accelerator) and `soundOutputSwitcherDevices`. Audio priority keeps an on switch and an ordered list each for outputs and microphones, with last-known names; on Windows they live in `audio-priority.json` beside `settings.json`, both switches off until turned on (the Mac's feature is an opt-in install). The guard, the roller and the switcher rest while the Mixer section is hidden or off, or the island is off (`hiddenModules`, `sections.mixer`, `enabled`), as the Mac's need the mixer installed; audio priority stays independent of the section on both.
* Escape closes the options first (`NMV:73-77`).

### 2.3 Mac APIs used
`AppVolumeMixer` (CoreAudio process taps plus aggregate devices; macOS 14.4+): `apps`, `outputDevices`, `systemOutputVolume/Muted`, `setVolume(_:for:)`, `toggleMute`, `setOutputDeviceUID(_:for:)`, `setUniversalOutputDeviceUID`, `setSystemSoundOutputDeviceUID`, `hideFromList`, `refreshApps` (`AVM:73-95`, `522-960`, `1219`, `1864`). `AudioInputDeviceManager` handles the input level and device choice. `MicMuteService`, `AudioPriorityService`.

### 2.4 Minimal Windows backend

| Capability | Windows 11 API |
|---|---|
| Enumerate apps playing audio | On the default render endpoint: `IAudioSessionManager2::GetSessionEnumerator` → `IAudioSessionControl2` (`GetProcessId`, `IsSystemSoundsSession`, `GetDisplayName`, `GetIconPath`). Group sessions by process tree: Chrome and Edge play audio from a separate audio-service utility process, so roll child processes up to their root exe, the way the Mac rolls up "responsible processes". Guard against PID reuse by also comparing the process creation time (`GetProcessTimes`). **Enumerate every active render endpoint**, because each one has its own session list. |
| New and ended sessions | `IAudioSessionManager2::RegisterSessionNotification` (`IAudioSessionNotification::OnSessionCreated`). Known quirk (Microsoft Learn): call `GetSessionEnumerator` and then `IAudioSessionEnumerator::GetCount` before relying on notifications, and register from a non-UI **MTA** thread (`CoInitializeEx(COINIT_MULTITHREADED)`). Tauri's main thread is STA, so run all audio-session COM on a dedicated MTA worker. Per-session `IAudioSessionEvents` (`OnStateChanged`, `OnSessionDisconnected`, `OnSimpleVolumeChanged`). |
| "Is playing" dot | `AudioSessionStateActive` from `IAudioSessionControl::GetState`, or `IAudioMeterInformation::GetPeakValue() > 0.0001` polled at about 4 Hz while the page is open. |
| Per-app volume and mute | `ISimpleAudioVolume::SetMasterVolume(0…1)` / `SetMute` on every session of the app. ⚠️ **No boost above 100%**: `ISimpleAudioVolume` caps at 1.0. Boosting would need an APO or a virtual driver. Recommendation: on Windows, set the column's range to 0–100%, remove the orange boost styling, and set the max of the editable percent to 100. This is a deliberate visible deviation. |
| App name and icon | `QueryFullProcessImageNameW` → `GetFileVersionInfo` (`FileDescription`) for the name. For the icon, `SHGetFileInfoW(SHGFI_ICON \| SHGFI_LARGEICON)` or `IShellItemImageFactory::GetImage(32×32 @ scale)`, encoded as PNG for the webview. System sounds: localized "System sounds" plus a speaker icon. |
| Per-app output routing ⚠️ | **Undocumented** `IAudioPolicyConfigFactory` (Windows "App volume and device preferences"), activated with `RoGetActivationFactory("Windows.Media.Internal.AudioPolicyConfig")`. Call `SetPersistedDefaultAudioEndpoint(pid, eRender, eMultimedia/eConsole, deviceId)`. The device ID is an HSTRING in the `\\?\SWD#MMDEVAPI#{…}#{e6327cad-dcec-4949-ae8a-991e976a79d2}` form for render (`#{2eef81be-33fa-4800-9670-1cd474972c3f}` for capture). `IPolicyConfig::SetDefaultEndpoint`, by contrast, takes the plain `IMMDevice::GetId` string. EarTrumpet switches IIDs at build 21390, so on Windows 11 only the new IID `ab3d4648-e242-459f-b02f-541c70306324` applies (the old one, `2a59116d-…`, covers Windows 10 builds including 21H2/19044). A route only takes effect after the app reopens its stream. Minimal port: **leave per-app routing out** (hide the Output ▸ submenu and the route badge) unless it is explicitly accepted. |
| System output menu, default switch | See §1.3 (`IPolicyConfig`, ⚠️ undocumented). |
| Sound effects output ⚠️ | Windows has no separate "alert sounds device". The closest concept is the default *communications* device, which is different. Hide this row. |
| Microphone level and device | `IAudioEndpointVolume` on `GetDefaultAudioEndpoint(eCapture, eCommunications)`. Choosing a device uses `IPolicyConfig` on `eCapture` (⚠️). |
| Audio priority | `IMMNotificationClient` reloads the devices; a pass 250 ms later compares the settled set with the final one and makes the first connected listed device the default through `IPolicyConfig` (`audio/priority.rs`). Newcomers are placed after 2 s. Tiers come from the endpoint's `PKEY_Device_EnumeratorName` (`HDAUDIO` built in, `ROOT`/`SWD` virtual, the rest hardware; display audio and virtual-cable names by name). Shown only where `IPolicyConfig` answered. |
| Output switcher | A global shortcut (tauri-plugin-global-shortcut, `RegisterHotKey`) moves the default through the chosen outputs (`audio/switcher.rs`). The island's toggle, the Sessions approval keys (Ctrl+Y/N/T) and a key without Ctrl, Alt or Win are refused. |
| Permission state | None on Windows. `needsPermission` is always false and `isSupported` always true. |
| Bypassed apps | Not needed: sessions work for every app. Drop `isBypassed`. |
| Hidden apps list, pin and order persistence | App-level JSON keyed by exe path or AppUserModelID (Windows' equivalent of `persistenceID`, the bundle ID). |
| Lower on headphone disconnect | `PKEY_AudioEndpoint_FormFactor == Headphones/Headset` or the Mac's name terms, followed through `IMMNotificationClient` (`audio/guard.rs`). The level goes back when headphones are the output again, or when the app quits. A driver that keeps one endpoint for a switching jack isn't seen. |
| Precise volume roller | While on, the volume keys alone or with Shift are registered as global shortcuts (`RegisterHotKey` through the plugin), so Windows never sees them; with Alt, Ctrl or Win they stay Windows'. Each press steps the output 1% instead of Windows' 2%, with the Mac's gate for wheel bursts, and a held key repeats after 500 ms every 50 ms until released (`audio/roller.rs`). A `WH_KEYBOARD_LL` hook was tried first: Windows didn't call it while the Settings window was in front. The held key shows no Windows flyout; the island's volume notice is the feedback. |

---

## 3. Clipboard (history)

### 3.1 What it shows
Source: `NCBV:9-664`, header actions `NCBV:768-802`, card sizes `CCS:15-80`, keys and scroll helpers `NSup:1709-1782`.

* **Band.** The leading side shows a magnifier button (`text.search`) while the search field is hidden. The trailing side holds `NotchClipboardHeaderActions` (`NCBV:768-786`):
  * The pinned filter: `line.3.horizontal.decrease.circle` or `.fill`, selected when on. The state lives on the service (`clipboardPinnedOnly`), so it survives page switches.
  * "Clear recent" (trash), disabled when there are no recent entries. It confirms with "Clear %d recent items?" (`clearRecentConfirmFormat`), message `clearRecentConfirmMessage`, and a destructive button.
  * "Open history window" (`arrow.up.forward.app`): `perform { showHistoryWindow(preferNotch: false) }`.
* **Search field** (`NCBV:501-523`): 36 pt tall (`NSup:317`), horizontal padding 12, control surface radius 14, magnifier plus a 12 pt plain text field. A 1 pt white 0.34 border shows while focused (0.15 s). The field is **always mounted and focused**, even while hidden at height 0 and opacity 0, so the first keystroke goes into it and makes it visible.
* **List**: a vertical lazy stack with 8 pt spacing (`NSup:320`), scrolling vertically. Card size comes from `notchClipboardCardSize` (`compact` or `comfortable`; any unknown value means comfortable; default comfortable, `DEF:1547`).
  * **Comfortable** card: 104 pt tall (`NSup:318`). Padding is 10 horizontal, 10 top, 4 bottom. The preview is up to 3 lines, then a 28 pt action row: source app icon (14×14) or a kind glyph; date "MMM d, HH:mm" at 9.5 pt tertiary in the app language's locale; "⌘n" badge for places 1–9 (only while the island has key focus); then the action buttons: edit (pencil, images only, when the screenshot editor exists), copy (`doc.on.doc`, turning to `checkmark` for 1.6 s after a copy), pin/unpin, delete (`NCBV:437-473`).
  * **Compact** card: 52 pt tall (`NSup:319`). Padding 10 × 6. A 14 pt leading icon (file icon or kind glyph), then a 1–2 line preview. While hovered or highlighted the trailing side shows the action buttons; otherwise it shows a pin glyph, the "⌘n" badge and the time (`NCBV:381-418`).
  * **Compact open-on-dwell**: resting the pointer 0.7 s (`dwell`) on a compact card, or holding the arrow keys on it for 1.4 s (`keyDwell`), "opens" the entry. The full card is drawn as an overlay **on top of** the neighboring rows (`zIndex 1`) so rows never move under the pointer. It is as tall as its text (up to 8 lines × 16 pt + 48 pt chrome; text measured at 12 pt), its image (80–170 pt), or 104 for files. Its offset is clamped so the whole card stays inside the list while its row is visible; once the row scrolls out of the list, the card scrolls away with it instead of staying pinned (`NSup:1764-1773`). It gets an opaque backing (`Color(white: 0.09)`, or glass), a 0.5 pt white 0.14 hairline, a 1 pt outer black backing, and a shadow (black 0.5, radius 10, y 4) (`NCBV:247-264`, `357-378`). It closes 150 ms after the pointer leaves, unless the pointer returns to the same entry (`NCBV:302-332`). Open and close animate with a 0.18 s easeOut.
  * **Highlight** ring: 1 pt white 0.34 around the highlighted card. A pinned card uses the "selected" fill.
  * **Previews by kind** (`NCBV:606-663`):
    * Text: a 12 pt preview. If the whole entry is a color, a 12 pt `ColorSwatch` comes first. Search matches render **bold** (weight only, no accent color).
    * Image: a thumbnail rounded 6, max width 72 when compact. If it fails to load, it shows "Image · W×H".
    * Files: a single image file shows its thumbnail. Otherwise the file name (single file) or `fileCountFormat` ("%d files"), with middle truncation.
* **Empty states**: history disabled and empty shows `text.disabled` plus a bordered "Enable" button, which sets `clipboardHistoryEnabled = true` and calls `syncWithPreferences()` (`NCBV:66-77`). No entries shows `text.empty` with the `doc.on.clipboard` glyph. A filter or search with no matches shows `text.noResults` with the `pin` or `doc.on.clipboard` glyph.

### 3.2 Interactions
* On open: the highlight goes to the **most recently copied entry**, which is the first unpinned one, or the top entry when all are pinned (`NSup:1733-1735`). The list scrolls by whole rows only if that entry would show less than half a compact card (`NSup:1775-1782`). The search field takes focus.
* Keys (`NSup:1743-1762`; anything with Cmd, Ctrl or Option passes through):
  * Up / Down move the highlight without wrapping; the first press lands on the top entry.
  * Return / Enter activates the highlighted entry, or the first entry when the highlight is stale.
  * Any printable character, or IME composition, shows the search field and goes into it.
  * Shift + arrows and Shift + Return belong to the text field.
  * ⌘1–⌘9 activate the entry at that visible position. Physical key codes are used, so this works on every keyboard layout, numeric keypad included. An empty slot beeps (`NSup:94-105`, `NCBV:153-160`).
* **Activate** means paste if the app has the Accessibility permission, else copy (`NCBV:564-566`). **Paste** collapses the island, writes the entry to the clipboard, reactivates the previously frontmost app and sends ⌘V (`CHS:583-594`). **Copy** writes the entry and shows a check for 1.6 s; the list scrolls to the copied entry, which moves to the top. Copy ignores the second click of a double click (`NCBV:573-585`).
* The context menu offers Paste (with permission), Copy, Pin/Unpin, Move up, Move down (only with no search active), and Delete (`NCBV:477-490`).
* A search starts from its top result. Escape first clears and hides the search, keeping focus (`NCBV:525-536`). A field that loses focus while empty hides itself.
* Capture filters, set in the clipboard's Settings and applied by the service: the sensitive-text filter and the ignored-apps list (`ClipboardIgnoredApps`). Port both as settings with the same defaults.
* Preferences read here: `clipboardHistoryEnabled` (default **false**, `DEF:1785`), `clipboardHistoryLimit` (default 50; allowed values `[20,50,100,250,500,1000,10000,0=unlimited]`, `DEF:1202`), `clipboardHistoryRetentionDays` (default 0 = keep, `DEF:1787`), `clipboardHistoryIncludeImagesFiles` (default true, `DEF:1789`), `notchClipboardCardSize`. The clipboard notice ("copied") is routed when `notchClipboard` is set (default true, `DEF:1545`); that belongs to the notices spec.

### 3.3 Mac APIs used and the entry model
`ClipboardHistoryService`: `entries`, `filteredEntries(matching:)`, `recentEntries`, `copy(_:completion:)`, `copyQuickEntry`, `togglePin`, `remove`, `move`, `canMove`, `clearRecent`, `editImage`, `showHistoryWindow`, `syncWithPreferences` (`CHS:120-480`). It polls `NSPasteboard.changeCount` every 0.8 s and trims to the limit hourly (`CHS:648-667`).

Entry model (`CHSup:135-178`): `{id, text, copiedAt, pinnedAt?, kind: text|image|files, filePaths[], imageFile? (PNG name in the image store), imageHash (SHA-256), imageWidth, imageHeight, sourceBundleID?}`. The preview takes at most 2,000 characters, with tabs and newlines collapsed (`CHSup:192-219`). Re-copying identical content refreshes the existing entry instead of adding a duplicate (`matchesContent`). Privacy: content marked `org.nspasteboard.ConcealedType` is always skipped, and so are apps on the ignore list (`CHSup:1058-1075`). Size caps: 1 M characters of text (`CHSup:294`), 16 MB PNG / 64 MB raw image data and 100 copied files (`CHS:817-819`). The 24 MP limit at `CHSup:521` only downscales images before OCR; it does not reject entries.

### 3.4 Minimal Windows backend

| Capability | Windows 11 API |
|---|---|
| Change detection | `AddClipboardFormatListener(hwnd)` on a message-only window (`HWND_MESSAGE`) → `WM_CLIPBOARDUPDATE`. Use `GetClipboardSequenceNumber()` to drop duplicates and to ignore our own writes. No polling needed. |
| Read | `OpenClipboard` (retry about 5 × 20 ms, since other apps hold the clipboard briefly). `CF_UNICODETEXT`; images from the registered `"PNG"` format, else `CF_DIBV5`/`CF_DIB`, re-encoded to PNG (the `image` crate); files via `CF_HDROP` + `DragQueryFileW`. Hash the PNG with SHA-256. |
| Privacy (the equivalent of ConcealedType) | Skip the update when the clipboard carries the registered formats `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory` (with DWORD 0), or `Clipboard Viewer Ignore`. Password managers set these. Keep the app ignore list. |
| Source app ⚠️ | `GetClipboardOwner()` → `GetWindowThreadProcessId` → exe. The owner is often missing or hidden; fall back to `GetForegroundWindow()` at the time of the update. This is less reliable than the Mac's `org.nspasteboard.source`, so the badge is best effort. |
| Write | `OpenClipboard` / `EmptyClipboard` / `SetClipboardData` (`CF_UNICODETEXT`; `CF_DIB` + `"PNG"`; `CF_HDROP` via `DROPFILES`). Add a private registered format such as `Vorssaint.Source`, and record the resulting sequence number so the listener skips the write. |
| Paste into the previous app ⚠️ | Remember the foreground HWND **before** the island takes keyboard focus. On paste: write the clipboard, `SetForegroundWindow(prev)` (allowed because we are the foreground app at that moment), wait about 50 ms, then `SendInput` Ctrl+V. UIPI blocks `SendInput` into elevated windows when we aren't elevated, and neither the return value nor `GetLastError` reveals it. So check **before** sending: if the target process is elevated (`OpenProcess` + `GetTokenInformation(TokenElevation)` fails or reports elevated), copy instead and show the check. `SendInput` doesn't reset keys the user is still holding (Ctrl from Ctrl+1, Enter), so first send key-ups for any that `GetAsyncKeyState` reports down. No permission gate is needed (`permissions.accessibility` is effectively always true). |
| File icons | `SHGetFileInfoW` / `IShellItemImageFactory`, cached per path set (the Mac caches 256). |
| Storage | `%APPDATA%\Vorssaint\clipboard\history.json` plus PNGs in an `images\` folder. Write atomically (temp file + `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`). |
| History window, edit image | Out of the minimal scope: hide the "open window" button, or make it select this page. Hide pencil/edit unless a screenshot editor is ported. |
| Alternative ⚠️ | Windows' own clipboard history (`Windows.ApplicationModel.DataTransfer.Clipboard.GetHistoryItemsAsync`) exists, but it requires the user's Win+V history to be on and cannot pin or reorder on our behalf. Don't depend on it. |

---

## 4. Captures (recent captures + post-capture preview)

### 4.1 Page content
`NV:586-594` selects between two views:
* **A live capture preview** (`service.captureContent != nil`), shown after a screenshot. `ScreenshotQuickPreviewController.show()` calls `presentCapture(...)` (`SQP:115-130`, `NSvc:2535-2552`), which opens the island on `.captures`. The page scrolls vertically (`NV:507`).
  * The content is the screenshot thumbnail, at most 1,200 px on its long edge (`SQP:219-235`), drawn scaled-to-fit at max 320×138 pt, rounded 9 with a 1 pt primary 0.12 stroke (`SQP:669-700`). A click opens the editor (Edit). The image can be dragged out as a file.
  * The band trailing slot holds the **toolbar** (`content.toolbar`, `SQP:556-622`), 28×28 buttons with spacing 2: Discard (trash, "⌫"), [QR when a code was detected, popping in with a scale and opacity transition], [Copy text when OCR is available], Save (⌘S, disabled once saved), Copy (⌘C, disabled once copied), Share (system share sheet), [share-link menu when sharing is enabled], Pin, and Edit (pencil, prominent, "⏎").
  * When a link has been shared, a link row appears below the image with the URL, a relative expiry, copy and delete (`SQP:733-770`).
  * The preview auto-dismisses after `dismissInterval`, from `screenshotPreviewDuration` (default 3 s, `SSup:258`, `DEF:1938`). The preview itself is gated by `screenshotPreviewEnabled` (default true), and `screenshotPreviewTakesFocus` (default true) decides keyboard focus (`DEF:1932-1938`). Hovering pauses the dismissal (`SQP:175-183`). The policy decides whether the preview takes keyboard focus and whether it closes when the island collapses (`ScreenshotSupport.confirmationPreviewPresentationPolicy`).
  * QR detection runs in the background on the full-resolution image (`SQP:185-197`).
* **Recent captures** (`RecentCapturesView(notchSize:)`, `RCV:20-195`):
  * A rail of cards 236 wide, row height computed from the page height (base 88, spacing 8, `RCV:113-120`). Each card is a `panelCard()` HStack: a preview 104×68 rounded 7 (thumbnail or a `photo`/`film` glyph), then the title "Screenshot" / "Recording" at 11.5 pt semibold, the file name for recordings (9.5 pt secondary, middle-truncated), a relative time (9.5 pt tertiary), and buttons: prominent mini "Restore" (screenshot) or "Open" (recording), plus a trash mini button.
  * The empty state is `text.empty` at 10.5 pt tertiary inside a card filling the page.
  * The band trailing slot holds `NotchClearCapturesButton` (trash, disabled when there are no entries; confirms above the island, `NV:625-634`, `RCV:92-100`).
  * Screenshots show only when the screenshot feature is available, recordings only when the recorder is (`RCV:37-43`).

Entry model (`RCStore:6-28`): `{id, kind: screenshot|recording, createdAt, screenshotName?, recordingPath?, thumbnailName?, scale?, anchorX/Y/Width/Height?}`. "Restore" reopens the screenshot editor with the stored image; "Open" opens the recording file.

### 4.2 Minimal Windows backend

| Capability | Windows 11 API |
|---|---|
| Region selection overlay | One borderless, topmost, transparent Tauri window per monitor (or one spanning the virtual desktop), showing a frozen snapshot with a crosshair and drag-to-select. Use per-monitor DPI v2 coordinates. |
| Still screenshot | **DXGI Desktop Duplication** (`IDXGIOutputDuplication::AcquireNextFrame`) per output, or GDI `BitBlt` from the screen DC (simplest; misses some HDR and protected content). `Windows.Graphics.Capture` also works, but for stills it draws a yellow border unless `GraphicsCaptureSession.IsBorderRequired = false`, which needs `GraphicsCaptureAccess.RequestAccessAsync(Borderless)` (Win11). Prefer DXGI for stills. |
| Window capture ("Window" mode, if ported) | `Windows.Graphics.Capture` with `IGraphicsCaptureItemInterop::CreateForWindow`. |
| Keep the island out of captures | `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)` (Win10 2004+). Maps `notchShowInCaptures` (default true, `DEF:1551`; true means the island **appears** in captures; false sets the affinity). |
| Save / Copy / Share | Save as PNG to a folder (default `%USERPROFILE%\Pictures\Screenshots`). Copy: `CF_DIB` + `"PNG"`. Share: `IDataTransferManagerInterop::ShowShareUIForWindow(hwnd)` + `DataRequested` with a `StorageFile`. |
| Copy text (OCR) | `Windows.Media.Ocr.OcrEngine.TryCreateFromUserProfileLanguages()` → `RecognizeAsync(SoftwareBitmap)`. |
| QR detection | No built-in Win32 API. Use a Rust crate (`rqrr`, or `rxing` for more formats). |
| Edit ⚠️ | A full editor is out of scope. Minimal option: `ShellExecuteW(…, L"edit", path)` (opens Paint or the default image editor), or hide Edit. Restore from recents follows the same rule. |
| Pin (floating screenshot) | Optional: a small always-on-top Tauri window. Hide in the minimal port. |
| Share link (upload) | Out of scope (`screenshotSharingEnabled` feature): hide. |
| Screen recording ⚠️ | `Windows.Graphics.Capture` frames → Media Foundation `IMFSinkWriter` (H.264/MP4); system audio via WASAPI loopback (`AUDCLNT_STREAMFLAGS_LOOPBACK`); mic via WASAPI capture. This is a large job. Minimal port: hide the `recording` tool, tile and quick-panel item. |
| History store | `%APPDATA%\Vorssaint\captures\index.json` + PNG files + thumbnails (≈ 2× of 104×68). |

---

## 5. Capture controls (the island during a screen selection)

### 5.1 What it shows
Source: `NCCV:12-114`, layout `NSup:1232-1275`, service `NSvc:2079-2275`.

When a selection starts (screenshot, recording, text, color) and `routesCaptureControls` is on (`notchCaptureControls`, default true, `DEF:1451`, `NSup:2072-2075`), the island shows the controls. They wait **collapsed** (closed island size) out of the way of what's being captured, and expand when the pointer rests on them for 0.25 s (`updateCaptureControlsHover`, `NSvc:2177-2203`).

Expanded layout (VStack, spacing 12, white foreground):
* **Header row**, `NotchLayout.horizontalInset` (28) padding:
  * On the left, the title `screenshot.screenCaptureTitle` at 12 pt semibold, one line.
  * On the right, the actions, spacing 6: an optional "R" key badge (`rectangle.dashed` + "R", or "R" alone when narrow; 10 pt semibold rounded, secondary, 26 pt tall, white 0.06 rounded 7) when "repeat last region" is offered; `chevron.up` (Collapse); `xmark` (Close / cancel the capture).
  * Without a camera, one row holds the title, a spacer and the actions (`NCCV:59-65`).
* **Tool tiles**: one `NotchActionTile` per available tool (or only the selected tool when the capture menu is off), HStack spacing 6, horizontal padding 18. Symbol and title come from the tool; the selected tool is active (white circle).
* With the recording tool selected: two small green switches, "System audio" and "Microphone" (spacing 16, `NCCV:98-114`).
* Window size: `headerTop + headerHeight + 12 + 74 + 16 (+ 40 with audio)` (`NSup:1268-1272`).

### 5.2 Behavior
* Tools are `screenshot` (key 1), `recording` (2), `text` (3) and `color` (4) (`SSup:12-30`). The digit keys choose a tool while selecting.
* The island leaves its normal window level, rising above the shielding level (`CGShieldingWindowLevel()+1`), and takes key focus (`NSvc:2108-2112`). A Command Bar open in the island is closed.
* Collapse rules (`NSvc:2160-2205`): 3 s after the last interaction while the pointer is away, or 0.18 s after the pointer leaves. A focused control or open menu keeps the controls open. They collapse immediately when an actual drag-selection starts.
* **Click-through**: only the visible controls catch the mouse; everywhere else, clicks fall through to the selection surface below (`NSvc:2223-2253`).
* Escape or Close calls the capture's cancel callback. When the selection ends, `endCaptureControls()` restores the normal level and focus.

### 5.3 Minimal Windows backend
* All of this is UI and window state on top of the selection overlay from §4.2. Needed pieces:
  * **Z-order**: the island must sit above the full-screen selection overlay windows. Create the overlays first, then call `SetWindowPos(island, HWND_TOPMOST)` again; both are topmost, and the most recently raised wins.
  * **Click-through outside the controls** ⚠️: Tauri's `set_ignore_cursor_events(bool)` maps to `WS_EX_TRANSPARENT`. While the window ignores the cursor, the webview receives no mouse moves, so the decision cannot be made in JS. Poll `GetCursorPos` at about 60 Hz in Rust during the selection, or use a `WH_MOUSE_LL` hook, and toggle ignore-cursor based on the controls' rectangle. An alternative is a window region (`SetWindowRgn`) covering only the controls.
  * The "repeat last region" option needs the last selection rectangle persisted per monitor.

---

## 6. Files (shelf + file tools)

### 6.1 What it shows
Source: `NFV:8-321`, tiles `STV:89-700`, service `SS`, tools `NFTS:14-178`, `NFTSup:8-125`, drop flow `NSvc:2296-2345`.

States (`NFV:41-84`):
1. **Choosing a drop destination**: while a drag that holds only media (images, or one video) is over the island and Media Tools exists. Two equal tiles sit side by side, spacing 12, each radius 20: "Files" (`tray.and.arrow.down`) and "Optimize media" (`wand.and.stars`). Each has a 28 pt light glyph and a callout-medium title. Selected: white 0.14 fill with a 1.5 pt white 0.7 border; otherwise white 0.04 fill with a 0.75 pt white 0.2 border. Optimize is disabled at 0.5 opacity while a job runs ("Running"). The right half of the content area is the media drop target (`NFTSup:11-16`).
2. **Media workspace** (compact `MediaWorkspaceView`) for an optimization session. Heavy; see 6.4.
3. **Empty shelf**: `NotchEmptyView(tray.and.arrow.down, notch.dropHint)` inside a dashed rounded rectangle (radius 20, 0.75 pt, dash [4,5], white 0.18).
4. **Tiles** (`ShelfTilesView(sideways: true)`, an AppKit view):
   * Tiles are 78×88, spacing 10, inset 4 (`STV:103-105`). Rows fill the height and the rail scrolls horizontally with no visible scroller.
   * Each tile has radius 10 and contains an icon well (7,6) of 64×50 (radius 8, white 0.06). The image sits inside the well, inset 4 for a real thumbnail or 13×8 for a generic icon. The title label is at (3,59), 72×24, 10 pt, centered, two lines, middle truncation, secondary color (`STV:368-420`).
   * A **batch** ("pile") adds two backplates offset +3 and +6 (white 0.035 / 0.055 with a 1 pt white 0.05 border), a count badge (22×15, 9 pt bold, accent capsule) and an expand chevron button.
   * **Selected**: accent 0.16 fill, 2 pt accent border, and a `checkmark.circle.fill` badge.
   * **Pinned**: a `pin.circle.fill` accent badge at the top right. On hover it is replaced by the remove button, `xmark.circle.fill` 17×17 (`STV:422-455`, `520-532`).
   * A hover tooltip popover shows the name plus the file kind, the text (up to 500 characters), the URL, or the pile breakdown with plural forms (`STV:458-490`, `SSupp:343-370`).
5. **Archive status** below the content: progress lives in the band. Afterwards: a failure in orange caption text (3 lines), or "Saved ✓" in green plus "Open in Finder", or "Cancelled". "Return to media" appears when a media session exists (`NFV:243-260`, `75-82`).

Band (`NFV:87-95`):
* **Leading**: the archive progress (spinner + "Create ZIP · n/m", or just "n/m" when narrow, plus a cancel ×), or the hint: `s.shelfHint` (10 pt secondary, 2 lines) with nothing selected, else "%d selected" (`shelfSelectedFormat`, 12 pt).
* **Trailing** (only when tiles show): the media actions (`wand.and.stars`; a 270-wide popover with "Create ZIP" + hint, plus a list of the media tools that accept the selection); Share (`square.and.arrow.up`); then either a trash menu ("Clear All" destructive / Cancel) with nothing selected, or a `trash.fill` "Remove selected" button.

### 6.2 Interactions
* Tile mouse handling (`STV:600-628`): a click toggles selection; Shift+click extends it; a double-click on a pile expands or collapses it; dragging more than 4 pt starts a native drag of the selected items (or just this one). Payloads whose file disappeared are dropped from the drag, and an all-dead drag explains itself (`STV:687-700`). Tiles accept the first click even when the island isn't key.
* Context menu (`STV:533-597`): Pin/Unpin; then, for file payloads, Open, Open With ▸ (apps common to all selected files, up to 40, sorted by name, with icons), Edit (one editable image), Share ▸ and Reveal in Finder.
* **Drop onto the island** (`NSvc:2296-2345`): when Files is enabled (`modules ∋ files`, `shelfEnabled`, `notchShelf` default true, `DEF:1449`), a drag entering the island opens the Files page with `takeFocus: false`. If `notchDragReveal` is on (default true, `DEF:1450`), the closed island first shows a drop hint with the mascot watching the file (`NV:111`). Dropping adds the items to the shelf, or starts media optimization on the right half. Then the page opens and the mascot "celebrates" with a patience of 30 s.
* **ZIP** (`NFV:271-319`, job driver `NFTS:133-177`, `ditto` invocation and cancellation `NFTSup:57-124`): with one input, a save dialog prefilled with a unique `<name>.zip` in the same folder; with several, a folder picker. Each item becomes its own ZIP (via `ditto -c -k --sequesterRsrc [--keepParent]`). The destination must not lie inside an input (checked by file identity). Outputs are written to a staged temp file, never overwrite existing files, and are added to the shelf. Cancel terminates the job (SIGKILL after 0.5 s).
* Shelf items are files, text, links or batches (`SS:22-80`), persisted under App Support `ShelfFiles` (`SS:256-260`).

### 6.3 Mac APIs used
`ShelfService`: `items`, `visibleItems`, `selection`, `expandedBatches`, `pinnedIDs`, `addFiles`, `acceptDrop(pasteboard:)`, `removeItems`, `removeItem`, `clear`, `toggleSelection`, `extendSelection`, `toggleItemPin`, `toggleBatchExpansion`, `fileURLsForActions`, `hasFilesForActions`, `dragItems`, `livingDragItems`. `NotchFileToolsService`: `archive`, `cancel`, `isRunning`, `completed/total`, `outputURLs`, `failure`, `wasCancelled`, media session state.

### 6.4 Minimal Windows backend

| Capability | Windows 11 API |
|---|---|
| Drop files onto the island | Tauri window drag-drop events (`DragDropEvent::Enter/Over/Drop/Leave` with paths and a physical position); `dragDropEnabled` is **on by default** for Tauri windows. A collapsed island that is click-through (`WS_EX_TRANSPARENT`) is invisible to OLE drag hit-testing, so keep the closed island's drop area hit-testable. If the app runs elevated, UIPI blocks drops from Explorer. ⚠️ With this enabled, Tauri on Windows **disables HTML5 drag-and-drop inside the webview**, so every in-island reorder (mixer columns, quick-panel tiles, scratchpad tabs) must use pointer events, not HTML5 DnD. |
| Drop text or URLs ⚠️ | Tauri reports file paths only. Accepting text and link drops needs a custom `IDropTarget` (`RegisterDragDrop`) on the island HWND, in place of WebView2's external-drop handling (`ICoreWebView2Controller4::put_AllowExternalDrop(FALSE)`). Minimal port: files only. |
| Drag tiles out to Explorer or apps ⚠️ | A native `DoDragDrop` with a shell `IDataObject` (`SHCreateDataObject` or `IShellItemArray::BindToHandler(BHID_DataObject)`), started from Rust on the UI thread when JS reports a drag start. The `drag` crate (CrabNebula `drag-rs`, used by `tauri-plugin-drag`) does this on Windows. drag-rs only reports `Dropped` or `Cancel`. Explorer's optimized move returns `DROPEFFECT_NONE` even after moving the file; the real result is in `CFSTR_PERFORMEDDROPEFFECT` on the data object. "Remove once the drop is accepted" therefore needs a custom `DoDragDrop` that reads it, or a check that the file still exists afterwards. |
| File icons and thumbnails | `IShellItemImageFactory::GetImage(SIIGBF_RESIZETOFIT)` at 64×50 @ DPI, or `SHGetFileInfoW` for the generic icon. Kind string: `SHGetFileInfoW(SHGFI_TYPENAME)`. |
| Open / Open With / Reveal | `ShellExecuteExW` ("open"); `SHAssocEnumHandlers(ext, ASSOC_FILTER_RECOMMENDED)` for the Open With list (intersected across files), launched with `IAssocHandler::Invoke`; `SHOpenFolderAndSelectItems` for "Reveal in File Explorer". |
| Share | `IDataTransferManagerInterop::ShowShareUIForWindow` with the `StorageFile`s. |
| ZIP | The Rust `zip` crate (Deflate), written to a temp file then renamed with `MoveFileExW`, never replacing. Directories are added recursively with the parent folder kept. A Windows identity check uses `GetFileInformationByHandle` (volume serial + file index) to refuse a destination inside an input. Cancel uses an atomic flag checked per entry. |
| Save dialog / folder picker | `tauri-plugin-dialog` (`save` with `.zip` filter, `open({directory:true})`), parented to the island window. Keep the island open while the dialog is up (`keepFileInteractionOpen`). |
| Shelf persistence | `%APPDATA%\Vorssaint\shelf\items.json` + a `files\` folder for pasted images and text. Windows has no bookmark data: store the path, and optionally the file ID (`FILE_ID_INFO`) to re-find a moved file on the same volume ⚠️ (best effort). |
| Media tools (optimize, GIF, video) ⚠️ | Out of the minimal scope: they need ffmpeg or Media Foundation transcoding. Set `AppFeature.mediaTools` unavailable, which removes the destination chooser, the wand button and "Return to media". The ZIP action then needs a home: keep the wand popover with only "Create ZIP", or move ZIP into the context menu. See Open questions. |

---

## 7. Tools (quick panel in the island)

### 7.1 What it shows
Source: `QLV:6-603`, items `QLS:10-43`, `run` `QLS:271-330`.

* **Band.** The trailing side holds the customize toggle (`slider.horizontal.3`, or `checkmark` while editing; selected when editing; 0.15 s easeOut). While a hosted utility is open, the leading side holds a back button titled with the utility's name (`NotchBandBackButton`) (`QLV:60-72`).
* **Grid, normal mode**: a `NotchRail` of cells 76×72, spacing 6, least spacing 0 (`NSup:303-311`). Where the gaps close, titles get extra inset so neighboring titles don't touch (`NSup:538-542`).
  * **Cell** (`QLV:242-347`): a 32×32 rounded square (radius 16) filled white 0.065, or 0.14 when hovered or keyboard-selected, holding a 16 pt semibold glyph (primary 0.85; accent when active; red for a muted mic). An active tool gets an 8 pt green dot at (+3, −3). The title is 10 pt medium, centered, 2 lines with space reserved. Vertical padding 4, no card background in the island.
  * Hovering a cell selects it; keyboard arrows move `keyboardIndex` and the rail scrolls to it.
* **Edit mode**: a 5-column grid with spacing 6 (`NSup:1659`) and a 9.5 pt secondary hint above it. Each cell gets a red `minus.circle.fill` at (+7, −7) that hides it, and a gear badge when it has options. Tapping a cell with options opens the **options card** (black background, radius 12, padding 10, 11 pt, small switches):
  * keepAwake: closed-lid toggle and default duration picker (15m/30m/1h/2h/4h/8h/indefinite).
  * micMute: menu-bar badge toggle.
  * clipboard: enable toggle, plus a limit picker when enabled.
  * colorPicker: segmented format picker, plus a bare-hex toggle for hex.
  
  Below the grid sits the **hidden tray**: "ADD" in uppercase (9 pt bold, tracking 0.5) followed by flow-wrapped capsule chips (black) "⊕ Title" (`QLV:349-374`, `415-492`).
* **Empty**: `s.launcherEmptyState` at 11 pt secondary, 26 pt vertical padding.
* **Hosted utilities**: `homebrew`, `media`, `urlCleaner`, `uninstaller`, `cleaner`, `windowLayout` and `toggles` render inside the page in a vertical scroll view at the page height (`QLV:103-128`).

### 7.2 Items, order and preferences
`QuickLauncherItem` default order (`QLS:14-16`): `keepAwake, cleaner, toggles, micMute, screenOCR, colorPicker, clipboard, windowLayout, cleaning, homebrew, media, urlCleaner, uninstaller, screenshot, screenRecorder, cameraPreview, scratchpad`. Order: `quickLauncherItemOrder`; hidden: `quickLauncherHiddenItems` (CSV, default `""`). An item shows only while its feature is available. The island page needs `notchQuickPanel` (default true) and the `tools` module.

`run(item)` (`QLS:271-330`) is skipped while editing. keepAwake and micMute toggle in place. OCR, screenshot, recorder and color picker hide the panel and start after 0.15 s. Camera opens inside the island if possible, else floating. Scratchpad, clipboard window and cleaning mode open after a delay. Utilities open hosted.

| Item | Symbol | Windows minimal |
|---|---|---|
| keepAwake | `bolt` / `bolt.fill` | ✅ §1.3 |
| toggles | `togglepower` | ✅ §7.3 |
| micMute | `mic` / `mic.slash.fill` | ✅ §1.3 |
| screenOCR | `text.viewfinder` | ✅ selection overlay + `Windows.Media.Ocr` |
| colorPicker | `eyedropper` | ✅ magnifier loupe overlay + `GetPixel` / a DXGI frame; formats hex/rgb/hsl… copied to the clipboard |
| clipboard | `doc.on.clipboard` | ✅ opens the clipboard page (or the history window if ported) |
| screenshot | `camera.viewfinder` | ✅ §4.2 |
| screenRecorder | `record.circle` / `stop.circle` | ⚠️ optional (§4.2) |
| cameraPreview | `web.camera` | ✅ §9 |
| scratchpad | `note.text` | ✅ §11 |
| cleaning (keyboard lock for cleaning) | `keyboard` | ⚠️ feasible: a `WH_KEYBOARD_LL` hook swallowing keys plus an overlay; Ctrl+Alt+Del cannot be blocked. Out of minimal scope. |
| windowLayout | `rectangle.3.group` | ⚠️ separate feature (Snap layouts exist natively); hide |
| homebrew | `shippingbox` | ⚠️ no equivalent (winget could stand in); hide |
| media | `photo.on.rectangle.angled` | ⚠️ needs ffmpeg; hide |
| urlCleaner | `link` | feasible (pure string rules on clipboard URLs); out of minimal scope |
| uninstaller | `trash` | ⚠️ no equivalent; hide |
| cleaner | `sparkle` | ⚠️ Mac-specific cleaner; hide |

### 7.3 Quick Toggles (hosted utility)
Source: `QTS:8-389`, list `QTSec:35-120`. A header (`togglepower` + title at 12 pt semibold, close ×) above a reorderable list of rows. Each row shows its own state: running (disabled), failed, or needs-permission. Actions, in default order (`QTS:11-12`), with per-row visibility keys `panelToggle*` (default true) and order `panelToggleOrder` (CSV, empty = default order). Rows use the menu panel's row component (`PanelRowGroup`, specified with the app panel), not island-specific metrics:

| Action | Mac implementation | Windows 11 API |
|---|---|---|
| darkMode | WindowServer appearance switch (private SkyLight symbol) | Set `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize` → `AppsUseLightTheme` and `SystemUsesLightTheme` (DWORD 0/1), then `SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, L"ImmersiveColorSet", SMTO_ABORTIFHUNG, 100)`. Most apps follow at once; some apps and Explorer parts lag ⚠️. |
| keyboardLight | BrightnessService | ⚠️ none; hide |
| micMute | MicMuteService | §1.3 |
| emptyTrash | Confirm, then AppleScript to Finder | `SHEmptyRecycleBinW(hwnd, NULL, SHERB_NOCONFIRMATION \| SHERB_NOUI)` after our own confirmation. Count with `SHQueryRecycleBinW` first: an empty bin probably makes the call return an error (unverified), so treat "already empty" as success. |
| ejectDisks | `NSWorkspace.unmountAndEjectDevice` | Enumerate removable and USB volumes (`GetDriveTypeW`, or `IOCTL_STORAGE_QUERY_PROPERTY` `BusType == BusTypeUsb`), then `CM_Request_Device_EjectW` on the device's `DEVINST` (via `CM_Get_Parent` from the volume's disk), **passing a `pVetoType` buffer**. Without one, Windows shows its own "Problem Ejecting" dialog. Excluded volumes list kept. ⚠️ Fails while files are open (report "failed"). |
| hiddenFiles | Finder pref + Finder restart | Documented route: `SHGetSetSettings(&state, SSF_SHOWALLOBJECTS, TRUE)` (stored in `HKCU\…\Explorer\Advanced\Hidden` = 1 show / 2 hide). Then refresh open Explorer windows (`IShellWindows` → each view's `Refresh`, or send F5). `SHCNE_ASSOCCHANGED` alone does not refresh views. No Explorer restart needed. |
| desktopIcons | Finder `CreateDesktop` + restart | ⚠️ Toggle with `SendMessage(SHELLDLL_DefView under Progman/WorkerW, WM_COMMAND, 0x7402, 0)`, which flips "Show desktop icons" (undocumented command ID). Read the state from `HKCU\…\Explorer\Advanced\HideIcons`. |
| lockScreen | private `SACLockScreenImmediate` | `LockWorkStation()` |
| displayOff | `pmset displaysleepnow` | `PostMessageW(HWND_BROADCAST, WM_SYSCOMMAND, SC_MONITORPOWER, 2)` about 500 ms after the click, or the pointer movement of the click wakes the screen at once. ⚠️ On Modern Standby laptops this can put the machine into standby. |
| screenSaver | open ScreenSaverEngine | `SendMessageW(GetDesktopWindow(), WM_SYSCOMMAND, SC_SCREENSAVE, 0)`; does nothing if no screen saver is configured ⚠️ |

---

## 8. Quick Access (side buttons around the open island)

Source: `NQAV:9-282`, layout `NSup:1538-1598`, config `NSup:1418-1470`.

* Up to 3 circular buttons per side (`left`, `right`, `bottom`), **44 pt** in diameter, black fill with a 0.75 pt white 0.16 ring (0.6 when selected or in high contrast), and a 17 pt medium glyph. Each button joins the island through a black "drop" with a neck that dissolves (`NotchQuickAccessDrop`, `NQAV:224-260`).
* Geometry: gap 12 from the island's edge, row spacing 54. A column is vertically centered on the body when the page is short, else aligned to the header (`NSup:1573-1589`). The hover corridor covers the column with a 16 pt margin; clicks hit-test the exact circles.
* Motion: reveal with a 0.38 s spring (bounce 0.12), optionally delayed; withdraw with a 0.16 s easeIn. Progress drives scale (0.4 → 1), opacity and position: center offset = −14 + p·(14 + 12 + 22) outward. Buttons become interactive only after the reveal completes.
* Default layout (`NSup:1422-1428`): left = Explore (sections) and Timer; right = Settings and Mixer; bottom = Music. The configuration is stored as JSON in `notchQuickAccessLayout` (default empty `Data()`, then legacy keys, then `initial`). Labels are trimmed to 40 characters and newlines removed (`sanitized`).
* An action is `explore | settings | pin | module(m) | control(c)`, with id strings `"explore"`, `"pin"`, `"<module>"`, `"control.<item>"`. Selected state: explore while the sections view shows; pin while pinned; keepAwake, mic and recording by their live state; a module while it is the current page. Title and symbol swaps: explore turns into "Back" (`arrow.uturn.backward`) while sections show; pin becomes `pin.fill` / "Unpin"; mic becomes `mic.slash.fill` with the unmute title; keepAwake becomes `cup.and.saucer.fill`; recording becomes `stop.circle.fill`.
* Glass variant: only on macOS 26 with `notchLiquidGlassEnabled`, so it doesn't apply on Windows; always use the solid black drops.

**Windows**: pure UI. The buttons extend outside the island body, so the transparent island window must be large enough to contain them, with the hit region limited to the circles: WebView2 transparent background plus `set_ignore_cursor_events` toggling as in §5.3, or a window region (`SetWindowRgn`). `WM_NCHITTEST` → `HTTRANSPARENT` does **not** work: it only passes clicks to windows of the same thread, and the mouse goes to WebView2's child HWND anyway.

---

## 9. Camera (mirror before a call)

Source: `NCamV:7-84`, `CPS:13-360`, `CPV:10-194`, `CPSup:15-26`.

* **Idle page** (the camera isn't running): an HStack with spacing 16 and horizontal padding 12. On the left, a 56×56 tile (white 0.05, radius 18) with `web.camera` at 30 pt light, secondary. On the right, `notchActivities.cameraHint` (callout, secondary) and a prominent "Start camera" button (`NCamV:20-32`). The page is at least 144 pt tall (`NV:487-489`).
* **Running page**: `CameraPreviewView(size: cameraPreviewSize(page), showsCameraMenu: false)` (`NCamV:18`). The preview fills the page, cropped to at most 16:9 on wide pages and 4:3 on tall ones: `w = min(W, H·16/9)`, `h = min(H, W·3/4)` (`NSup:363-366`). Black background, corner radius 14, 1 pt white 0.14 stroke (`CPV:28-35`), video **mirrored** and aspect-fill (`CPV:151-182`).
* States (`CPV:43-68`): idle, waitingPermission or starting show a small white spinner. Denied shows `video.slash` (26 pt light, white 0.55), a caption, and "Open Settings". Unavailable shows "Start camera" (retry). noCamera shows a message only.
* **Band**: the leading side holds `NotchCameraChooser`, a menu of cameras showing the current name, visible only while running with more than one camera. The trailing side holds `NotchCameraStopButton`: a prominent "Stop camera" pill without a camera housing (`video.slash` icon beside a camera) (`NCamV:43-84`, `NV:574-576`).
* Lifecycle: the capture session exists only while the page shows. Leaving the page (`onDisappear`) calls `hideEmbedded()` and stops the camera, so its light goes off immediately. The embedded camera and the floating mirror never capture at the same time. The camera choice is remembered by the system (`AVCaptureDevice.userPreferredCamera`), and only for explicit picks (`CPS:257-268`). The page needs `notchCameraEnabled` (default true, `DEF:1472`), the cameraPreview feature, and the `camera` module.

**Windows**:

| Capability | API |
|---|---|
| Live preview | `navigator.mediaDevices.getUserMedia({video:{deviceId, width:{ideal:1280}}})` in WebView2, a `<video>` with `transform: scaleX(-1)` and `object-fit: cover`. Release by stopping all tracks on page exit; the camera LED turns off once the last track stops. |
| Device list and change events | `enumerateDevices()` (labels appear only after permission) plus `devicechange`. |
| Permission prompt ⚠️ | WebView2 shows its own permission prompt for the app origin. Handle `ICoreWebView2::PermissionRequested` in Rust (via `with_webview` → `ICoreWebView2`) and allow `COREWEBVIEW2_PERMISSION_KIND_CAMERA` for our origin only, so the island never shows a browser-style prompt. A camera blocked in Windows settings (Privacy & security → Camera) fails with `NotAllowedError`, which maps to the "denied" state; "Open Settings" launches `ms-settings:privacy-webcam`. |
| Remember the camera | Store the `deviceId` in app settings (it is stable per origin); fall back to the first device. |
| "In use by another app" | `NotReadableError`, which maps to "unavailable" with retry. |

---

## 10. Mirror (closed island copies on other displays)

The file name is misleading: this is **not** the camera. `NotchMirrorView` (`NMirV:8-68`) draws the **closed** island on every other display when `notchDisplay` is `all` (values `automatic, builtIn, main, pointer, all`; default `automatic`, `DEF:1515`; `NSup:110-114`). Each copy has exactly the size and geometry the closed island would have on that display (`mirrorSurface`, `NSvc:3158-3167`).

* It renders the same closed strip as the main island: the activity strip (timer, watch, downloads, agents, sessions, calendar, music, keepAwake) or the resting view, laid out with that display's geometry (`NMirV:24-67`). The mascot can visit over the strip (`NMirV:29-34`). It is dark scheme, white tint, takes no hits and is hidden from accessibility.
* Model: `NotchMirrorModel {geometry, strip, size, activity}` with change detection (`NSup:881-910`). One window per extra display (`NSvc:3176-3183`). A click anywhere on a copy runs `bringIsland(to:)`: the island collapses where it was, moves to that display and opens there, or opens the calendar countdown when that activity shows (`NSvc:3208-3218`). Copies hide on displays running something full screen when `notchHideInFullscreen` is on (default false, `DEF:1523`). Copies follow the capture-sharing setting (`sharingType`, `NSvc:3140`).

**Windows**: `EnumDisplayMonitors` + `WM_DISPLAYCHANGE`. One small topmost, `WS_EX_NOACTIVATE` Tauri window per extra monitor, rendering the same React closed-strip component from shared state (broadcast via Tauri events). Detect a full-screen app on a monitor with `SHQueryUserNotificationState` (`QUNS_RUNNING_D3D_FULL_SCREEN` / `QUNS_PRESENTATION_MODE`; this only reports the primary state ⚠️), or compare the foreground window rect with `MONITORINFO.rcMonitor` per monitor. Apply `WDA_EXCLUDEFROMCAPTURE` to match the capture setting.

---

## 11. Scratchpad

Source: `NSPV:14-333`, model `SPSup:9-130`, `505-545`.

* **Page**: a VStack with spacing 6:
  * Optional format bar (28 pt) when `marksExpanded`.
  * An optional save-failure label (10.5 pt, white 0.6, ⚠).
  * The editor surface (`NotchControlSurface`, radius 12, not interactive-styled): a plain-text editor (`PlainTextEditor`) at `scratchpadTextSize` pt (default 13, range 10–22, `SPSup:219-225`), white text and caret, inset 6×6. It supports find (Ctrl+F, Ctrl+G, Shift+Ctrl+G), task boxes `- [ ]`, list continuation, and Ctrl+Return to toggle the preview. While empty it shows a placeholder at white 0.35.
  * In **preview** mode: a Markdown rendering (`MarkdownPreview`) whose task boxes can be clicked, which edits the underlying text through the editor's undo.
* **Band, leading** (`NSPV:162-178`): without a camera, a horizontal tab strip (spacing 4, 28 pt tall, auto-scrolled to the selected tab) plus a "+" button (disabled at **12 pads**, with the tooltip "Up to %d pads").
  * Tabs are 22 pt capsules: name at 11 pt medium; selected = white text on white 0.14 fill with a `chevron.down` (8 pt) and opens the pad menu; unselected = white 0.6 on 0.05 fill, showing a close × (14×14) on hover when there are at least 2 pads (`NSPV:239-292`).
  * Pad menu (`NSPV:206-234`): (list of pads when beside a camera), New pad, Rename, Close pad; Export… (save .txt named `"<Pad> yyyy-MM-dd.txt"`), Clear (undoable); "Open floating pad". The context menu on a tab offers Rename and Close.
* **Band, trailing** (`NSPV:180-198`): Format marks (`textformat`, selected while expanded; disabled in preview), Preview / Edit (`eye` / `pencil`; disabled when empty), Copy all (`doc.on.doc`, then `checkmark` for 1.6 s; disabled when empty).
* Behavior:
  * New and selected pads put the caret at the end of the text.
  * Closing a non-empty pad asks for confirmation ("Delete “%@”…"); an empty pad closes at once (`SPSup:519-521`).
  * Rename opens a dialog with a 240-wide field; names are capped at 40 characters (`SPSup:69-71`).
  * New pads are named "<Scratchpad> n" (`nextPadName`).
  * Text saves on edit (debounced 0.8 s, `ScratchpadService`) and commits when the page disappears.
  * Retention: `scratchpadRetention` (never, day, week or month; default never) clears text not edited within the period, checked when the pad opens.
  * Ctrl+T and Ctrl+W (new and close pad) are routed by the service (`scratchpadCloseSerial`).
* Motion: the format bar opens and closes with a 0.15 s easeOut (`NSPV:185`); the tab strip scrolls the selected tab to center with a 0.15 s easeOut (`NSPV:147-150`); the Copy check reverts after 1.6 s (`NSPV:126-131`). Nothing else animates.
* Storage: `Scratchpad.json` = `{pads:[{id,name,text,modifiedAt?}], selectedID}` (`SPSup:57-120`).
* Routing: the island page needs `notchScratchpad` (default true, `DEF:1456`) and the `scratchpad` module; otherwise the shortcut opens the floating pad.

**Windows**: pure frontend plus file I/O. Use a `<textarea>`, or CodeMirror 6 for find, undo and list continuation (closer to `NSTextView` behavior), and a Markdown renderer (`markdown-it` with a task-list plugin) whose checkbox clicks edit the source text. Persist `%APPDATA%\Vorssaint\Scratchpad.json` with atomic writes. Export through `tauri-plugin-dialog save` + `fs`. Copy all: clipboard write, which must also be ignored by our clipboard listener (§3.4).

---

## 12. Mascot (the companion)

The mascot is **not a sprite sheet**. It is drawn procedurally from vector shapes and animated with Core Animation keyframes, so the port is data plus a few pure functions, rendered as SVG in the webview with the Web Animations API. (The pixel-art "Clawd" buddies are a separate feature in the sessions spec.) Source: `NMS` (model, geometry, motion), `NMasV` (rig and animation), settings `UI/Settings/NotchMascotSettings.swift`, scheduling `NSvc:4434-5060`.

### 12.1 Preferences (`DEF:956-964`, `1554-1562`)

| Key | Values | Default |
|---|---|---|
| `notchMascotEnabled` | bool | false (a one-time beta migration turns it on, `DEF:2220-2229`) |
| `notchMascotVisits` | bool, strolls now and then | true |
| `notchMascotReactions` | bool, reacts to island events | true |
| `notchMascotStyle` | `minimal` \| `robot` | minimal |
| `notchMascotShape` | `ball` \| `egg` \| `squircle` \| `pill` (minimal only) | ball |
| `notchMascotPalette` | `pearl, lemon, peach, rose, lilac, sky, mint` | pearl |
| `notchMascotSide` | `left` \| `right` (beside the camera; a capsule rests in the middle) | left |
| `notchMascotVisitFrequency` | `rare` (600–1200 s), `normal` (240–540 s), `frequent` (90–180 s) | normal |
| `notchMascotHidesWhenIdle` | bool, tucks away after 30 s idle | true |
| `notchCommandBar` / `notchCommandBarStyle` | the bar comes out of the island: `droplet` \| `island` | true / droplet |

Enabled = feature available, island on, and `notchMascotEnabled` (`NMS:1233-1236`).

### 12.2 Look: a data format proposal (`mascot.json`)
All coordinates are **shares of the mascot's square** (0…1), y pointing down, origin top-left. This is the exact data from the Swift source:

```jsonc
{
  "colors": {                        // sRGB 0..1 (NMS:27-40)
    "ink":   [0.07, 0.07, 0.08, 1],  // minimal style's eyes
    "visor": [0.09, 0.09, 0.11, 1],  // robot face plate
    "shine": [1, 1, 1, 0.55]         // body highlight (minimal only)
  },
  "palettes": {                      // body gradient top (light) -> bottom (shade), (NMS:44-74)
    "pearl": {"light": [0.99,0.98,0.96], "shade": [0.84,0.85,0.90]},
    "lemon": {"light": [1.00,0.96,0.64], "shade": [0.97,0.83,0.30]},
    "peach": {"light": [1.00,0.86,0.74], "shade": [0.98,0.64,0.50]},
    "rose":  {"light": [1.00,0.80,0.87], "shade": [0.95,0.54,0.68]},
    "lilac": {"light": [0.88,0.83,1.00], "shade": [0.69,0.59,0.96]},
    "sky":   {"light": [0.77,0.90,1.00], "shade": [0.43,0.67,0.97]},
    "mint":  {"light": [0.72,0.97,0.86], "shade": [0.40,0.83,0.66]}
  },
  "bodies": {                        // NotchMascotGeometry.body (NMS:356-387)
    "ball":     {"ellipse": [0.08, 0.12, 0.84, 0.84]},
    "egg":      {"path": "M.5 .10 C.71 .10 .86 .36 .86 .60 C.86 .80 .70 .96 .5 .96 C.30 .96 .14 .80 .14 .60 C.14 .36 .29 .10 .5 .10 Z"},
    "squircle": {"superellipse": {"cx": 0.5, "cy": 0.55, "rx": 0.43, "ry": 0.40, "exponent": 3.4, "samples": 72}},
                 // x = cx + rx·sign(cosθ)·|cosθ|^(2/3.4), y = cy + ry·sign(sinθ)·|sinθ|^(2/3.4), polyline closed
    "pill":     {"roundedRect": [0.05, 0.28, 0.90, 0.60], "radius": 0.30}
  },
  "robot": {                         // NMS:413-429; fill = palette gradient on head, ears/antenna = shade
    "head":    {"roundedRect": [0.12, 0.26, 0.76, 0.68], "radius": 0.20},
    "ears":    [{"roundedRect": [0.05, 0.50, 0.08, 0.20], "radius": 0.035},
                {"roundedRect": [0.87, 0.50, 0.08, 0.20], "radius": 0.035}],
    "antenna": {"roundedRect": [0.475, 0.12, 0.05, 0.16], "radius": 0.025},
    "bulb":    {"ellipse": [0.43, 0.04, 0.14, 0.14], "glowRadius": 0.09},  // lit = palette.light, asleep = shade
    "visor":   {"roundedRect": [0.20, 0.44, 0.60, 0.38], "radius": 0.15},
    "eyeFill": "palette.light", "eyeGlow": {"opacity": 0.9, "radius": 0.04}
  },
  "faces": {                         // NotchMascotLook.face (NMS:317-347)
    //            leftEye        rightEye       eyeSize(w,h)    shineCenter    shineRadii      shineAngle(rad)
    "robot":    {"l":[0.36,0.63],"r":[0.64,0.63],"eye":[0.085,0.16],"shine":[0.27,0.34],"shineR":[0.07,0.04],"angle":-0.3},
    "ball":     {"l":[0.36,0.50],"r":[0.64,0.50],"eye":[0.10,0.20], "shine":[0.31,0.29],"shineR":[0.085,0.055],"angle":-0.5},
    "egg":      {"l":[0.375,0.55],"r":[0.625,0.55],"eye":[0.095,0.19],"shine":[0.33,0.31],"shineR":[0.075,0.05],"angle":-0.6},
    "squircle": {"l":[0.35,0.53],"r":[0.65,0.53],"eye":[0.10,0.20], "shine":[0.24,0.28],"shineR":[0.08,0.05],"angle":-0.55},
    "pill":     {"l":[0.37,0.57],"r":[0.63,0.57],"eye":[0.09,0.19], "shine":[0.20,0.42],"shineR":[0.07,0.045],"angle":-0.45}
  }
}
```

Rendering rules (`NMasV:100-165`):
* Layer order: `ears, antenna, bulb, body fill (gradient masked by body or head path), visor, shine`, then a face group with both eyes.
* The gradient runs vertically from `light` at the top to `shade` at the bottom.
* Each eye layer is a box 0.4·size wide centered on its eye point. The eye outline is computed around that center.
* A look change at the same size crossfades for 0.24 s.
* No implicit animations: every motion is explicit.

**Eyes as data** (`NMS:441-480`, `604-667`). Each eye is generated from a shape function at unit size, mirrored for the right eye, **resampled to exactly 24 points** (`eyeCurveCount`) evenly spaced along the outline, starting where the outline crosses x=0 at the top and running clockwise. It is drawn as a closed curve with one cubic per point, using controls `p_i + (p_{i+1} − p_{i−1})/6` and `p_{i+1} − (p_{i+2} − p_i)/6`. Because every eye has the same 24-point structure, any eye morphs into any other by interpolating points. That works in WebView2 either by JS interpolation or by animating the CSS `d: path()` property, which Chromium interpolates between paths with identical command lists.

With `w`, `h` the face's eye size × mascot size, the eye shapes (`NMS:455-479`) are:

| Eye | Outline |
|---|---|
| open | stadium w × h, centered |
| closed | stadium 1.5w × 0.45w at (0, +0.12h) |
| squint | stadium w × 0.55h at (0, +0.08h) |
| wide | circle of radius 0.8w at (0, −0.02h) |
| happy | arch band: span 2.1w, rise 0.42h, thickness 0.55w, top −0.18h, round caps |
| heart | the classic heart curve `x=16sin³t, y=13cos t−5cos2t−2cos3t−cos4t`, scaled to 2.0w × 1.8w at (0, −0.02h) |
| sleepy | lid: flat top at +0.02h, width 1.25w, half-ellipse below of depth 0.38h |
| determined | the open stadium clipped below the line from (−w/2, −0.30h) to (w/2, +0.02h) (the lid slants toward the nose) |

Port `eyePoints` + `resample` + `smoothClosedPath` to TS verbatim (≈150 lines, pure math), or precompute the 24-point arrays per eye and face at unit size into `mascot.json` (8 eyes × 5 faces × 2 sides).

**Moods** (`NMS:286-302`): `{left, right, gaze(x,y in shares of size), tilt(rad, clockwise)}`

| mood | left | right | gaze | tilt |
|---|---|---|---|---|
| idle | open | open | 0,0 | 0 |
| happy | happy | happy | 0,−0.02 | 0 |
| wink | open | happy | 0,0 | −0.08 |
| love | heart | heart | 0,0 | 0 |
| sleepy | sleepy | sleepy | 0,0.03 | 0.05 |
| determined | determined | determined | 0,0 | 0 |
| surprised | wide | wide | 0,−0.02 | 0 |
| searching | open | open | 0.05,0 | 0 |
| thinking | squint | open | 0.06,−0.06 | −0.1 |
| confused | open | squint | −0.03,0.01 | 0.14 |
| alert | wide | wide | 0,0 | 0 |

A mood change morphs the eye paths over 0.22 s with `cubic-bezier(0.2,0.8,0.3,1)`. Gaze moves on a spring (damping 16, stiffness 260); tilt on a spring (damping 12, stiffness 200) (`NMasV:199-234`). The resting mood is `alert` while Keep Awake is active, else `idle` (`NSvc:471`).

### 12.3 Transform stack and keyframe primitives
The layers nest as `root (position) › hopper (translateY) › squasher (squash transform) › tilter (rotation) › attention (eye offset) › face (gaze) › eyes`. In SVG these are nested `<g>`s, each with its own WAAPI animations. Additive animations translate to `composite: "add"`.

`squashed(a)` = `scale(1+0.8a, 1−a)` anchored so the bottom (0.42·size below center) stays put (`NMasV:444-449`). Positive values flatten, negative values stretch.

| Primitive | Target | Values @ keyTimes | Duration |
|---|---|---|---|
| hop(h) | hopper translateY (additive) | 0, −h, 0, −0.3h, 0 @ 0, .32, .6, .78, 1 (easeOut/easeIn alternating) | 0.62 |
| hop squash | squasher | sq(0), sq(−.12), sq(0), sq(.14), sq(0), sq(.06), sq(0) @ 0, .12, .3, .62, .72, .84, 1 | 0.62 |
| pop | squasher | sq(0), sq(−.14), sq(.06), sq(0) @ 0, .35, .7, 1 | 0.34 |
| wobble | tilter rotate (additive) | 0, .12, −.1, .06, 0 rad @ 0, .25, .5, .75, 1 | 0.5 |
| heartbeat | each eye scale | 1, 1.3, 1, 1.22, 1 @ 0, .2, .45, .65, 1 | 0.7 |
| stretch | squasher | sq(0), sq(−.12), sq(−.12), sq(.05), sq(0) @ 0, .35, .6, .82, 1 | param |
| flashFace(m, d) | eye paths | from current, to m, hold, back @ 0, .15, .85, 1 (discrete with Reduce Motion) | d |
| blink | each eye scaleY | 1, .1, 1 @ 0, .45, 1 (0.17 s); double blink 18% of the time: 1, .1, 1, 1, .1, 1 @ 0, .2, .4, .58, .78, 1 (0.44 s) | |
| idle glance | attention translateX ±0.12·size (additive) | 0, away, away, 0 @ 0, .2, .8, 1; starts 1.1 s before the blink; 25% of blinks, not within 3 s of attending something | 1.3 |

**Reactions** (`NMS:116-161` lengths, `NMasV:497-636` choreography; `lift` = `track.hop(0.22)`):

| Reaction | Length | Choreography |
|---|---|---|
| celebrate | 1.0 | hop(lift) + flash happy 1.0 |
| love | 1.4 | flash love 1.4 + heartbeat at +0.22 |
| surprised | 0.9 | flash surprised 0.9 + hop(0.7·lift) |
| flash | 0.5 | eyes scaleY 1, .06, .06, 1.18, 1 @ 0, .18, .42, .7, 1 (0.45 s) + pop |
| confused | 1.2 | flash confused 1.2 + wobble |
| wakeUp | 1.6 | flash sleepy 0.7; stretch at +0.35 for 0.6; flash happy 0.9 at +0.7; a dozing robot relights its bulb after 0.35 |
| yawn | 1.2 | flash sleepy 1.2 + stretch at +0.1 for 0.9 |
| perk | 1.0 | flash alert 1.0 + hop(0.55·lift) |
| bounce | 1.0 | flash alert; translateY 0, −h, 0, −.7h, 0, −.15h, 0 @ 0, .14, .32, .46, .63, .75, .88 (h=max(1, 0.6·lift)); squash sq 0, −.12, 0, .14, −.1, 0, .12, 0, .04, 0 @ 0, .04, .14, .32, .36, .46, .63, .7, .8, .88; 0.95 s |
| hush | 1.0 | eyes scaleY 1, .1, .1, 1 @ 0, .22, .78, 1; squash 0, .1, .1, −.03, 0 @ 0, .25, .7, .88, 1; 0.95 s |
| ready | 1.0 | flash determined 1.0; nod sq 0, .1, −.06, 0 @ 0, .3, .65, 1, 0.5 s at +0.12 |
| groove | 1.6 | flash happy; 4 beats: bob −h on odd steps (h = max(1, 0.45·lift)), lean ±0.11 rad alternating, squash −.05 up / .07 down; keyTimes step/8 |

Every reaction also makes the robot's bulb glow (shadow opacity 0, 1, 1, 0 @ 0, .12, .7, 1) for its length. With Reduce Motion only the face changes (`NMasV:504-516`).

**Reaction sources** (`NSvc`, `TimerService`, etc.):
* screenshot → flash (patience 1)
* timer start → ready; timer end → focus: celebrate, break: ready, timer or stopwatch: surprised (`NMS:1187-1195`)
* mic mute → hush; unmute → perk
* keep awake on → perk; off → yawn
* unlock → wakeUp
* download done → celebrate; download failed → confused
* file dropped onto the shelf → celebrate (patience 30)
* event started → bounce
* music started → groove, at most every 600 s, 1.2 s after the start (`NMS:1167-1170`)
* charger in → love; battery full → celebrate; battery low → yawn
* Command Bar results → celebrate

Gating (`NMS:228-244`): no reaction within 2.5 s of the previous one, no repeat of the same reaction within 30 s, and a reaction waits at most 8 s (default patience) for the mascot to be showable.

### 12.4 Placement, visits and motion
* **Size** (`NMS:1322-1327`): in a capsule, `max(10, min(18, bodyHeight − 6))`; beside a camera, `max(12, min(20, stripHeight − 12))`. In a notice it is 18 pt (`noticeSize`), in the drop hint 20 pt (`NV:111`).
* **Track** (`NMS:750-805`, `1329-1371`): `{width, height, size, rest, hidden?: camera range, baseline, ceiling, mirrored, scale}`. A **capsule** (Windows) rests at `width/2` with `baseline = height/2` and no hidden range. `hop(share) = min(size·share, max(scale, baseline − size/2 − ceiling − 1·scale))` keeps hops below the capsule's top edge.
* **Visit kinds and durations** (`NMS:669-747`, `824-860`):
  * lap 3.6 s, pass 3.4 s
  * home and arrive 0.62 s
  * cameo(reaction): 0.5 arrival + (length + 0.3) hold + 0.32 exit
  * linger(reaction): hold + 0.32
  * countdown(t): t + 0.1 + 0.32 (beside a camera only)
  * retreat 0.32, farewell 0.78, cross 1.24
* **Paths are generated** from a list of steps (`NMS:1078-1160`), sampled at **60 fps** into parallel arrays `{keyTimes, x, lift, squash, gaze}` plus a `greeting` time window. They are played as keyframes on root.position (`x`, `baseline − lift`), squasher (`squashed(squash)`) and face translateX (`gaze·size`, additive) (`NMasV:666-731`). The step types:
  * `stay(t, bounces)`: lift = |sin(πn·share)|·hop(0.16) when bouncing.
  * `hop(to, t, height)`: x eased with smoothstep `s²(3−2s)`; lift = `4·height·s(1−s)`; squash −0.12·(1−s/0.2) while s < 0.2 and +0.14·(s−0.88)/0.12 when s > 0.88; gaze ±0.05 toward the travel direction; landing squash 0.14 recovers over 0.14 s.
  * `walk(to, t)`: linear x, bob = |sin(3π·s)|·hop(0.06), gaze ±0.05.
  * `jump(to)`: an instant teleport, two frames at the same time.
* **Capsule step lists** (the Windows branch: `hidden == nil`):
  * lap: stay .18, hop→farSpot (0.60, h = hop .3), stay .58, stay .94 with 2 bounces, walk→width+size (.40), jump→−size, hop→rest (.52, hop .26). Greeting window 0.78…1.78. farSpot = `min(width − 0.8·size, rest + 0.22·width)`.
  * pass: start −size; stay .20, hop→rest (.42, hop .26), stay .88 with 2 bounces, hop→farSpot (.60, hop .3), stay .80, walk→width+size (.40). Greeting 0.55…1.55.
  * home: stay .08, hop→rest (.36, hop .2). Greeting 0…0.4.
  * arrive: start −size; stay .06, hop→rest (.46, hop .3). Greeting 0…0.52.
  * cameo: start −size; stay .06, hop→rest (0.44, hop .3), stay hold, walk→width+size (.32).
  * linger: stay hold, walk→width+size (.32).
  * farewell: stay .06, hop→rest (.3, hop .22), stay .02, walk→width+size (.4). Greeting 0…0.44.
  * cross becomes home. countdown and retreat do nothing (countdown watching needs a camera, `NSvc` `watchMascotCountdown` guards `!geometry.floats`).
* **Greeting** mood for a visit: random among happy, wink and love (`NMS:1214`, `1316-1320`).
* **Scheduling** (`NSvc:4434-4480`): the next visit comes after a random delay in the frequency's range. It is skipped (and rescheduled) when the island is busy or can't host it, in Low Power Mode, or with Reduce Motion when it would cover content. A visit while resting is a lap; otherwise a pass over the activity strip, which fades out (0.2 s) while the mascot crosses (`NMasV:1277-1332`).
* **Idle life** (`NMS:1218-1231`; `NMasV:779-830`):
  * Blinks every 2.6–6.4 s (5.2–12.8 s in Low Power Mode).
  * After 36 blinks at rest (12 between 22:00 and 06:00) it turns **sleepy** ("dozed"); anything that happens wakes it.
  * When hiding while idle is on, it tucks away 30 s after its last activity with `linger(.yawn)`, retrying 3 s later if busy.
  * Welcome hello 1.5 s after being switched on.
* **Pointer** (`NMasV:986-1032`): while resting, its eyes follow the pointer over the island. Gaze = clamp(dx / 3·size, dy / 2·size) × (0.08, 0.05). Pointer entry wakes it (a wakeUp reaction when dozing). Resting the pointer on it for 1 s is a pat: love reaction, at most once per 30 s, plus a haptic tick on the Mac (⚠️ none on Windows).
* Reduce Motion: no walking. It fades in at its stand point, greets and fades out (opacity 0, 1, 1, 0 @ 0, .12, .88, 1), and blinks are discrete.

### 12.5 Windows needs
Pure frontend: SVG plus WAAPI, with `requestAnimationFrame` only for path morphing if CSS `d` animation turns out unreliable. Inputs from the backend:
* **Low Power Mode** → Windows **Energy Saver**: `PowerRegisterForEffectivePowerModeNotifications`, mapping only `EffectivePowerModeBatterySaver` (`BetterBattery` is the power slider, not Energy Saver), or `RegisterPowerSettingNotification(GUID_ENERGY_SAVER_STATUS)` on 24H2, or `GetSystemPowerStatus().SystemStatusFlag == 1`.
* **Reduce Motion** → Windows "Animation effects". Chromium reads `SPI_GETCLIENTAREAANIMATION` and updates on `WM_SETTINGCHANGE` (`ui/gfx/animation/animation_win.cc`), so WebView2's `prefers-reduced-motion` follows it.
* **Lock/unlock** → `WTSRegisterSessionNotification` (`WTS_SESSION_UNLOCK`).
* **Charger and battery** → `RegisterPowerSettingNotification`, called once per GUID: `GUID_ACDC_POWER_SOURCE` and `GUID_BATTERY_PERCENTAGE_REMAINING`.
* Pointer position over the island → webview pointer events.

---

## 13. Accessory notices (Bluetooth connect / low battery)

Source: `NAcc:10-129`, `NAccSup:6-131`. These are not a page: they are **notices** (`NotchEvent.accessory`) queued at most 8 deep, each expiring after 30 s, shown one after another while the island is closed (`NAcc:113-128`). There are two kinds:
* **Connected**: title "Connected", detail = device name, symbol chosen from the device name or its Bluetooth class of device (AirPods variants, headphones, keyboard, mouse, trackpad, speaker, car, TV, game controller, printer, watch, glasses…; `NAccSup:44-66`). Phones, tablets, computers and access points (major class 0x01–0x03) never announce. A silent baseline is taken when the feature starts and after sleep or lock.
* **Low battery**: when a device crosses ≤ 20%, it re-arms only after a fresh reading ≥ 25%. Detail "Low battery · n%", level bar = n/100. Missing readings never count as recoveries (`NAccSup:79-118`).

Preference `notchAccessoriesEnabled` (default true, `DEF:1473`), which also needs the monitorPower feature.

**Windows**: `Windows.Devices.Enumeration.DeviceWatcher` with `BluetoothDevice.GetDeviceSelectorFromConnectionStatus(Connected)` and `BluetoothLEDevice.GetDeviceSelectorFromConnectionStatus(Connected)`. The class of device comes from `BluetoothDevice.ClassOfDevice` (`MajorClass`, `MinorClass`). Battery: for BLE, the GATT Battery Service `0x180F` / characteristic `0x2A19` (notifications); for Classic and HFP devices, the PnP property `{104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2` (DEVPKEY_Bluetooth_Battery, ⚠️ undocumented) read with `PnpObject.CreateFromIdAsync`. Hearing which devices are low needs a shared battery sampler (the system spec).

---

## 14. Backend capability checklist (minimal Windows build)

| # | Capability | Section | Status |
|---|---|---|---|
| 1 | Output volume/mute + events (`IAudioEndpointVolume`) | 1, 2 | ✅ documented |
| 2 | Output device list + default switch (`IMMDeviceEnumerator` + `IPolicyConfig`) | 1, 2 | ⚠️ switch is undocumented |
| 3 | Per-app sessions: volume, mute, peak, icon, name (`IAudioSessionManager2`) | 2 | ✅ (no boost >100% ⚠️) |
| 4 | Per-app routing (`IAudioPolicyConfigFactory`) | 2 | ⚠️ undocumented, optional |
| 5 | Mic level + mute all inputs | 1, 2 | ✅ |
| 6 | Keep awake (`PowerSetRequest`) | 1, 7 | ✅ |
| 7 | Brightness: WMI + DDC/CI | 1 | ✅ (focus display ⚠️) |
| 8 | Clipboard listener/read/write/privacy formats/paste via `SendInput` | 3 | ✅ (UIPI, source app ⚠️) |
| 9 | Screen snapshot (DXGI/BitBlt) + selection overlay + exclude-from-capture | 4, 5 | ✅ |
| 10 | OCR (`Windows.Media.Ocr`), QR (`rqrr`) | 4, 7 | ✅ |
| 11 | Share sheet (`IDataTransferManagerInterop`) | 4, 6 | ✅ |
| 12 | File drop in (Tauri), drag out (`DoDragDrop`), shell icons/open-with/reveal | 6 | ✅ (text drops ⚠️) |
| 13 | ZIP (`zip` crate) | 6 | ✅ |
| 14 | Quick toggles: theme registry, recycle bin, eject, hidden files, desktop icons, lock, display off, screen saver | 7 | ✅ (desktop icons ⚠️ undocumented) |
| 15 | Camera via WebView2 `getUserMedia` + permission handler | 9 | ✅ |
| 16 | Per-monitor windows, full-screen detection | 10 | ✅ (full-screen detection ⚠️ heuristic) |
| 17 | Scratchpad / shelf / clipboard / captures JSON stores (atomic writes) | 3, 6, 11 | ✅ |
| 18 | Power mode, reduce motion, lock/unlock, AC/battery events | 12 | ✅ |
| 19 | Bluetooth connect + battery | 13 | ⚠️ Classic battery needs an undocumented property |
| 20 | Screen recording (WGC + MF + WASAPI loopback) | 4, 5, 7 | ⚠️ large; optional |

---

## 15. Windows edge cases to handle

1. **Clipboard.** A cut in Explorer sets `Preferred DropEffect` to MOVE. When writing `CF_HDROP` back from history, set `Preferred DropEffect` to COPY, or a paste moves the files. Delayed-render owners (Office) can stall `GetClipboardData`, so read on a worker thread with a timeout (the Mac uses 5 s). RDP file copies arrive as `FILEDESCRIPTOR` / `FILECONTENTS`, not `CF_HDROP`; skip them or record them as text.
2. **Captures.**
   * Take the frozen snapshot **before** the overlay and island appear.
   * DXGI duplication fails across adapters on hybrid-GPU laptops (`DXGI_ERROR_UNSUPPORTED`) and loses access on UAC prompts and the lock screen; fall back to `BitBlt`.
   * Resolve the save folder with `SHGetKnownFolderPath(FOLDERID_Screenshots)`, because OneDrive often redirects Pictures.
   * `OcrEngine.TryCreateFromUserProfileLanguages()` returns null without an OCR language pack, and `OcrEngine.MaxImageDimension` caps the input, so downscale first.
3. **Capture controls.** Drawing the controls inside our own selection overlay window, rather than as the island, avoids the click-through problem entirely. Evaluate this before building cursor polling.
4. **Camera.** Windows 11 lets several apps share a camera, so `NotReadableError` ("in use") rarely fires, and "denied" vs "busy" can be hard to tell apart. Changing Tauri's `useHttpsScheme` changes the webview origin, which resets the saved `deviceId` and the camera permission.
5. **Brightness.** Run WMI on its own MTA thread. For DDC/CI prefer `SetVCPFeature(hMonitor, 0x10, value)` over `SetMonitorBrightness`, which parses the slow capabilities string. Docks and DisplayLink adapters often lack DDC/CI. One HMONITOR can map to several physical monitors.
6. **Bluetooth.** Dual-mode devices appear in both the Classic and BLE watchers, so dedupe by address. BLE devices have no class of device; use the GAP Appearance value for the symbol. `DEVPKEY_Bluetooth_Battery` sits on the hands-free (HFP) child device node, not the main device node.

---

## Open questions / risks

1. **Mixer boost.** Windows' `ISimpleAudioVolume` cannot go above 100%. Is a 0–100% mixer (no orange boost range) acceptable as the one visible deviation, or should we ship a virtual audio driver or APO (large, needs driver signing)?
2. **Undocumented audio APIs.** `IPolicyConfig` (default device) and `IAudioPolicyConfigFactory` (per-app routing) are undocumented. Their IIDs changed once (21H2), and a Windows update can break them. Decide whether per-app routing ships at all, and keep both behind a capability probe that hides the UI when activation fails.
3. **HTML5 drag-and-drop vs. Tauri file drop.** With `dragDropEnabled` on the island window, in-webview HTML5 DnD stops working on Windows. Every reorder (mixer columns, quick-panel edit grid, scratchpad tabs, quick-access editor) must use pointer-event drag code. If the Files section is split into its own window instead, this conflict disappears, but the island is a single window.
4. **Text and link drops onto the shelf** need a custom `IDropTarget`, which competes with WebView2's own drop handling. Minimal: files only. Confirm.
5. **Drag-out from the shelf** needs a native `DoDragDrop` started from Rust while the pointer button is down in the webview. Verify the `drag` crate works with a transparent, non-activating Tauri window.
6. **Click-through regions** (capture controls, Quick Access circles outside the island body). Tauri's ignore-cursor-events is all-or-nothing per window, so it needs Rust-side cursor polling or a window region. This needs a prototype; it affects the shell spec too.
7. **Paste into the previous app.** Focus restoration plus `SendInput` is best effort: it fails silently into elevated apps (UIPI) and in some UWP and games. Fallback: copy plus the check. The "⌘1–⌘9" badge becomes "Ctrl+1…9", which is wider; check that the compact row still fits (the badge sits beside the time).
8. **Clipboard source app** attribution is unreliable on Windows. Show the app icon only when the owner was resolved, else the kind glyph (as the Mac does for unknown apps).
9. **Media tools are out** of the minimal scope, but the Files band's wand button hosts "Create ZIP". Decide where ZIP lives without media tools (keep a popover with only ZIP, or add ZIP to the tile context menu).
10. **Screen recording and the screenshot editor** are not in the minimal scope, but several island controls point at them (recording tile, Edit, Restore, Pin, capture-controls audio switches). Hiding them changes layouts (fewer tiles and buttons). Confirm the subset: screenshot + OCR + color picker.
11. **Desktop icons toggle** uses an undocumented Explorer command (0x7402). **Eject** fails while files are open. **Screen saver** does nothing when none is configured. Each needs a visible "failed" state, as the Mac row already has.
12. **Mascot path morphing.** The plan relies on Chromium interpolating CSS `d: path()` between 24-cubic paths (WAAPI). If that is janky in WebView2, do the per-frame point interpolation in JS. Verify on a 60 Hz and a 144 Hz display.
13. **Reduce Motion mapping.** Resolved: Chromium (and so WebView2) maps Windows "Animation effects" (`SPI_GETCLIENTAREAANIMATION`) to `prefers-reduced-motion` and follows changes live. Low Power Mode has no such mapping; pass Energy Saver in from Rust (§12.5).
14. **Accessory battery** for Bluetooth Classic and HFP devices uses an undocumented PnP property. BLE GATT is documented but requires the device to expose the Battery Service.
15. **Haptics** (petting the mascot, island feedback) have no Windows API. Drop them silently.
16. **`NotchMirrorView` is not the camera.** The brief listed "camera + mirror" together. Both are covered here: the camera "mirror" (CameraPreview, §9) and the closed-island copies on other displays (§10).
