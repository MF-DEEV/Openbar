# 04 — Media & info sections (behavior spec for the Windows port)

Scope: Music (lyrics, queue, equalizer bars, browser artwork, live audio level, shuffle, preferred player, playback source, commands), Notifications, Calendar, Timer (ruler, strip, alerts), Downloads, Watch, Keep Awake strip, System, Lock screen (brief).

Source of truth: the Swift tree at the commit this was written against (`482540dd`). All `file:line` refs are relative to `Sources/Vorssaint/` unless they start with `Sources/`. Abbreviations: **NS** = `Services/Notch/NotchService.swift`, **NSup** = `Services/Notch/NotchSupport.swift`, **Def** = `Core/Defaults.swift`.

Units: SwiftUI points map 1:1 to CSS px at 100 % scaling (DIPs). On Windows, multiply by the monitor's DPI scale for physical pixels. "SF Symbol X" means an icon of the same meaning. The Windows build needs a matching icon set (Fluent System Icons is the natural pick). Colors are sRGB.

Window-state vocabulary used below (the shell spec defines them):
- **closed / resting**: the bare island, optionally with resting content (`NotchIdleContent`: none, battery, music, agents; NSup:712).
- **compact live activity ("strip")**: the closed island widened with one activity, or a pair (primary + companion).
- **notice**: a transient banner in the closed island (`NotchNotice`, NS:9). It has an event type, priority and duration.
- **expanded page**: the open island showing one module's page. The "band" is the island's top row (beside the camera on a notched Mac). Pages put controls there through `.notchBand(leading:trailing:)`.

> **Windows geometry note.** A Windows display has no camera housing, so the island is always the **floating capsule** (`NotchSilhouette.capsule`, the default, NSup:119-125) or a *simulated* notch. The capsule variants (`UI/Notch/NotchCapsuleViews.swift`) are the primary closed-state reference for Windows. The "beside the camera" strip variants apply only when the person picks the simulated-notch silhouette.

---

## 0. Cross-cutting rules for these sections

### 0.1 Live activity priority (who takes the closed island)
`NSup:1791-1799` sets the automatic order, first true wins:

1. `timer`: a session exists and its countdown isn't hidden, or it completed (`Services/Notch/NotchTimerSupport.swift:178-181`)
2. `watch`: watch enabled and active (watching or hidden) (NS:489-491)
3. `sessions` (Claude sessions, other spec)
4. `downloads`: download route on and some item `active && !completed` (NS:493-496)
5. `agents` (other spec)
6. `calendar`: a countdown inside its 60-minute lead window (NS:514-521)
7. `music`: playing, or held as playing because the strip's own button just paused it under the pointer (NS:498-503, NSup:1874-1876)
8. `keepAwake`: `notchKeepAwakeActivity` on and a session active (NS:523-525)

- With more than one activity, hovering the closed island shows an **activity picker**. A manual choice outlives gaps (for example a song change) and resets when nothing is left (`NotchActivitySelection`, NSup:915-945). Switching animates `.smooth(duration: 0.26)` (NS:573-584).
- **Pairs** (primary keeps its reading on the right, companion's mark on the left; NSup:1806-1819):
  - `timer` + {downloads; agents / calendar / music only while the timer is *running*}
  - `calendar` + {downloads, agents, music}
- A new song that waits for its track notice is filtered out of the activities until the notice has shown (NS:527-530).
- If the person enabled the island outline, it turns system orange while the timer is the compact activity (NS:3135-3138).

### 0.2 Notices raised by these sections
`NotchEvent` (NSup:1600-1643):

| Event | Pref key (default) | Priority | Duration | Raised by |
|---|---|---|---|---|
| `track` | `notchTrackChange` (true) | 0 | 3 s | New song, debounced 0.5 s (NS:2406-2427) |
| `systemNotification` | `notchNotificationsEnabled` (true) | 1 | 3 s | Mirrored banner (NS:4118-4127) |
| `timer` | `notchTimerEnabled` (true) | 2 | 6 s | Countdown/phase finished (`NotchTimerService.swift:77-90`) |
| `download` | `notchDownloadsEnabled` (true) | 0 | 6 s | Download completed (NS:4019-4024) |
| `watch` | `notchWatchEnabled` (true) | 2 | 6 s | Watch rule met / window closed (`NotchWatchService.swift:359-378`) |

Replacement rule: a new notice replaces the current one if its priority is ≥ the current one's. While a banner is held open (expanded preview), only the same event or a strictly higher priority replaces it (NSup:2094-2098). Notice text is 11 pt medium, monospaced digits. The track notice shows the live cover (18 pt max) in the symbol slot and truncates at the tail. Timer notices are orange (`UI/Notch/NotchNoticeView.swift:16-30, 104-118`).

### 0.3 Motion conventions used throughout
- `.smooth(duration: d)` = critically damped spring with that duration (no bounce). On the web: `cubic-bezier(0.25, 0.1, 0.25, 1)` over `d`, or a spring with damping ratio 1.
- `contentTransition(.symbolEffect(.replace))` = the glyph cross-fades/scales into the new one (≈0.2 s).
- `contentTransition(.numericText(countsDown:))` = digits roll vertically (down when counting down). This is the "rolling digits" modifier (`UI/Notch/NotchComponents.swift:81-94`). In the closed island only the part before the last `:` rolls (`everySecond: false`). Seconds change in place to save energy (`NotchTimerSupport.swift:288-295`).
- Every animation is disabled under Reduce Motion (Windows: `SystemParametersInfo(SPI_GETCLIENTAREAANIMATION)` / CSS `prefers-reduced-motion`).
- Decorative loops (equalizer bars, watch eye, agent marks) step at **≤30 fps** on their own clock, and stop when the island window is hidden or occluded (`UI/Notch/NotchDecorativeClock.swift:13-48`). Windows: drive them from `requestAnimationFrame` throttled to 30 fps, and pause on `document.visibilitychange`.
- Buttons use `NotchButtonStyle` (press/hover feedback, defined in the shell spec).

---

## 1. Music

### 1.1 Data model
- `NotchPlayback` (`Services/Notch/NotchMusicSupport.swift:28-98`) holds: track (title, artist, album, artworkData, appBundleIdentifier, appPID), isPlaying, elapsed, duration, rate, sampledAt, canSeek, hasPosition, itemIdentifier, commandContext, canSendCommandsDirectly, canSkipNext/Previous (nil = unknown).
  - The position is extrapolated: `min(duration, max(0, elapsed + (isPlaying ? (now - sampledAt) * rate : 0)))` (:54-56).
  - Decoding clamps rate to 0…16 (default 1 when missing) and times to 0…7 days (:74-82).
- **Gap grace.** A reading that finds nothing playing (or another player's paused song) right after a playing song is held for **1.5 s** (`gapGracePeriod`, :31) before it is applied. A real reading inside the gap replaces it at once. Commands are refused during a gap (`Services/Notch/NotchMusicService.swift:243-264, 561-564`).
- **Track change detection** (`NotchTrackChange`, NotchMusicSupport.swift:168-224) is per player and works on title + artist.
  - A reading without an artist still matches the same title.
  - Only a *playing* new song counts.
  - The first reading after start or a source change only seeds state.
  - `trackChanges` fires before publish and drives the track notice. `trackEnds` fires when a song stops with no successor and drives the strip's leave animation.
- **Artwork cache** (`NotchArtworkCache`, :106-159). It keeps one decoded cover. Metadata-only updates keep it. On a new song with no artwork yet, the old cover stays up to **1.5 s** (`transitionDuration`). Covers are thumbnailed to **160 pt @2x** (MusicService:175).
- **Artwork tint** (`NotchArtworkTint.from`, NotchMusicSupport.swift:14-25):
  1. Average the cover down to one pixel (MusicService:355-373).
  2. Reject it if max channel ≤ 0.05 or (max−min)/max < 0.12, so neutral covers give **no** tint and the UI uses white.
  3. Otherwise stretch each channel: `(c − min)/(max − min)·0.86 + 0.06`.
  - Accent uses: timeline fill, active shuffle glyph, equalizer bars on the strip, strip play/pause glyph, lock-screen glow.

### 1.2 States and layout — expanded page (`UI/Notch/NotchMusicView.swift`)
Content width is fixed at **344 pt** (`musicContentWidth`, NSup:326). Layout constants (NSup:326-342):

| Constant | pt |
|---|---|
| artwork | 56 (corner radius 0.19 × size = 10.6, continuous) |
| title block height | 38 |
| timeline row height | 14 |
| transport row height | 44 |
| head↔timeline spacing | 14 |
| bottom room | 8 |
| `rowSpacing` | 10 |
| camera clearance | 6 |
| min extra (lyrics/queue) height | 88 |
| extras height added to the island | **216** (custom size: `min(216, budget)`, NSup:2694) |

The player height is `head + 14 + (playing ? 14 + 10 : 0) + 44 + 8`, where head = 56 without a camera band (NSup:660-668). When lyrics or the queue opens, the island **grows** by 216 and the player keeps its height. Only at a custom island size that can't hold both does the player give way, and only when less than 88 pt would remain for the extra (`musicSplit`, NSup:676-688).

**Playing** (`player(_:)`, :256-304), top to bottom:
1. **Head row** (`head`, :135-153): `HStack(alignment: .bottom, spacing: 14)`.
   - Cover button (56 pt). It opens the player app. When paused it scales to **0.94**, animated `.smooth(0.3)` (:259-266).
   - Titles: title 14 pt semibold, 1 line. Second line 13 pt `.secondary`: artist, else album, else "Now playing". After a failed command it shows "Could not change playback." in **orange** (:268-278).
   - Right column: optional corner row (28 pt high, lyrics/queue toggles + sections button; only without a camera band). Below it the accessory row:
     - **source menu** icon, only if more than one source or a manual choice (:281-283)
     - **equalizer** (4 bars, 2.5 pt wide, 15 pt high, white @ 0.5, flat while paused) (:286-287)
2. **Timeline** (`NotchMusicTimeline`, :611-718), shown only if duration > 0.
   - Times sit beside the bar. Each side reserves the width of the longest possible reading so the bar never changes length. Left shows elapsed, right shows `−remaining`. 10 pt medium, monospaced digits, secondary.
   - Format: `m:ss`, or `h:mm:ss` at ≥ 1 h.
   - The bar is a draggable slider (10 pt high, tint = accent) if `canSeek`, else a 6 pt meter.
   - Redraws once a second while playing (paused = static).
3. **Transport row** (:239-252): **five equal slots of width/5**.
   - Slot 1: shuffle (if offered). Slots 2-4: previous / play-pause / next. Slot 5: audio **output** menu (if the mixer feature exists).
   - Empty slots keep their width, so play stays centered.
   - Glyphs are white with no plate: prev/next 19 pt semibold, play/pause **28 pt** semibold, hit area 44×44 circle (:542-608).
   - A skip button the player doesn't support is hidden but its slot is kept (:544-554).
   - Play↔pause glyph swap uses symbol replace, `.smooth(0.22)`.
4. 8 pt bottom room.
5. Swipe feedback: an accepted swipe-skip nudges the whole player 8 pt in the swipe direction (cubic 0.09 s), then springs back (`.smooth`, 0.25 s) (`UI/Notch/NotchMusicStrip.swift:212-233`).

**Idle (nothing playing)** (:155-203):
- The cover placeholder is black with a music-note glyph (30 % of size, white 0.45), or the player app's icon on black (14 % padding) (`NotchComponents.swift:252-283`).
- Titles: a source picker if sources exist, else "Nothing playing" (14 pt semibold). The second line names the app the buttons open ("Open Spotify").
- The transport row shows prev / play / next, and **every button opens the preferred player** (`NotchMusicIdleTransport`, :464-496). If no player can be resolved, the row shows the centered hint "Your music and playback controls appear here." (12 pt secondary, 2 lines).

**Loading** (`awaitingPlayback`, first request until the adapter's first reply): a clear frame of the player height, not "Nothing playing" (:58-62).

**Extras** (lyrics / queue): toggles named by glyph (`quote.bubble`, `list.bullet`) in the band, or in the corner row. Only one can be open at a time. Escape closes the extra before the island (:117-124, 206-224). With the player hidden (short custom island) the toggles move to a 28 pt row above the list (:52-69).

**Home card** (`NotchMusicControlsView`, :724-800) on the Controls page:
- Card height 80 pt (`cardHeight`), radius 18, padding 12. Cover is `max(40, height−24)` pt, radius 16.
- Title 13 pt semibold, artist 11 pt. The compact transport (prev/next 16 pt, play 22 pt, 36 pt hit) sits beside the title, or wraps below it when narrow.
- When idle, the cover opens the preferred player. Clicking the title opens the Music page.

### 1.3 Closed states
**Capsule strip (Windows default)** (`NotchCapsuleMusicStrip`, `UI/Notch/NotchCapsuleViews.swift:262-339`):
- Cover in the left round end, concentric (radius = side/2, side = `stripBodyHeight − 6`, inset `(body − side)/2`; NSup:1035-1036).
- Title in the middle: 12 pt semibold (`NotchCapsuleLayout.titleFont`, NSup:973). It shows **only for 4 s when a song starts** (`musicTitleDuration`, NS:405, 2892-2907) or while the pointer rests on the cover; otherwise the middle is empty and the capsule shrinks.
- **7 bars** at the right end: bar width 1.8, gap 0.85 × width (NSup:373-377), height `min(16, max(6, body − 8))`, tint = accent.
- Width = cover + (title + 2×8 padding) + bars, capped at **260 pt** of capsule plus end caps and by the free menu-bar room (`musicSurface`, NSup:1092-1097; `Maximum.music`, NSup:984).

**Strip beside a (simulated) camera** (`UI/Notch/NotchMusicStrip.swift:21-183`):
- Cover in the left wing, bars in the right wing.
- With a simulated camera gap ≥ 56 pt, the title and artist are drawn *between* the wings (11 pt semibold / 9 pt medium at white 0.62, the artist only if content height ≥ 28) (NSup:781-786).
- Otherwise, resting on the cover for **0.3 s** "names the song": the left wing grows from the island's curved end with title over artist, up to **240 pt** (NSup:754-825, NS:2994-3014).

**Strip interactions** (both variants):
- Hovering the bars cross-fades them (`.easeOut(0.15)`) into a play/pause glyph in the accent color, `min(13, max(8, barHeight−3))` pt (MusicStrip:128-143, 187-208). Clicking it toggles playback in place (NS:3034-3064).
  - The requested state shows immediately for direct players, held up to 1.5 s.
  - A song paused this way stays on the strip while the pointer is inside (`musicStripHeldSong`).
- Clicking anywhere else opens the Music page.
- A strip that is leaving keeps its last song/cover snapshot (`NotchCompactMusicSnapshot`, MusicStrip:7-16).

**Resting** (`notchIdleContent = music`, the default): while playing, the resting island shows 3 bars (2 pt wide, 11 pt high, accent) on the right wing (`UI/Notch/NotchView.swift:721-728`). The companion mascot gives way to real playback (NSup:1973-1982).

### 1.4 Interactions & shortcuts
- **Space** = play/pause while the page is focused (not in Settings previews) (:590).
- **Two-finger horizontal swipe** (precise trackpad only) on the closed music strip or the open Music page = skip (NS:3870-3895; `Services/Notch/NotchGestureSupport.swift:100-122`).
  - The axis locks after 4 pt of travel with 1.5× dominance. It fires at **40 pt**. Leftward travel (negative delta) = next.
  - Pref `notchGesturesEnabled` (true).
  - Windows: precision-touchpad horizontal scroll (`WM_MOUSEHWHEEL` / `wheel` `deltaX`). Mouse wheels have no smooth horizontal phase, so accept only touchpad-class deltas.
- **Scrubbing.** Dragging the thumb shows the dragged time. On release it seeks once; the thumb holds still until the player reports within ±2 s, or for 1 s at most, then follows the player (:661-672, 676-703). Disabled while a command is pending.
- **Play/pause optimistic state.** For direct players the glyph flips at once. It returns to the player's word when the player confirms, the track changes, or after 1.5 s (:511-586).
- **Source menu** (:307-335). The current player's app icon (18 pt, padding 3, radius 6) opens a menu: "Automatic" (check when automatic), a separator, then each source by display name (check = manually chosen PID, kept through gaps).
  - Choosing what's already in effect is a no-op.
  - Choosing a source clears the playback, artwork and queue, then waits for the adapter's re-read (MusicService:429-449).
  - The manual choice lives only while the reader runs. It is never persisted (:69-72) and is dropped if the source disappears (:225-234).
- **Output menu** (slot 5). The glyph shows the current output kind: headphones, laptop/desktop (built-in), speaker (hardware), AirPlay (virtual). The menu lists outputs plus the mixer (:427-460). The mixer spec covers it. Windows: `IMMDeviceEnumerator` and the default render endpoint; switching the default endpoint needs the undocumented `IPolicyConfig` (flag in the mixer spec).

### 1.5 Preferences (music)
| Key | Default | Meaning |
|---|---|---|
| `notchShowPlayingMusic` | true | Music takes the closed island while playing (NSup:1864-1876) |
| `notchIncludeOtherPlayers` | true | Automatic source also follows non-music apps (videos, browsers) (MusicService:101-112) |
| `notchPreferredPlayer` | "" (automatic) | App opened when idle (`NotchPreferredPlayer.swift`) |
| `notchIdleContent` | "music" | Resting content |
| `notchTrackChange` | true | Track-change notice |
| `notchLyricsEnabled` | true | Lyrics toggle available |
| `notchLyricsOnline` | false | Online lyric lookup consent |
| `notchQueueEnabled` | true | Queue toggle available |
| `notchLiveEqualizer` | false | Bars follow the real audio |
| `notchBrowserCovers` | false | Firefox-family cover lookup |
| `notchGesturesEnabled` | true | Swipe gestures |

(Def:1432-1552; on migration from an old configured install, lyrics, queue, live EQ and track change default to false, Def:2097-2108.)

### 1.6 Data source — macOS
- **Now Playing.** The private framework MediaRemote can no longer be called in-process since macOS 15.4. The app runs `/usr/bin/perl now-playing.pl libVorssaintNowPlaying.dylib watch|watch_all` as a child process. It emits one JSON line per change on stdout and takes commands on stdin (MusicService:94-213, `Sources/NowPlayingAdapter/NowPlayingAdapter.swift:1-13`).
  - It is event driven, not polled. It subscribes to 7–8 MediaRemote notifications plus app-terminate, debounced **0.12 s** (:287-306). A read that waits longer than 2 s exits the child (:234-238).
  - The child is restarted at most 2 times, after 1 s then 2 s. The budget resets after 60 s of uptime (MusicService:290-306).
  - Artwork travels base64, ≤12 MB (:32).
  - Command capability ids: play 0, pause 1, next 4, previous 5, seek 24 (:208-220). The commands themselves are at :327-361.
- **Source enumeration** (`Sources/NowPlayingAdapter/NowPlayingSelection.swift`) lists registered Now Playing clients (≤16, `Services/Notch/NotchPlaybackSource.swift:45-59`). Each source has pid, bundle, isMusicApp, isPlaying, hasTrack, displayName. *Music app* = category `public.app-category.music`, or Music / iTunes / Spotify (:23-27).
- **Automatic choice** (`preferred`, :66-86), in order:
  1. A playing music app.
  2. A playing other app (when "include other players" is on).
  3. The followed non-music player, while paused.
  4. Any music app with a track.
  - Ties break by previous PID, then the system's current PID, then lowest PID. A manual choice always wins while it has a track. A chosen player without a track stays chosen for 5 s.
- **Commands to players without direct command support** fall back to Apple Events (`Services/Notch/NotchMusicAutomation.swift`). The player's `.sdef` dictionary supplies the event codes (`NotchMusicAutomationSupport.swift`). This needs per-app Automation consent, which drives the "Allow playback control" button. Validation must arrive within 3 s and the event within 0.5 s of validation (MusicService:673-715).
- **Shuffle** (`Services/Notch/NotchShuffleService.swift`) reads and writes the app's `shuffling` / `shuffle enabled` scripting property through the same Automation consent. It re-reads 1.5 s after a switch, because some players report the old value for about 1 s (:79-89). "shuffling enabled" = false (radio) disables the button.
- **Queue** (`Sources/NowPlayingAdapter/NowPlayingQueue.swift`) uses the MediaRemote playback queue while the queue view is open.
  - Covers are 96 px and ≤64 KB.
  - "Play now" is verified by watching the item identifier change.
  - The queue is unavailable if the player shares only its current item (`Services/Notch/NotchQueueSupport.swift:34-69`).

### 1.7 Data source — Windows 11 equivalent
**Now playing / sources / commands: SMTC.** `Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager.RequestAsync()` works from an unpackaged Win32 process (no identity needed).

| Mac concept | SMTC |
|---|---|
| Source list | `GetSessions()`, event `SessionsChanged` |
| System current player | `GetCurrentSession()`, event `CurrentSessionChanged` |
| Title / artist / album / artwork | `TryGetMediaPropertiesAsync()` → `Title`, `Artist`, `AlbumTitle`, `Thumbnail` (stream ref), `PlaybackType` (Music / Video). Event `MediaPropertiesChanged` |
| isPlaying, rate, capabilities | `GetPlaybackInfo()` → `PlaybackStatus`, `PlaybackRate`, `Controls.IsPlayEnabled / IsPauseEnabled / IsNextEnabled / IsPreviousEnabled / IsPlaybackPositionEnabled / IsShuffleEnabled`. Event `PlaybackInfoChanged` |
| elapsed / duration / sampledAt | `GetTimelineProperties()` → `Position`, `StartTime`, `EndTime`, `LastUpdatedTime`. Event `TimelinePropertiesChanged`. Extrapolate from `LastUpdatedTime`, not from receipt time |
| toggle / next / prev / seek | `TryTogglePlayPauseAsync` (or `TryPlayAsync` / `TryPauseAsync` when only one is enabled), `TrySkipNextAsync`, `TrySkipPreviousAsync`, `TryChangePlaybackPositionAsync(ticks)` (100 ns units). The bool result is the `sent` flag |
| Shuffle | `PlaybackInfo.IsShuffleActive` + `TryChangeShuffleActiveAsync(bool)` |
| Identity / commandContext | `SourceAppUserModelId` (AUMID) + a recording revision (title+artist+album+duration hash). There's no PID or item id, so build `NotchPlaybackContext` from (AUMID, revision) |

- **Port rules:** keep the same 0.12 s debounce, 1.5 s gap grace, 1.5 s artwork transition, optimistic toggle and 2 s seek tolerance.
  - Run SMTC on a dedicated MTA thread in Rust (windows-rs). Push JSON-shaped events to the webview, so the child-process / perl layer disappears.
  - Keep the `generation` guards: drop events from a session that has been replaced.
- **No consent branch.** The whole Apple Events path (`automationAvailability`, "Allow playback control", the Open Settings/refresh buttons in `NotchMusicTransport` :519-534) has **no Windows counterpart and should be dropped.** A session whose `Controls` disable everything shows the transport disabled plus "Open player" (the `.unavailable` branch).
- **isMusicApp heuristic (flag).** Windows has no app category. Use a known AUMID/exe list:
  - Spotify (`Spotify.exe`, `SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify`)
  - Apple Music (`AppleInc.AppleMusicWin_…`)
  - Media Player (`Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic`)
  - iTunes, foobar2000, MusicBee, TIDAL, Deezer, Amazon Music, AIMP, Winamp
  - plus `PlaybackType == Music` from media properties.
- **AUMID → app name / icon.** For packaged apps use `AppInfo` / `Package`. For Win32 AUMIDs (`chrome`, `msedge`, `firefox`'s hash-like id such as `308046B0AF4A39CB`), resolve through `shell:AppsFolder` (`IShellItem` from `SHCreateItemInKnownFolder(FOLDERID_AppsFolder, AUMID)` gives name and icon). Fall back to the raw id. This replaces `NotchPlayerApps` (MusicView:413-423).
- **Opening the player** (cover click / "Open player"): `IApplicationActivationManager::ActivateApplication(AUMID)` for packaged apps, `explorer.exe shell:AppsFolder\<AUMID>` for others. For browsers, bring the owning window forward if it can be found.
- **Queue (no equivalent).** SMTC exposes no queue. The queue extra must be **hidden on Windows**, or show `queueUnavailable` for every player. A later Spotify Web API integration (OAuth, `GET /v1/me/player/queue`, `POST /v1/me/player/next` loops) is the only realistic source and is out of scope.
- **Shuffle is better on Windows.** It's native SMTC, with no consent step. Show the button when `Controls.IsShuffleEnabled`. Keep the 1.5 s re-read, because players lag here too.

**Preferred player** (`Services/Notch/NotchPreferredPlayer.swift`):
- The stored id is a string (bundle id on Mac, AUMID on Windows).
- Resolution order: the chosen player if installed, else the music app that last played *in this launch* (not persisted, :12-14, 33-37), else Spotify, else Apple Music (Windows: Spotify, then Apple Music, then Media Player).
- The Settings list shows installed known players plus any music-category app (:61-70). Windows has no category, so use the known list filtered by `PackageManager.FindPackagesForUser` and Start-menu AUMIDs.
- Opening never starts playback.

### 1.8 Lyrics
- **Provider (identical on Windows).**
  1. **LRCLIB.** `GET https://lrclib.net/api/get?track_name&artist_name[&album_name]&duration`. Duration must be 1–3600 s, fields ≤1024 bytes. The album is omitted when the player reports none (`Services/Notch/NotchLyricsSupport.swift:129-145`).
  2. Then `https://lrclib.net/api/search` fallbacks: normalized artist/title, primary artist, discovery query (:197-206, 316-330).
  3. Last resort: **MusicBrainz** artist/recording/release-group lookups (`https://musicbrainz.org/ws/2/...`, :499-571) to verify artist aliases, rate-limited to **one request per 1.05 s** (`Services/Notch/NotchLyricsService.swift:233-252`).
  - Requests are ephemeral: no cookies, cache or credentials; **redirects refused**; timeouts 10 s per request / 15 s per resource; payload ≤128 KB (≤1 MB for search). Header `User-Agent: Vorssaint/<version> (https://github.com/vorssaint/vorssaint-utils)`, `Accept: application/json` (:356-410).
  - A match must agree on title, artist and album (when both have one), and duration within ±2 s (:147-160).
  - Results are cached: "no lyrics" 60 s, failure 15 s (:137), artist identities 15 s / 600 s / 3600 s (:262).
- **Consent.** Online lookup runs only with `notchLyricsOnline`. Off, the view shows an explanation plus an "Find lyrics online" button that flips the pref (`UI/Notch/NotchLyricsView.swift:93-106`). Lookups run only while the lyrics view is visible.
- **States** (`State`: idle, consent, loading, unavailable, failed, ready; Service:10):
  - **Loading**: spinner + "Loading lyrics…", plus an Import pill.
  - **Unavailable / failed**: empty view with a `quote.bubble` or `exclamationmark.triangle` glyph, a reason, and the next step (Retry or Find online) plus Import.
  - **Instrumental**: "Instrumental" plus Import.
  - **Synced**: timed lines, but only if the player reports a position. Lines are 17 pt bold, white; the active line at opacity 1.0, others 0.35, animated `.smooth(0.28)`. The active line auto-scrolls to center (`.smooth(0.28)`). Before the first line, "Waiting for the first verse" (caption). Empty text lines show "♪". Spacing 12, vertical padding 6, 24 pt edge fade (LyricsView:116-147).
    - The highlight is scheduled at exact line boundaries computed from `sampledAt / elapsed / rate`. There's no timer while paused (LyricsSupport:39-59).
    - **Timing row**: Import icon, "Timing", then − / offset / + in 0.25 s steps clamped to ±10 s. Clicking the value resets it (LyricsView:27-49; LyricsSupport:95-98).
  - **Plain / no position**: scrollable 15 pt medium text, selectable. When the player gives no position, the note "The player does not share its position." appears.
- **Import.** A file dialog for `.lrc` or plain text, ≤128 KB, parsed by `parse(_:duration:)` (handles `[offset:]`, ≤2000 lines). It's kept only in memory for this recording. The island reopens on Music afterwards (Service:282-343). Windows: `IFileOpenDialog`, owned by the island window, topmost.
- Memory holds one recording's lyrics and offset. A new recording clears both (`NotchLyricsMemory`, LyricsSupport:76-102).

### 1.9 Queue view (Mac only, see 1.7)
`UI/Notch/NotchQueueView.swift`:
- Header "Up next" (callout semibold), a mini spinner while loading or acting, and a refresh button.
- "The player did not switch to this song." in orange on failure.
- Rows: 34 pt cover, title (callout medium), artist (caption, secondary), and a play button (28 pt circle, `play.fill` 11 pt) only if the queue can play. Vertical padding 8. The list scrolls with an edge fade.
- Empty: "This player does not share its upcoming songs." / "No upcoming songs", plus "Open player".

### 1.10 Equalizer bars
`UI/Notch/NotchEqualizerBars.swift`:
- **Synthetic motion** (:130-171). Bar i of n (center c = (n−1)/2):
  - `envelope = (1 − |i−c|/max(1,c))^1.5`
  - `low = max(w, h·(0.12 + 0.25·env))`, `high = max(w, h·(0.12 + 0.88·env))`
  - Half-cycle period `π/(5.2 + 0.61·i)` s, phase lead `0.17·i`
  - Eased with `(1 − cos(π·t))/2` (DecorativeClock:65-70)
  - When not moving, bars rest at height = bar width (a dot). Bar pitch is `1.85·w`, corner radius `w/2`. Motion keeps its phase across metadata updates.
- **Live motion** (only with `notchLiveEqualizer`). Bar height = `max(w, h·(0.1 + 0.9·level))`. Levels arrive at 30 Hz, already smoothed. With fewer bars than bands, bar i samples band `floor((i+0.5)/bars·bands)` (`Services/Notch/NotchAudioLevelSupport.swift:114-118`).

### 1.11 Live audio level
- **macOS** (`Services/Notch/NotchAudioLevelService.swift`, `NotchAudioLevelSupport.swift`):
  - A Core Audio **process tap** (`CATapDescription(stereoMixdownOfProcesses:)`, macOS 14.4+) on the playing player's process, read through a private aggregate device.
  - FFT size 1024 (:709). **7 bands** with edges 50/120/250/500/1k/2k/4.5k/12k Hz, a 60 dB floor, **30 updates/s**.
  - Smoother: attack 0.55, decay 0.12, auto-gain ceiling decay 0.995, peak target 0.92 (Support:10-17, 122-143).
  - If the tap stays silent for **8 s** (no recording permission, or silence), it falls back to synthetic motion for that play only (:25, 57-69).
  - Pausing keeps the reader 1.5 s, so a skip doesn't blink (Service:141-151).
  - Disabled under Reduce Motion (:31-34).
- **Windows 11.**
  - **Recommended:** WASAPI **process loopback capture**. `ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, …, AUDIOCLIENT_ACTIVATION_PARAMS{ ActivationType = PROCESS_LOOPBACK, TargetProcessId = pid, ProcessLoopbackMode = INCLUDE_TARGET_PROCESS_TREE })` (Win10 2004+ / Win11). Run the same 1024-point FFT and 7-band math in Rust. There's no permission prompt on Windows, so the 8 s silence fallback rarely fires; keep it anyway.
  - **Fallback:** `IAudioSessionManager2` → for the session whose `IAudioSessionControl2::GetProcessId` matches → `IAudioMeterInformation::GetPeakValue` / `GetChannelsPeakValues` at 30 Hz. This gives **amplitude only, no spectrum**, so all bars would move together. Spread a single level across bars with the envelope above. That's a visible deviation (flag).
  - **PID mapping (flag).** SMTC gives an AUMID, not a PID. Map it by matching the AUMID/exe name against audio sessions' process image names (`QueryFullProcessImageNameW`). Browsers render audio in a child "audio" utility process, so use `INCLUDE_TARGET_PROCESS_TREE` on the browser's main PID.

### 1.12 Browser artwork
`Services/Notch/NotchBrowserArtwork*.swift`, opt-in `notchBrowserCovers`:
- **macOS.** Firefox-family browsers publish no cover, or a 60 px one for YouTube Music.
  - A cover counts as too small below **300 px** on its short side (Support:16, 25-32).
  - The lookup reads the browser's session file `sessionstore-backups/recovery.jsonlz4` / `sessionstore.jsonlz4` (mozLz4, ≤32 MB file / 128 MB decoded) from the newest profile under `~/Library/Application Support/<Firefox|zen|librewolf|Waterfox|Floorp>/Profiles` (Support:53-65).
  - It finds the YouTube tab matching the title and loads `https://i.ytimg.com/vi/<id>/maxresdefault.jpg`, then `mqdefault.jpg` (Support:251). The center square is cropped and re-encoded as JPEG q0.92 (Artwork:199-214).
  - For YouTube Music with no video tab it asks once: `https://itunes.apple.com/search` → 600×600 art (Support:171-200).
  - Retries at 0, 2, 5, 10, 18, 30 s, because Firefox saves its session ≤15 s after a change. A failed key cools down 60 s. 8 covers are remembered in memory (Artwork:17-22).
- **Windows.**
  - Chromium browsers and recent Firefox publish MediaSession artwork into the SMTC `Thumbnail`, so this lookup is **mostly unnecessary**. Keep it only as a fallback when the thumbnail is missing or < 300 px.
  - Paths: `%APPDATA%\Mozilla\Firefox\Profiles\*`, `%APPDATA%\zen\Profiles`, `%APPDATA%\librewolf\Profiles`, `%APPDATA%\Waterfox\Profiles`, `%APPDATA%\Floorp\Profiles`. Same file names, same mozLz4 format.
  - Identifying "a Firefox-family player" requires AUMID → exe mapping (flag).

### 1.13 Error / empty summary (music)
- **Command failed:** the artist line turns orange "Could not change playback." until the next command.
- **Adapter gave up** (restart budget exhausted): idle state (`awaitingPlayback = false`).
- **Lyrics / queue states:** see 1.8 and 1.9.
- **No source at all:** idle with the hint, or with the preferred-player buttons.

---

## 2. Notifications

### 2.1 What it shows
- **Expanded page** (`UI/Notch/NotchNotificationsView.swift:7-48`):
  - **Permission missing**: Accessibility permission row plus the privacy text.
  - **Empty**: a 56×56 `bell` tile (26 pt light, white 0.6, background white 0.05, radius 18) beside "New notifications will appear here" (headline), or "Waiting for the system notification service" when not attached. The privacy text below (caption, white 0.65) reads "Show new visible banners only. Messages stay in memory and are cleared when you lock this Mac or turn this off."
  - **List**: a rail of cards **240 pt wide**, rows of ≥120 pt, gap 8. The card height fills the page / rows (:35-42).
  - **Card** (:79-126): padding 12, background white 0.06, radius 14.
    - Header row (22 pt high, 11 pt, white 0.6): 18 pt app icon, app name, spacer, received time `HH:mm`, an Open button (`arrow.up.forward.app`) or a mini spinner while opening, and Dismiss (`xmark`).
    - Below: title 13 pt semibold (2 lines), subtitle 12 pt (1 line), body 12 pt at white 0.8 (fills, selectable).
    - Error line in orange caption: "This notification can no longer accept this action."
  - **Header button**: "Clear All" (trash). It asks for confirmation above the island because the island may hold the only copy (:53-75).
- **Closed (notice / banner)** (`Services/Notch/NotchNotificationSupport.swift:171-207`):
  - Beside a camera: the app icon (22 pt) and title (11 pt semibold) on the left wing; the message (subtitle · body, 11 pt regular) on the right.
  - Each wing is 36–190 pt with inset 16 and air 6. The message wraps to 2 lines if the strip is ≥30 pt tall.
  - A replacing banner keeps at least the previous banner's wings, so bursts don't resize the island (NS:2351-2353).
  - In the capsule: icon (`min(16, body−4)`), sender, message on one row, up to **380 pt** (NSup:982, 1084-1090).
- **Held preview.** With `notchOpenOnHover` on, a pointer resting on a banner for the hover delay expands it in place to the full message (NS:1708-1733). Layout (`NotchNotificationPreviewLayout`, Support:211-239; view :130-213):
  - Header 28 pt: 26 pt icon, app name 12 pt semibold at white 0.7, time, dismiss.
  - Title 14 pt semibold (2 lines), subtitle 13 pt medium (1 line), body 13 pt at white 0.82 (6 lines).
  - Action row 28 pt: "Open" (bordered, small). When more than one item is in the inbox, a pill with `tray.full` and the count opens the page.
  - A banner arriving while the pointer is already over the island is held at once (NS:2373-2379). Without hover it auto-dismisses after 3 s.

### 2.2 Interactions
- **Open** (card or preview): activate the original banner's default action, else launch the source app. On failure the item is marked unavailable and the page opens (NS:2382-2394; Service:225-254).
- **Dismiss** removes the item from the island inbox only. **Clear All** empties the inbox, disabled while an open is in flight (Service:155-167).
- Inbox: newest first, at most **50** (Support:71). Items that vanished natively stay listed but become non-openable (Support:35-52).
- The first scan after attaching only primes the seen set; **pre-existing banners are never imported** (Support:43-49).

### 2.3 Preferences
| Key | Default |
|---|---|
| `notchNotificationsEnabled` | true (routes the notice and enables the reader) |
| `notchDismissNativeNotifications` | false: "Hide the system banner" while the island shows it |
| `notchOpenOnHover` | false (shell pref; enables the held preview) |

The section is also gated by the module being in the island and feature availability (Support:130-135).

### 2.4 Data source — macOS
- No database is read. An **Accessibility observer** watches the `com.apple.notificationcenterui` process for window-created, layout-changed, destroyed and focus-changed events. A scan runs 0.03 s after a window is created, plus a debounced pass at 0.12 s (Service:56-122).
- Reading walks the AX tree on a budget: 0.8 s and 384 nodes per read (`Services/Notch/NotchNotificationReaderCore.swift:252`). It looks for `AXNotificationCenterBanner` / `Alert` roles and text by identifier header/title/subtitle/body (Support:69-70, 137-155).
- When Notification Center itself is focused, nothing is imported (ReaderCore:76-81).
- **Hiding the native banner** (opt-in): on macOS 27 the banner window is moved off-screen, so it still sounds and still reaches the Center. On earlier versions the banner is *closed* 1.2 s after it appears, which can clip its sound (Support:72-81, Service:169-219). Everything is put back when monitoring stops.
- The source app is resolved by name among running and installed apps, refreshed every 10 min (`NotchNotificationReader.swift:29-61`).
- The inbox is memory only, cleared on lock (privacy text).

### 2.5 Data source — Windows 11
- **Primary: `Windows.UI.Notifications.Management.UserNotificationListener`.**
  - Requires **package identity** (MSIX, or a sparse package with external location for the Tauri exe), the `userNotificationListener` restricted capability in the manifest, and user consent via `RequestAccessAsync()` (shows a system prompt). This is the Windows "permission row" equivalent of Accessibility.
  - `GetNotificationsAsync(NotificationKinds.Toast)` gives `UserNotification` with `Id`, `CreationTime`, `AppInfo.DisplayInfo.DisplayName` / `GetLogo(size)`, and `Notification.Visual.GetBinding(KnownNotificationBindingNames.ToastGeneric).GetTextElements()`. Map the first text to title, the rest to body; subtitle is usually absent.
  - The `NotificationChanged` event is unreliable for desktop apps. **Poll every ~1 s and diff by `Id`**, priming on the first poll exactly like the Mac inbox.
- **Open (flag).** The listener can't invoke a toast's launch arguments. "Open" can only activate the source app through its AUMID (`AppInfo.AppUserModelId` → `ActivateApplication` / `shell:AppsFolder`). That matches the Mac fallback path; the "exact banner" path is lost.
- **Dismiss native (flag, behavior differs).** `RemoveNotification(id)` deletes the toast from Action Center. There's no API to hide only the pop-up banner while keeping it in the center (Mac moves the window off-screen). Options:
  - (a) Offer "Hide the system banner" as "Remove from Notification Center after showing", with the same 1.2 s grace.
  - (b) Hide the setting on Windows. Recommendation: (b) for v1.
- **Do Not Disturb / Focus (flag).** The listener sees toasts that were suppressed as banners. The Mac mirrors only *visible* banners. Read `SHQueryUserNotificationState` (QUNS_BUSY / QUNS_QUIET_TIME / QUNS_PRESENTATION_MODE) or the Focus state, and don't raise island notices while banners are suppressed.
- **Fallback without identity (not recommended).** UI Automation on the toast pop-up window hosted by `ShellExperienceHost` / `explorer` (class `Windows.UI.Core.CoreWindow`, name "New notification"). This is the closest analogue to the Mac AX approach but fragile across builds.
- Clear the inbox on `WTS_SESSION_LOCK` (`WTSRegisterSessionNotification`).

### 2.6 Live activity / notices
- There's no compact activity. Each new item raises a `systemNotification` notice (priority 1, 3 s). When it's shown while closed (not expanded, no capture controls or drag), the native banner hide is requested (NS:4118-4127).

---

## 3. Calendar

### 3.1 What it shows
**Expanded page** (`UI/Notch/NotchCalendarView.swift`) has three layouts:
- **Wide** (page ≥ 420 × 300 pt): a month grid in a 196 pt column (scrollable), a 1 pt divider (white 0.12), and the agenda (header + list) (:23, 30-42).
- **Short** (default islands): a **week strip** (52 pt high) above a vertical agenda (:45-49).
- **Month grid over the strip** (short island, toggled by the month title): replaces the strip and agenda. Picking a day returns to the strip. Escape closes the grid first (:43-44, 73-77, 103-110).

**Band** (top row):
- Left: the month title ("October 2026", or abbreviated if it doesn't fit), 13 pt semibold. On short islands it's a button: a 28 pt high plate with padding 7 and radius 9, background white 0.12 while the grid is open (:115-142).
- Right: ‹ / today (`circle.circle`) / ›. These step by **week** over the strip and by **month** over a grid. An "Open Calendar" button (`arrow.up.forward.app`) appears when the row has room for 4 buttons (side ≥ 118) (:148-171).

**Week strip / month cells** (`UI/Notch/NotchCalendarMonthView.swift`):
- Weekday letter 9 pt medium, white 0.45.
- Day number in a circle: 26 pt (strip), 24 pt (month view), or row-derived 16–30 pt (grid, `calendarMonthRowHeight`, NSup:357-359). The number is 12 pt (strip) / 11 pt, bold when today or selected.
- **Today** = filled circle in `rgb(1, 0.36, 0.39)` (coral red) (MonthView:100).
- **Selected** = white circle with black text. Selected + today = coral with a 1.5 pt white @ 0.8 ring.
- Out-of-month days are white 0.4.
- Below the number: up to **3 event-color dots** (3 pt, gap 2).
- Optional week-number column (18 pt, 9 pt monospaced, white 0.45) when `notchCalendarWeekNumbers` is on (:252-278).
- Tapping a day selects it. Tapping the selected day again in the strip returns to "the coming week".

**Agenda** (:196-307):
- The header (32 pt min) shows "Week", or the selected day ("Fri 24 Oct", 13 pt semibold) plus a button back to the week (`calendar.badge.clock`).
- Groups are per day for the next 7 days (or the selected day). The day label is 11 pt medium at white 0.5, "Today" in white. Group spacing 14, row spacing 7.
- **Event card** (:370-451):
  - Padding 10 plus a 12 pt left inset for a 3 pt color bar (radius 2).
  - Background = calendar color at 0.10 (0.20 while ongoing), radius 11. Border white 0.05 (0.16 ongoing, 0.5 in high contrast), 0.75 pt.
  - Optional kicker, 9 pt semibold: "Now" in **mint** or "Next" at white 0.7.
  - Title 13 pt semibold (white 0.65 once ended).
  - Time line 11 pt monospaced at white 0.75: "All day", or `HH:mm · HH:mm`, or `d MMM HH:mm → d MMM HH:mm` across days. A `timer` glyph shows when the event is individually chosen to count down.
  - Calendar name 10 pt at white 0.5. Location with `mappin`, 10 pt at white 0.6.
- **Loading**: a small spinner (72 pt min height).
- **Empty**: a `calendar.badge.checkmark` glyph (26 pt light, white 0.45) and the empty text, plus an "Open Calendar" pill when the band can't hold it.
- **Permission card** (:309-335): a `calendar.badge.clock` glyph and an explanation.
  - "Allow" requests access and keeps the island open. "Settings" appears when denied or restricted.
  - A spinner shows while requesting. An orange caption shows if the request failed.

The page refreshes every minute (`TimelineView(.everyMinute)`). On day change it refocuses to today unless a day is selected.

**Closed strip** (capsule: `NotchCapsuleViews.swift:582-669`; notch: `UI/Notch/NotchCalendarStrip.swift`):
- Event color dot (6 pt, 0.5 pt white @ 0.5 ring), title (12 pt semibold in the capsule / 11 pt beside a camera, tail-truncated).
- Countdown `m:ss` (`countdownText`, `Services/Notch/NotchCalendarSupport.swift:307-310`), 13 pt semibold monospaced. White for upcoming, **mint** for "time left" in an ongoing event.
- Then the target time: `· HH:mm` for a start, `→ HH:mm` for an end (:302-305), 11 pt at white 0.55. It's dropped first when space is short.
- Ticks every 1 s with rolling digits (minutes only). The capsule max is **300 pt**.
- **Paired** with a companion (downloads / agents / music): the companion mark takes the title's place, leaving the dot + clock.
- Click: opens Calendar and scrolls once to that event (`revealing`, NS:1402-1409; View:232-248).

### 3.2 Countdown rules
`NotchCalendarSupport.swift:31-44, 239-294`:
- Only **timed** (non-all-day) events count. The lead window is **60 min** before the target.
- **Start countdown**: when `notchCalendarCountdown` is on, or the event is individually chosen (context menu "Add countdown" / "Remove countdown", shown only when the global countdown is off and the event is timed and in the future; View:279-284, 453-470).
- **Time-left countdown**: with `notchCalendarTimeLeft`, from `max(start, end−60 min)` until the end.
- The nearest target wins. At equal targets the ongoing one wins.
- Chosen countdowns are persisted as `{countdownKey: endDate}`. The key is the event id, or `id@occurrenceStart` for recurring events. They're refreshed on read and forgotten once ended. They're kept out of backups (Support:146-186).

### 3.3 Preferences
| Key | Default |
|---|---|
| `notchCalendarEnabled` | true |
| `notchCalendarCountdown` | false |
| `notchCalendarTimeLeft` | false |
| `notchCalendarWeekNumbers` | false |
| `notchCalendarExcluded` | [] (calendar ids hidden; new calendars start shown) |
| `notchCalendarChosenCountdowns` | unregistered dict |

### 3.4 Data source — macOS
`Services/Notch/NotchCalendarService.swift`:
- **EventKit** with full access, on a private actor (:9-49).
- Read window: today → +7 days. When a month is displayed, its 42-day grid (extended to +7 days for the current month). An extra read of the current week happens when countdowns need it (Support:74-91).
- Skips canceled events and those the current user declined (:19-21).
- Calendar color comes from `EKCalendar.cgColor`, with fallback `rgb(0.35, 0.65, 1)` (Support:12).
- **Refresh triggers**: `EKEventStoreChanged`, app activation, clock or time-zone change, day change, permission change (:132-139). A one-shot timer fires at the nearest of: next event start/end, midnight, now+15 min, or the next countdown window edge, with 1 s tolerance (:179-189; Support:337-343).
- **Open in Calendar**: `ical://ekevent/[<UTC start>/]<calendarItemIdentifier>?method=show&options=more`, forced to Calendar.app (Support:322-335; View:339-347).

### 3.5 Data source — Windows 11
- **API**: `Windows.ApplicationModel.Appointments.AppointmentManager.RequestStoreAsync(AppointmentStoreAccessType.AllCalendarsReadOnly)`. Then `FindAppointmentCalendarsAsync(FindAppointmentCalendarsOptions.None)` for the Settings list and colors. Then `FindAppointmentsAsync(start, duration, options)`.
  - Set `FetchProperties`: Subject, Location, StartTime, Duration, AllDay, CalendarId, IsCanceledMeeting, UserResponse, Recurrence, OriginalStartTime, LocalId, RoamingId.
  - Filter out `IsCanceledMeeting` and `UserResponse == Declined`.
  - Map: `AppointmentCalendar.DisplayColor` → color, `DisplayName` → calendar, `SourceDisplayName` → account grouping (replaces `NotchCalendarChoice.source`).
  - `countdownKey` = `LocalId` or `LocalId@OriginalStartTime`.
- **Identity requirement (flag).** The appointments store needs package identity plus the `appointments` capability. Use the same MSIX / sparse package as notifications. Access is consent-based through Settings ▸ Privacy ▸ Calendar. Map `denied` to the "Settings" button, which opens `ms-settings:privacy-calendar`.
- **Change notifications**: `AppointmentStore.StoreChanged` (call `ChangeTracker.Enable()`). Day / clock / time-zone change: `WM_TIMECHANGE` and `WM_SETTINGCHANGE("intl")` / `SystemEvents.TimeChanged`. Keep the same next-boundary timer.
- **Open event**: `AppointmentManager.ShowAppointmentDetailsAsync(localId, instanceStartDate)`, which opens the default calendar handler. "Open Calendar" without an event: `outlookcal:` URI, falling back to `ms-settings`-registered default.
- **BIG RISK (flag).** The store is populated by the legacy *Windows Mail and Calendar* app, which Microsoft retired at the end of 2024. The *new Outlook for Windows* does **not** sync into `AppointmentStore`. On many current machines the store is empty or stale. The task statement accepts "Google later", but Microsoft 365 / Outlook.com via Microsoft Graph (`/me/calendarView`) is likely needed early too. Keep the reader behind a provider interface so several sources (WinRT store, Graph, Google) can merge.
- **Week start / week numbers**: use the user's locale first day (`GetLocaleInfoEx(LOCALE_IFIRSTDAYOFWEEK)`). Week numbers follow the same calendar's week-of-year, not ISO, to match Mac behavior (Support:129-135).

### 3.6 Live activity / notices
- Compact priority #6, inside the 60-minute window only. It can pair with downloads, agents or music, and can be a companion to the timer. There's no calendar notice event.
- The Controls tile uses `tileEvent` (next timed start within the week) and `tileStartText` (`HH:mm` today, else `EEE HH:mm`) (Support:265-280). That's the controls spec.

---

## 4. Timer (timer / Pomodoro / stopwatch)

### 4.1 Model
`Services/Notch/NotchTimerSupport.swift:85-172`:
- Modes: `timer` (1–180 min, `notchTimerMinutes` default 15), `pomodoro`, `stopwatch`.
- Pomodoro defaults: focus 25 (1–180), short break 5 (1–60), long break 15 (1–60), long-break interval 4 (1–24), total sessions 4 (1–24). The running cycle takes an immutable copy (:32-59).
- The anchor is a **continuous monotonic clock** (`ContinuousClock`, which counts through sleep) (`NotchTimerService.swift:12-23`). Countdown: anchor = deadline. Stopwatch: anchor = zero time.
- Pause stores the reading, resume re-anchors it. Finishing sets `completed`, counts focus sessions, and never auto-starts the next phase: "Start <next phase>" is an explicit action (:156-169).
- Next phase: after a focus, the long break if completedFocuses is a multiple of the interval, else the short break. After a break, focus. The cycle finishes when completedFocuses ≥ totalSessions.
- **Memory only**: never persisted, never restored on relaunch (Service:7-8).
- **Windows**: use `QueryInterruptTime` / `GetTickCount64` (both include sleep) to match `ContinuousClock`. Not `QueryUnbiasedInterruptTime`, which excludes sleep. Schedule completion with a waitable timer, or recompute after `PBT_APMRESUMEAUTOMATIC`.

### 4.2 Expanded page
`UI/Notch/NotchTimerView.swift`. The page tint is **orange** (system orange).

**Setup (no session)** (:57-80):
- Band, left: mode picker. Three words (12 pt medium, padding 4, height 24, spacing 14): selected word white with a **2 pt orange underline** that slides between words (`matchedGeometryEffect`, `.spring(response: 0.3, dampingFraction: 0.8)`); others white 0.5. Beside a camera, or when the three don't fit, it becomes a menu: the current name + chevron, minimum scale 0.7, or the mode glyph if even that doesn't fit (:98-133, 147-174; TimerSupport:197-224).
- Band, right: **Start** capsule (12 pt semibold orange text, horizontal padding 10, height 28, background orange 0.18). Tooltip "pomodoroHint" in Pomodoro (:83-96).
- Body: the **ruler** (82 pt high, minimum 56) and a setup clock (26 pt **thin**, monospaced, orange) side by side, spacing 12. The stopwatch has no ruler.
- Pomodoro adds a 28 pt row of four readouts (short break, long break, interval, total): name 9.5 pt medium secondary, value 12 pt semibold orange + chevron. Each opens a menu of every allowed value. They spread evenly, or scroll horizontally when narrow (:250-288).

**Ruler** (`UI/Notch/NotchTimerRuler.swift`), a custom-drawn slider, 1–180 min:
- **14 pt per minute** (`NotchTimerRulerScale.spacing`, TimerSupport:11-30).
- Ticks are 4 × 38 pt rounded at y = 26 pt (scaled by height/82). Minutes ≤ selected are orange at full alpha; above, 0.35 (0.65 in high contrast). Ticks fade over 12 % of width at both edges.
- Labels at minute 1 and every 5: 14 pt medium monospaced, "1h05" style past 60.
- A center pointer triangle points up from y = 70→80 (scaled).
- Disabled = alpha 0.4. Keyboard focus = 1 pt orange rounded rect (radius 7).
- **Input** (:70-155):
  - Drag: value moves by −dx/14 (dragging left increases).
  - Click without drag: jumps so the clicked tick comes to center.
  - Wheel / trackpad: dominant axis; precise deltas in points, line deltas × 14.
  - Arrow keys ±1. Home = 1, End = 180.
  - Each step plays haptic feedback on Mac (no Windows equivalent; omit).

**Running / paused / completed** (:186-244):
- Band, left: phase title, 13 pt semibold orange ("Timer", "Focus", "Short break", "Long break", "Stopwatch", "Finished", or "Pomodoro finished"), plus "Session n of N" (12 pt secondary monospaced) on one line, or two when needed.
- Band, right: `xmark`. Cancel, or Done when completed.
- Body: a round button, 52 pt circle at orange 0.28 with a 23 pt semibold orange glyph: pause / resume, or "Start <next phase>" when a Pomodoro phase completed. On the right, the **clock 62 pt thin monospaced orange**, min scale 0.5, rolling digits each second.
- Clock format: `MM:SS`, or `H:MM:SS` at ≥ 1 h. Countdowns round **up**; the stopwatch rounds **down** and saturates at 99:59:59 (TimerSupport:249-264).
- Ticks align to whole-second boundaries (`tickScheduleOffset`, :303-320). On Windows, schedule the next tick at the boundary, not every 1000 ms from an arbitrary start.

### 4.3 Closed strip
- **Capsule** (`NotchCapsuleViews.swift:343-396`): mark + reading.
  - Mark: 12 pt orange glyph `timer` / `stopwatch` / `pause.circle` / `checkmark.circle`, or the companion's mark (music cover, download arrow + %, agent marks, event dot + clock).
  - Reading: 13 pt semibold monospaced orange, rolling minutes.
  - Max **220 pt** (`Maximum.activity`).
- **Beside a camera** (`UI/Notch/NotchTimerStrip.swift`): mark or companion left (icon `min(20, h − 10)`), reading right (text `min(16, h − 6)` pt medium, min scale 0.65). Wings are 44–64 pt (TimerSupport:335). The mark appears at wing ≥ 28 and the reading at ≥ 42.
- **Compact text** (TimerSupport:297-329):
  - ≥ 1 h: `1h35`.
  - Otherwise a narrow unit style: `15m`, `45s` (locale-formatted, rounded up).
  - Stopwatch: `MM:SS` until 1 h, then `1h35`.
- When the reading changes length (10m → 9m) the island re-measures (`readingShape`).
- Click: the mark opens the timer (or the companion's page); the reading opens the timer.

### 4.4 Alerts
- On finish:
  - Notice `timer`: title "Finished" or "Pomodoro finished", detail = phase name, priority 2, 6 s, orange.
  - Mascot reaction.
  - **Sound** if `notchTimerSoundEnabled` (true): `/System/Library/Sounds/Glass.aiff`, **repeated every 2 s for up to 5 min**, until dismissed / cancelled / next phase started. The deadline survives suspend and redraws, so the alarm can't restart (`Services/Notch/NotchTimerAlert.swift:8-66`).
- **Windows (flag).** Apple's Glass.aiff can't be shipped. Bundle an original short chime (or use a Windows system sound such as `Windows Notify` / `Alarm01`) via WASAPI or `PlaySound(SND_ASYNC)`. Keep the 2 s / 5 min loop.
- Suspension (island off, lock) cancels pending tasks but keeps the session. On resume `finishIfDue` runs immediately (Service:25-31, 67-73).

### 4.5 Preferences
`notchTimerEnabled` (true), `notchTimerMode` ("timer"), `notchTimerMinutes` (15), `notchTimerSoundEnabled` (true), `notchHideTimerCountdown` (false: hides the running countdown from the closed island; a finished timer always shows), and the `notchPomodoro*` values above.

### 4.6 Priority
Compact #1 (the highest). It can pair as primary with downloads, and while running with agents / calendar / music.

---

## 5. Downloads

### 5.1 What it shows
- **Setup** (folder not chosen, unavailable, or feature off) (`UI/Notch/NotchDownloadsView.swift:36-121`): a 56×56 glyph tile (`arrow.down.circle` or `exclamationmark.triangle`), copy (12 pt, white 0.65, or orange "folder unavailable"), and actions: a "Choose Folder…" capsule (12 pt semibold, padding 14, height 28, white 0.14), "Clear Folder", or an enable switch when off.
- **Waiting** (folder set, nothing in it): empty view with `arrow.down.circle` and "waiting".
- **List** (:141-149, 172-231): newest first. Cards: padding 12, white 0.07, radius 14, spacing 8.
  - **Completed**: green `checkmark.circle.fill` + name (12 pt medium, middle truncation) as a button that opens the file; "Show in Finder" (`folder`); "Add to Shelf" (`tray.and.arrow.down`) when the shelf exists; caption "Saved".
  - **In progress**: `arrow.down.circle` + name + `NN%`; a 5 pt meter (or a small spinner when the size is unknown); caption2 row: "In progress" ··· received bytes (file-style) or "total unknown".
- **Band**: a folder menu (folder glyph + name, middle-truncated) with Choose / Clear (:160-170).
- **Closed strip**:
  - **Capsule** (CapsuleViews:507-540): `arrow.down.circle.fill` (12 pt), file name (12 pt medium, middle truncation), a 36 pt meter (4 pt high), and the percent (11 pt monospaced) or a mini spinner. Max **280 pt**.
  - **Beside a camera** (DownloadsView:234-302): arrow (`min(17, h − 10)` pt) plus the name when the wing is ≥ 64 pt; percent 10 pt monospaced (min scale 0.75) on the right with a meter when the name shows. The wing is 56 pt, growing to 64–160 when the menus leave room for the name (`Services/Notch/NotchDownloadSupport.swift:185-239`).
  - Click opens Downloads.

### 5.2 Rules
- Partial-file extensions: `crdownload`, `download` (Safari package), `part` (Support:263-268). An item is active while its partial was modified within **120 s** (Service:364-386).
- A completed item stays in the "finished" list for **15 s** (max 5) and then only lives in the folder listing (Service:351-362). The folder listing shows ≤ 200 newest entries and observes ≤ 32 partials (Support:181-183).
- Completion is only claimed with evidence: a progress object finished, the file identity (dev:inode) matched the partial's, or the browser's announcement. A vanished partial with no file raises `onFailure` after a 2 s grace, which triggers the mascot's "confused" reaction (Service:287-349).

### 5.3 Preferences
`notchDownloadsEnabled` (true), `notchDownloadsFolderBookmark` (security-scoped bookmark; none by default, so the user must pick a folder; the dialog starts in `~/Downloads`).

### 5.4 Data source — macOS
- A kqueue `DispatchSource` on the folder and each partial (write / extend / attrib / rename / delete / revoke), debounced 0.3 s. A folder rename or delete stops the watch and marks the folder unavailable (Service:259-285).
- `Progress.addSubscriber(forFileURL:)`: browsers publish `NSProgress` with `fileOperationKind == .downloading` for files directly in the folder. This gives the **fraction** (Support:248-261; `NotchDownloadProgressObserver.swift`).
- Distributed notification `com.apple.DownloadFileFinished` (object = final path) (Support:300-317).

### 5.5 Data source — Windows 11
- **Folder.** Default `SHGetKnownFolderPath(FOLDERID_Downloads)`. Persist a plain path (no bookmark concept). If the folder disappears, mark it unavailable.
- **Watch.** `ReadDirectoryChangesW` (overlapped, `FILE_NOTIFY_CHANGE_FILE_NAME | SIZE | LAST_WRITE`), non-recursive. Same 0.3 s debounce and rescan model.
- **Partial-file rules per browser:**
  - **Chrome / Edge / Brave / Opera / Vivaldi**: `<name>.crdownload` (Chrome also uses `Unconfirmed NNNN.crdownload` before the name is known), renamed on finish.
  - **Firefox**: `<name>.part` **plus a 0-byte placeholder `<name>`** created at the start. The placeholder must not count as "completed" until the `.part` is gone and the size is > 0 (or matches). New rule vs Mac.
  - Add `.partial` (legacy Edge) and `.opdownload` (old Opera) to the list.
- **Progress fraction (flag).** There's no cross-process `NSProgress` equivalent; taskbar progress (`ITaskbarList3`) is write-only. On Windows the fraction will usually be **unknown**: show the spinner and received bytes. Optional enhancement: read Chromium's `History` SQLite (`downloads` table: `total_bytes`, `received_bytes`, `state`), copying the file first since it's locked. It updates only every few seconds. Firefox has no live equivalent.
- **Completion evidence.** Rename from the partial name onto the final name, with file identity via `GetFileInformationByHandle` (volume serial + file index) instead of dev:inode. There's no browser "finished" broadcast.
- **Actions.** Open: `ShellExecuteW(open)`. "Show in Finder" → "Show in folder": `SHOpenFolderAndSelectItems`. Beep: `MessageBeep(MB_ICONWARNING)`.

### 5.6 Priority / notices
- Compact #4.
- On completion: notice `download` with title "Download completed", detail = file name, `arrow.down.circle.fill`, priority 0, 6 s, plus mascot "celebrate" (NS:4019-4025).

---

## 6. Watch (live reading of a screen region)

### 6.1 Flow
`Services/Notch/NotchWatchService.swift`:
1. **Choose.** If screen-recording access is missing, request it. Otherwise show the area-selection overlay (the screenshot selector in geometry mode; Vorssaint windows hidden; no pointer; no frozen frame; purpose text shown) (:87-108).
2. **Resolve the target** (:110-151):
   - If the selection hit a window, the target is that window with a crop in points from its top-left. Otherwise the window under the area's middle (front to back) is picked, clipped to ≥ 8 × 8 pt (`windowCrop`, `NotchWatchSupport.swift:354-361`).
   - With no window (desktop or menu bar), a **fixed display region** in pixels, keeping the island itself out of the capture.
   - The island opens on the Watch page.
3. **Loop** (:247-261): read every **1 s** while the page is open and the island expanded, else every **2 s**.
4. **Read** (:269-357):
   - If the window is gone → finish `.closed`.
   - Missing permission / minimized / screen locked / no image → state `hidden` (keep watching).
   - Capture the whole window, crop, compute a **16 × 16 grey fingerprint** (levels /16).
   - If it's the same as last time (≤ 3 cells nudged, none moved by > 1 level) and the last OCR is < **10 s** old, skip OCR but still feed the tracker.
   - Otherwise OCR the crop, downscaled so the longest side is ≤ **900 px**, with UI-language fallbacks.
   - Publish the preview image, full text and headline.
5. **Headline** (`headline`, WatchSupport:178-208):
   - A percentage first (sign and spaced thousands kept).
   - Else the first line if ≤ 22 chars.
   - Else a clock (`h:mm[:ss]`), else an amount with a ≤ 3-letter unit.
   - Else the first line cut to 21 chars + "…".
   - The person can pin a specific line (lines are listed, max 12).
6. **Rules** (`NotchWatchTracker`, WatchSupport:58-144):
   - **changes**: the signature (normalized text, or the image fingerprint when there's no text) differs from the baseline continuously for **3 s**.
   - **settles**: it changed at least once, then stayed unchanged for **30 s**.
   - **contains**: the normalized text (case-, diacritic- and width-insensitive, whitespace collapsed) contains the typed text.
   - **reaches**: the first percentage, else the first number, crosses the typed target from the starting side (or equals it at the start). Numbers parse with both decimal marks plus locale disambiguation (:216-268).
   - Typed text and numbers apply only on Return or on leaving the field (WatchView:199-266).
7. **Finish** (:359-378):
   - Sound if `notchWatchSound` (Glass).
   - Notice `watch` (title = outcome: changed / settled / "Shows ‘X’" / "Reached X" / closed; detail = app name; priority 2, 6 s) unless already on the page. Mascot reaction: perk / celebrate / confused.
   - If no notice could show (fullscreen, locked), a **system notification** is posted instead (`Notifier.post`).

### 6.2 What it shows
**Setup page** (WatchView:271-335): a 56 pt `eye` tile and copy. The action is the enable switch, "Allow access" (screen recording), or "Choose area…".

**Watching page** (:38-110):
- Band, left: app icon (16 pt) + **App** · window title (12 pt; secondary after the dot; middle truncation).
- Band, right: "Choose again" (`viewfinder`), "Stop" (`xmark`).
- **Area card** (:114-197): padding 10, white 0.07, radius 14.
  - The live preview fits the card, radius 8; opacity 0.35 when hidden, with an overlay label "hidden" (`eye.slash`) or the permission hint (`lock`).
  - Below: either the headline menu (15 pt semibold monospaced; "In island" section: Automatic + lines) or the outcome row (green `checkmark.circle.fill`, or secondary `xmark.circle.fill` when closed, 13 pt semibold).
  - The full text as a 2-line caption (selectable) when it differs from the headline.
- **Rule row**: "Tell me when" + condition menu (glyphs `arrow.triangle.2.circlepath`, `pause.circle`, `text.magnifyingglass`, `flag.checkered`), plus a text or number field (24 pt high, max 180 pt, white 0.08, radius 7, 1 pt white 0.34 focus ring). Settles adds a hint line.
- **Footer**: "Watching since HH:mm" (caption2), or after finishing "Watch again" (if the target still exists) plus "Choose again".

**Closed strip**:
- **Capsule** (CapsuleViews:544-578): breathing **eye** + headline (12 pt monospaced), a 40 pt-wide thumbnail when the area has no text, or a mini spinner before the first read.
- **Beside a camera** (WatchView:384-445): eye left (purple at 0.75 alpha, breathing), reading right in **purple** (`min(16, h − 6)` pt, min scale 0.7, no rolling animation). Wings 56–150 pt (WatchSupport:157).
- **Eye breathing**: scale 0.84 → 1.0 and opacity 0.7 → 1.0, cosine ease, half-cycle **1.2 s** (`NotchAgentAnimationView.swift:155-162`). Static `eye.slash` while hidden.

### 6.3 Preferences
`notchWatchEnabled` (true, but the feature's availability flag is migrated to off for existing users, Def:2261-2265), `notchWatchSound` (true), `notchWatchCondition` ("changes"). The typed match text and number aren't persisted.

### 6.4 Windows 11 equivalent
- **Capture a window even when covered**: `Windows.Graphics.Capture` with `IGraphicsCaptureItemInterop::CreateForWindow(hwnd)`.
  - Keep **one** `GraphicsCaptureSession` alive per watch and grab the latest frame from a `Direct3D11CaptureFramePool` (FreeThreaded, 1–2 buffers) at each tick. Starting a session per tick would flash the capture border.
  - Set `IsCursorCaptureEnabled = false`.
  - **Yellow border (flag)**: `IsBorderRequired = false` needs Windows 11 and `GraphicsCaptureAccess.RequestAccessAsync(GraphicsCaptureAccessKind.Borderless)`. Behavior for unpackaged apps should be verified; it may require identity or show a consent prompt.
  - **Minimized windows** deliver no new frames → map "no frame within N ms" (or `IsIconic`) to the `hidden` state. A window on another virtual desktop still captures.
- **Display region**: `CreateForMonitor(hmonitor)` + crop. Exclude the island from captures with `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)` on all island windows. That also covers the Mac's `hideVorssaintWindows` and `keepsIslandOut`.
- **Window metadata**: `IsWindow` (closed detection), `GetWindowTextW` (title), `GetWindowThreadProcessId` → process → app display name (the exe's `FileDescription`, or the AUMID via `SHGetPropertyStoreForWindow(PKEY_AppUserModel_ID)`) and icon. The crop is stored in DIPs relative to `DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)`.
- **OCR**: `Windows.Media.Ocr.OcrEngine.TryCreateFromUserProfileLanguages()`, or `TryCreateFromLanguage` per the UI language, then `RecognizeAsync(SoftwareBitmap)`.
  - Check `OcrEngine.MaxImageDimension`.
  - **Quality (flag)**: Windows OCR is weaker than Vision on small UI text. **Upscale** small crops (text under ~20 px tall) by 2–3× before OCR, and still cap the long side.
  - OCR language packs must be installed (`Windows.Globalization.Language.IsSupported`).
  - Line order comes from `OcrResult.Lines` (top to bottom).
- **Permission**: Windows has no screen-recording permission (except Win11's per-app "Screenshot borders" / capture privacy setting in recent builds, flag). The "Allow access" UI is mostly not needed; keep the state for an access-denied result from `RequestAccessAsync`.
- **Fallback notification** when the island can't show: a Windows toast via `ToastNotificationManager`. It needs an AUMID: identity, or a Start-menu shortcut with `System.AppUserModel.ID`.
- **Sound**: same caveat as the timer (no Glass.aiff).

### 6.5 Priority
Compact #2.

---

## 7. Keep Awake strip

- The section has no page; clicks open **Controls**, where the Keep Awake tile lives (NSup:845-856). The strip is shown only when `notchKeepAwakeActivity` (default **false**) is on and a session is active (`Services/Notch/NotchKeepAwakeSupport.swift:15-18`).
- **Capsule** (CapsuleViews:674-732): `cup.and.saucer.fill` (12 pt, **yellow**) + time left (13 pt semibold monospaced yellow: `45m`, `1h35`), or an `infinity` glyph for an open-ended session.
- **Beside a camera** (`UI/Notch/NotchKeepAwakeStrip.swift`): cup left (icon `min(20, h − 10)`, wing ≥ 28), time or infinity right (wing ≥ 42), all yellow.
- Minutes left round **up** (min 1). The text uses the timer's compact shapes, including past 3 h (KeepAwakeSupport:20-33). It ticks **once a minute**, aligned to the minute boundary of the remaining time (:35-39).
- Accessibility value: "Ends in …", "Until turned off", or the automation conditions (Strip:91-100).
- **Data source**: `KeepAwakeManager` (`Services/KeepAwakeManager.swift:22-23, 670-710`) holds `isActive`, `endDate` (nil = indefinite) and `sessionTrigger` (manual / automation). It's implemented with IOPM assertions `PreventUserIdleSystemSleep` (+ `PreventUserIdleDisplaySleep`).
- **Windows**: `PowerCreateRequest` + `PowerSetRequest(PowerRequestSystemRequired / PowerRequestDisplayRequired)` with a reason string (shows in `powercfg /requests`). This beats `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED)`, which is tied to one thread. Neither blocks lid-close or power-button sleep on Modern Standby devices. That matches Mac idle-only semantics, but say so in the UI copy.
- **Priority**: compact #8 (the last).

---

## 8. System (CPU / memory / etc. page)

### 8.1 What it shows
`UI/Notch/NotchSystemView.swift`:
- A grid of cards, each **≥ 128 pt wide × 72 pt high** (NSup:299-300). Rows are balanced so every row is filled with no empty cells (NSup:477-487). Gap 10; outer inset `ceil(width·0.015)+1` for hover growth (NSup:302).
- The grid scrolls only if it's taller than the page.
- **Card** (:130-157): radius 18, control-surface background, padding 12.
  - Label (glyph + title) 10 pt medium secondary.
  - Value: **22 pt medium rounded** monospaced (15 pt when there's a detail line), numeric roll `.smooth(0.25)`, min scale 0.75, "…" when unknown.
  - Then either a detail line (11 pt secondary) or a 5 pt meter. The meter is white, or **orange** when attention is needed.
- Click opens the metric's detail page (`selectedMetric`; metrics spec).
- Empty state when no monitor features exist: `gauge` glyph + "monitor unavailable".

Cards, in order (:27-87):

| Card | Value | Meter / detail | Attention when |
|---|---|---|---|
| CPU | % | meter | ≥ 85 % |
| GPU | % | meter | ≥ 85 % |
| Memory | used/total % | meter | ≥ 85 % |
| Battery (only with a battery) | charge % (bolt glyph when charging) | meter | ≤ 20 % and not plugged in |
| Network | ↓ rate | detail ↑ rate (unit `networkSpeedUnit` bytes/bits) | — |
| Disk available | free bytes of `/` (else the first internal disk) | meter = free fraction | < 10 % free |
| Power | system watts | detail adapter watts when plugged in | — |
| Fans (only if fans reported) | first fan RPM | — | — |
| Connected devices | count | — | — |

- **Data source (Mac)**: `SystemMonitor` (`Services/SystemMonitor/SystemMonitor.swift`): host statistics, IOKit / IOReport for GPU, SMC for fans / temperatures / power, IOPS for battery, interface counters. Default cadence **2 s** (:145), and it samples only while a surface needs it.
- **Windows 11:**

| Metric | API |
|---|---|
| CPU | PDH `\Processor Information(_Total)\% Processor Utility` (matches Task Manager; `% Processor Time` reads lower on turbo CPUs), or `GetSystemTimes` deltas |
| GPU | PDH `\GPU Engine(*)\Utilization Percentage`. Sum per engine type (3D, Copy, VideoDecode…) across processes, take the max type (Task Manager method) |
| Memory | `GlobalMemoryStatusEx`: used = `ullTotalPhys − ullAvailPhys`. Note that the Mac "used" excludes cached memory; this is close |
| Battery | `GetSystemPowerStatus`, or `Windows.Devices.Power.Battery.AggregateBattery.GetReport()` (charge %, charging status) |
| Network | `GetIfTable2` byte-counter deltas, excluding loopback, tunnel and virtual adapters (`InterfaceAndOperStatusFlags.HardwareInterface`) |
| Disk | `GetDiskFreeSpaceExW(%SystemDrive%\)` |
| Power watts (flag) | Only battery `ChargeRateInMilliwatts` (negative = discharge) from the battery report. **No total system draw** on desktops or while on AC. No adapter wattage API. Hide the card when unavailable |
| Fans (flag) | No standard API (`Win32_Fan` WMI is almost never populated; vendor SDKs only). Hide the card |
| Connected devices | `Windows.Devices.Enumeration` (Bluetooth + USB); see the connected-devices spec |

- **Priority**: no live activity (the battery notice and the low-battery tint belong to the shell spec).
- **Open Activity Monitor** (`MetricDetailView.swift` `ActivityMonitorButton`, beside the CPU and memory details' process list). **Windows:** opens Task Manager with `ShellExecuteExW("taskmgr.exe")`; `CreateProcess` fails for admins with error 740, as Taskmgr asks for the highest level available. A running Task Manager comes to the front.

---

## 9. Lock screen (brief)

- **macOS behavior** (`Services/Notch/NotchLockScreenService.swift:9-15, 98-167`; `NotchLockScreenSupport.swift`; `UI/Notch/NotchLockScreenView.swift`). Opt-in `notchLockScreen` (false); `notchLockSounds` (false).
  - While locked, the island draws over the lock screen. Its windows join a private CGS Space at absolute level **400** (above the lock screen's 300).
  - It shows: a padlock in the island at the camera (animated closing 0.35 s after lock); a line of activities under the clock (timer, calendar, agents, downloads); and a **music player pane** on Liquid Glass (404–460 pt wide, ≥ 190 pt high) between the clock and the login controls, with cover glow in the artwork tint and play/pause/skip/scrub.
  - Only the player pane takes clicks, and it never becomes key, so the password field keeps focus. Panels fade in over 0.35 s.
  - Music shows while playing, or once it has played during this lock.
  - Optional padlock lock/unlock sounds.
- **Windows: not portable.** The lock screen runs on the secure Winlogon desktop, and no third-party window can be drawn there. There's also no public lock-screen widget API for Win32 apps (lock-screen "detailed status" is limited to a few inbox apps on Windows 11).
- **What still matters on Windows:**
  1. **Lock/unlock lifecycle**: `WTSRegisterSessionNotification` → `WTS_SESSION_LOCK` / `UNLOCK`. On lock: suspend the timer tasks, stop media and SMTC readers if desired, **clear the notifications inbox** (privacy promise), hide the island. On unlock: resume, run `finishIfDue`, re-read sources.
  2. **Lock / unlock sounds**: can still play from the session on the lock and unlock events (Mac plays the padlock sounds). Ship original sounds.
  3. **Music on the lock screen is already native**: Windows 11 shows SMTC media controls on the lock screen for the active session. No work is needed beyond publishing correct SMTC data (nothing to do: we only read SMTC).
  4. Recommendation: hide the "Show over the lock screen" setting on Windows. Optionally keep "Lock sounds".

---

## 10. Open questions / risks

1. **Package identity is a gating decision.** UserNotificationListener (notifications), AppointmentStore (calendar), probably borderless Windows.Graphics.Capture, and toast fallbacks all need identity. Ship as MSIX, or as unpackaged Tauri + a sparse package (external location)? This affects the installer, auto-update and signing.
2. **The calendar source may be empty.** The retired Mail & Calendar app was the main writer to `AppointmentStore`; the new Outlook doesn't sync there. Decide whether Microsoft Graph (and Google) must land in v1 for Calendar to be useful.
3. **Notification parity gaps**: the exact banner can't be activated (only its app); the pop-up banner can't be hidden (only removed from Action Center); Focus / DND suppressed toasts must be filtered by hand. Confirm acceptable UX.
4. **No queue on SMTC.** Hide "Up next" on Windows, or add Spotify Web API (OAuth) later?
5. **SMTC → PID / exe mapping** is heuristic but required for the live equalizer (process loopback), Firefox-family detection and app icons. Validate with Spotify (Win32 and Store), Apple Music, Media Player, Chrome, Edge, Firefox and YouTube Music PWA.
6. **SMTC timeline quirks.** Some apps (notably Spotify and browsers) update `TimelineProperties` rarely or with a stale `LastUpdatedTime`. The 2 s seek tolerance and 1 s scrub hold may need per-app tuning. `PlaybackRate` is often null → default 1.
7. **Download progress fraction** is mostly unavailable on Windows; the strips will usually show a spinner instead of a percent and meter. Is the Chromium History DB read acceptable (privacy, file locking)?
8. **OCR quality** of Windows.Media.Ocr on small, anti-aliased UI text vs Apple Vision. Needs a test corpus (terminal output, progress bars, percentages) and an upscale strategy. It also requires the OCR language packs.
9. **Capture border and consent** for Windows.Graphics.Capture borderless mode for unpackaged apps; verify on Win11 23H2 / 24H2.
10. **Sounds**: Apple's Glass.aiff (timer, watch) and the macOS padlock sounds can't be redistributed. Commission or choose original replacements.
11. **Monotonic time across sleep**: confirm the Rust clock choice (`QueryInterruptTime`) so timers and calendar countdowns behave like `ContinuousClock` across Modern Standby.
12. **Haptics** (ruler steps, notice hold) have no Windows equivalent and are dropped.
13. **Music-app classification and the preferred-player list** need a maintained AUMID allowlist; Windows has no "music category" metadata.
14. **Power and fan cards** will be missing on most Windows machines; decide whether to hide them or show "—".
15. `Support/NotchMediaPresentationProbe.swift` is a dev-only probe for the *Files / media tools* page (video compressor), not the music player. It isn't relevant here and needs no port.
