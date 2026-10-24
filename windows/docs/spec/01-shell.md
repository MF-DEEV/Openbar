# 01 · Island shell: behavior spec (macOS → Windows 11)

Scope: the island **shell** (window, silhouette, sizes, state machine, motion, navigation chrome, quick-access bubbles, notices framing, settings that drive the shell). Section page *contents* (music player, timer ruler, clipboard list, sessions, mascot/companion, lock screen, Command Bar internals) are other specs; this doc only gives the frame they live in.

Source commit: `482540dd` (branch `feat/windows`). All references are `File.swift:line`. File legend (all under `Sources/Vorssaint/`):

| Short name | Path |
|---|---|
| `NotchSupport.swift` | `Services/Notch/NotchSupport.swift` (geometry, layout constants, motion math) |
| `NotchService.swift` | `Services/Notch/NotchService.swift` (state machine) |
| `NotchWindowHost.swift` | `Services/Notch/NotchWindowHost.swift` (window, mask animation, hit testing) |
| `NotchView.swift`, `NotchComponents.swift`, `NotchCapsuleViews.swift`, `NotchSectionsView.swift`, `NotchQuickAccessView.swift`, `NotchNoticeView.swift`, `NotchScrollEdgeFade.swift`, `NotchLevelSlider.swift`, `NotchDecorativeClock.swift`, `NotchUpdateControl.swift`, `NotchIslandAlert.swift`, `NotchMirrorView.swift` | `UI/Notch/…` |
| `NotchGestureSupport.swift`, `NotchSectionPaging.swift`, `NotchOverlaySpace.swift`, `NotchMenuBarSpace.swift`, `NotchSliderEditing.swift`, `NotchVolumeKeyGate.swift`, `NotchNotificationSupport.swift` | `Services/Notch/…` |
| `NotchSettings.swift`, `NotchSettingsTabRow.swift`, `NotchLayoutEditor.swift` | `UI/Settings/…` |
| `Defaults.swift` | `Core/Defaults.swift` (registered defaults, lines 1210–1565) |
| `NotchPresentationProbe.swift` | `Support/NotchPresentationProbe.swift` (dev-only self-test harness, `#if VORSSAINT_DEVELOPMENT`) |

Conventions:
- **pt** = macOS point. On Windows use **1 pt = 1 DIP = 1 CSS px** inside WebView2 (device-independent, already multiplied by the monitor DPI scale). All numbers below are in pt/DIP.
- "**Windows case**" = a display **without** a camera housing (`isNotched == false`), which is every Windows display. macOS-only notch branches (`isNotched == true`) are noted only where they change shared logic.
- y grows **downward** from the top edge of the screen unless stated (the Swift code uses a flipped canvas, `NotchWindowHost.swift:1051`).
- Spring notation: SwiftUI `Spring(duration d, bounce b)` ⇒ ω₀ = 2π/d, ζ = 1 − b. Framer Motion: `stiffness = ω₀²`, `damping = 2ζω₀`, `mass = 1`. Legacy `spring(response r, dampingFraction z)` ⇒ `stiffness = (2π/r)²`, `damping = 4πz/r`.

---

## 1. Geometry

### 1.1 Inputs on a display without a notch

`NotchGeometry.init` (`NotchSupport.swift:2277-2326`) is driven by:

| Input | macOS source | Windows proposal |
|---|---|---|
| `screen` | `NSScreen.frame` (full monitor rect, not the work area) | `MONITORINFO.rcMonitor` converted to DIPs |
| `safeAreaTop`, `cameraWidth` | 0 / 0 on a display without notch → `isNotched = false` (`:2287`) | always 0 |
| `menuBarHeight` | measured bar height, clamped 16…64, fallback `NSStatusBar.system.thickness` else **24** (`NotchSupport.swift:2202-2216`, `:2286`) | no menu bar exists: use a **virtual bar height of 24** (see Open questions) |
| `barEdge` | `1 / backingScaleFactor` (`NotchService.swift:3524`), clamped 0…1 (`NotchSupport.swift:2303`) | `1 / devicePixelRatio` of that monitor (1.0 @100 %, 0.8 @125 %, 0.667 @150 %, 0.5 @200 %) |
| `silhouette` | pref `notchSilhouette`, default **`capsule`** (`Defaults.swift:1516`, `NotchSupport.swift:122-124`) | same |
| `layout` | pref `notchSize`, default **`spacious`** (`Defaults.swift:1440`) | same |
| `capsuleFit` | width −40…40 (even), height −4…4, drop 0…20 (`NotchSupport.swift:170-193`) | same |
| `cameraFit`, `hideMenuBarGap` | ignored when not notched (`:2289`, `:2319`) | ignore |
| `outline` | `outlineRoom = 2` only if notched (`:2311`) → 0 on Windows | 0 |
| `compactSideRoom` | free menu-bar room either side of the cutout (see 1.1.1) | computed as "empty bar" (see 1.1.1) |

#### 1.1.1 `compactSideRoom` (free room beside the cutout)

`NotchMenuBarLayout.sideRoom` (`NotchSupport.swift:3164-3178`): left = `screen.minX + 8`, right = `screen.maxX − 8`, camera rect centred; room = `min(camera.minX − left, right − camera.maxX)`; any occupied rect overlapping the camera ⇒ `nil`.
With `notchCoversMenus = true` (default, `Defaults.swift:1526`) or on a display without a menu bar, the service applies the **empty-bar** value immediately (`NotchService.swift:3311-3317`): room = `(screen.width − cameraWidth)/2 − 8`. For 1920 DIP: (1920 − 135)/2 − 8 = **884.5**.
**Critical:** a non-notched island is only shown at rest if `compactSideRoom != nil` (`NotchService.swift:2703`). On Windows always set it (no AX menu measurement exists).
AX menu measurement (`NotchMenuBarSpace.swift`, 1 s timer `NotchService.swift:3334`) is macOS-only; drop it.

### 1.2 Derived constants (Windows case)

Formulas `NotchSupport.swift:2293-2324`, `:398-401`. Bar height `B = 24`, edge `e = 1/dpr`.

| Quantity | Capsule (default) | Simulated notch ("notch" silhouette) |
|---|---|---|
| `floatingGap` g | `min(2, max(0, floor((B−20)/2)))` = **2** | `nil` |
| `fullStrip` | `max(B + fit.h, 2g+12)` = 24 | — |
| `stripHeight` = `cameraHeight` | `max(B − e + fit.h, 2g+12)` = **24 − e** (23 @100 %, 23.2 @125 %, 23.33 @150 %, 23.5 @200 %) | **B = 24** |
| `profileHeight` | `fullStrip − (g − 2)·2` = 24 | 24 |
| simulated `cameraWidth` | `round(180·24/32 + fit.w)` = **135** | `180·24/32` = **135** |
| cap | `cameraWidth ≤ 0.7·screen.width` | same |
| `menuBarHeight` (geometry) | `max(stripHeight, B)` = 24 | 24 |
| `stripBodyHeight` | `stripHeight − 2g` = 19 … 19.5 | 24 |
| `safeContentTop` (`:2338`) | `bottomInset` = **16** | `cameraHeight + 10` = **34** |
| `floatingDrop` | `capsuleFit.drop` (0…20) | 0 |

### 1.3 Shapes

Two silhouettes; both are recomputed for every animation frame from the current (width, height) only. **Never animate CSS border-radius; derive the path.**

Per-height helpers (`NotchSupport.swift:382-383`, `:404`):
- `surfaceRadius(h) = min(28, 0.34·h)`
- `shoulder(h) = min(14, 0.19·h)`
- `capsuleRadius(h) = min(max(0,h)/2, 28)`
- Bézier circle constant `k = 0.55228475`.

#### 1.3.1 Hanging notch shape ("ears") — `NotchShape.path` (`NotchView.swift:792-818`)

Rect `W × H`, origin top-left. `s = shoulder(H)`, `b = min(surfaceRadius(H), H/2, (W − 2s)/2)`.

```
M 0,0
L W,0                                   top edge spans the full width (incl. ears)
C (W − s·k, 0) (W − s, s·(1−k)) (W − s, s)      right ear: CONCAVE quarter arc, centre (W, s), radius s
L W − s, H − b
C (W − s, H − b·(1−k)) (W − s − b·(1−k), H) (W − s − b, H)   bottom-right convex corner, radius b
L s + b, H
C (s + b·(1−k), H) (s, H − b·(1−k)) (s, H − b)               bottom-left convex corner
L s, s
C (s, s·(1−k)) (s·k, 0) (0, 0)                               left ear: concave quarter arc
Z
```
The visible body is `W − 2s` wide; the ears flare outward at the top edge so the island "hangs" from the screen edge. Examples:

| State | W × H | s | b |
|---|---|---|---|
| bare rest | 135 × 24 | 4.56 | 8.16 |
| resting with wings | 223 × 24 | 4.56 | 8.16 |
| expanded list (spacious) | 560 × 328 | 14 | 28 |
| peek | 340 × 86 | 14 | 28 |

#### 1.3.2 Floating capsule — `NotchLayout.capsulePath` (`NotchSupport.swift:413-452`)

Surface rect `W × H` (the "surface" includes the transparent margins). Body:
- `side = min(shoulder(H), W/2)`, `inset = min(g, H/2)`
- body = `x: side, y: inset, w: W − 2·side, h: H − 2·inset`
- `corner = min(capsuleRadius(body.h), body.w/2)`; same 8-element rounded-rect path with `handle = corner·(1 − k)`.

So the capsule floats **g = 2 pt below the top edge** and **2 pt above the strip bottom**, and sits `side` pt in from each side of the surface. Closed (body ≈ 19.5 high) the ends are full half-circles (corner 9.75); open, corners cap at 28 (`:403-404`). Every size, down to a line, keeps the same 8 path elements so frames interpolate (`:428-430`).

`capsuleSurface(ofBody:)` (`:423-426`) inverts this (used to start a resize from what is drawn).

#### 1.3.3 Hit-test geometry of a shape (`NotchWindowHost.swift:1298-1307`)

`contains(point)` = path contains point **or**, for a capsule only, the point lies in the strip *above* the capsule body (`0 ≤ y ≤ body.minY`, within body x-range): the 2 pt of "menu bar" above a capsule belongs to the island so a pointer slammed against the screen's top edge hits it. For the hanging shape, the top edge row (y = 0) belongs to the island (flipped-rect rule, `NotchSupport.swift:2828-2833`, `:1663-1667`).

### 1.4 Size catalogue

`surfaceSize` precedence (`NotchService.swift:936-967`; capsule overrides `:989-998`). Values for the Windows case, 1920×1080 DIP, spacious unless noted. "C" = capsule (e = 0.5 for the worked numbers; at 100 % use 23/104), "N" = simulated notch.

| # | State | Size rule | C | N |
|---|---|---|---|---|
| 1 | Hidden in fullscreen (non-notched) | window hidden entirely (`NotchService.swift:438-440`, `:2703`) | — | — |
| 2 | Capture controls collapsed | N: `(cameraWidth + 56, stripHeight)` (`:941`); C: `NotchCapsuleLayout.captureSurface` (`NotchSupport.swift:1198`) | 106 × 23.5 | 191 × 24 |
| 3 | Capture controls open | `NotchCaptureControlsLayout.size` (`NotchSupport.swift:1232+`) | other spec | |
| 4 | Expanded | `expandedSize` (1.4.2) or Command Bar `commandBarSurfaceSize` | | |
| 5 | Drag placeholder (file dragged toward island) | `(peek.width, safeContentTop + 66)` (`NotchService.swift:946`) | 340 × 82 | 340 × 100 |
| 6 | Notice (compact) | N: `cameraWidth + leading + trailing` × stripHeight (`NotchSupport.swift:2636-2639`); C: `capsuleNoticeSize` (`NotchService.swift:1004-1023`) | see 1.4.3 | |
| 7 | Notice expanded (held banner → preview card) | `notificationPreviewSize` (`NotchSupport.swift:2648-2655`) | 400 × (16 + content + 16) | 400 × (34 + content + 16) |
| 8 | Peek (hover preview) | `(min(W−24, max(cam+110, 340)), safeContentTop + 52)` (`:2656-2658`) | 340 × 68 | 340 × 86 |
| 9 | Activity picker (≥2 live activities, hovered) | `NotchActivityPickerLayout` (`NotchSupport.swift:1203-1227`) | formula 1.4.4 | |
| 10 | Compact live activity strip | N: `compactActivityGeometry.compactActivitySize`; C: `capsuleStripSize` | 1.4.1 | |
| 11 | Resting (no activity) | `restingSize(showsContent:)` (`NotchSupport.swift:2419-2428`) | 106 × 23.5 | bare 135 × 24, with idle content 223 × 24 |
| + | Hover pulse on 10/11 | `NotchHoverEmphasis.size` (`NotchSupport.swift:735-742`) | +20 wide, +0 tall | +20 wide, +5 tall |
| — | Hidden-until-hover | shrink to `(collapsed.width, 0)` then window hidden (`NotchWindowHost.swift:154-161`) | | |

#### 1.4.1 Closed sizes in detail

- **Bare capsule** (`NotchSupport.swift:2424-2427`): `min(cameraWidth, round(max(5·body + fit.w, 2·body) + 2·shoulder(strip)))`. e = 1 → 104 × 23; e = 0.8 → 105 × 23.2; e = 0.667 → 106 × 23.33; e = 0.5 → 106 × 23.5. `capsuleRestingAspect = 5` (`:408`).
- **Resting wings** (simulated notch only): `restingWingWidth = 44` if `compactSideRoom ≥ 44` else 0 (`:2412-2415`); `collapsed = (min(W − 24, cam + 2·wing), stripHeight)` (`:2416-2418`). Capsule at rest always shows its idle content *inside* the bare capsule (`NotchService.swift:994-996`).
- **Hover pulse** (`NotchSupport.swift:735-742`): `occupiedWing = max(0,(rest.w − cam)/2) + reach`; `freeSide = max(0, compactSideRoom − occupiedWing)`; growth = `min(10, freeSide)`; width += 2·growth; height += 5 for hanging, +0 for capsule.
- **Compact activity, simulated notch**: always one row (no footer when not notched, `:2578-2580`). Width = `cameraWidth + 2·wing`, height `stripHeight`, where the wing comes from the activity's geometry function (`:2439-2568`):
  - music: wing = 44 required, uses `min(56, room)` → strip = `min(max(520 spacious | 440 compact, cam+88), W−24)` limited to `cam + 2·room` → **247 × 24** (`:2569-2575`, `:2439-2447`).
  - timer / keep-awake: wing clamped 44…64 (80 if a download is paired); wings only if `compactSideRoom ≥ 64` (`:2500-2513`, `:2506`, `NotchTimerSupport.swift:335`).
  - downloads: wing default 56, needs room ≥ 44 (`:2516-2522`).
  - agents / sessions: 44…80 (`NotchAgentSupport.swift:216`); watch: 56…150 (`NotchWatchSupport.swift:157`); calendar: 72…120 (paired: 44…120) (`NotchSupport.swift:2523-2543`).
  - Wings narrower than `minimumWing` (44 default) are dropped, not drawn cramped (`:2275`, `:2574`).
- **Compact activity, capsule** (`NotchCapsuleLayout.surface`, `NotchSupport.swift:1052-1059`): `width = clamp(ceil(content + leading + trailing + 2·side), bareRest, min(floor(maximum + 2·side), availableWidth))`, height `stripHeight`; `availableWidth = min(W − 24, cam + 2·room)` (`:1044-1047`). Maxima (`:981-988`): notification 380, notice 340, music 260, calendar 300, download 280, activity 220. Paddings: endPadding 8, spacing 6, groupSpacing 10, symbol slot 16 (12 pt glyph), meter 56×4, markSpacing 4, calendar dot 6, download meter 36, spinner 16, `air` 2 (`:953-978`).
  - Capsule music: cover `artworkSide = body − 6`, inset `(body − side)/2 = 3`, bars width `1.8·(7 + 6·0.85) = 21.78` (`NotchSupport.swift:373-377`, `:1036-1037`, `:1104-1108`). Without title → clamps up to bare 106. The title is shown for **4 s** after each new song (`NotchService.swift:405`, `:2892-2907`) and while the pointer rests on the cover ≥ **0.3 s** (`NotchSupport.swift:765`, `NotchService.swift:2994-3014`).

#### 1.4.2 Expanded size

Width (`NotchSupport.swift:2660-2679`):
- `limit = W − 24 − 2·72` (72 = quick-access gutter, `:1541`).
- Enlarged page (Sessions detail): `min(max(min(1180, 0.66·W), cam + 36), limit)`.
- Otherwise `chosen = preferredWidth(layout)` = compact **480**, spacious **560**, custom `notchCustomWidth` (360…600, default 440) (`:457-463`, `:130-133`).
- Under a preset, a page with a natural width takes `min(chosen, natural + 56)` (Windows: the `isNotched` camera term is 0). Natural widths (`:555-567`): music **344**, timer **344**, controls `max(railWidth, musicCardMin=224, 344)` (rail = n·84 − 8 when it fits one row). ⇒ **Music, Timer, Controls (default) are 400 wide; every other page is 560 (spacious)/480 (compact).** Custom size: always `chosen` on every page.
- Final: `min(max(preferred, cam + 36), limit)`.
- Gallery (sections), details (metric, app panel) and the empty island use `chosen` (`NotchService.swift:897-902`).

Height (`NotchSupport.swift:2726-2793`) = `headerTopInset + headerChromeHeight + contentHeight`, custom ⇒ `min(…, customHeight)`, always `≤ screen.height − 48 − quickAccessBottomInset` (bottom inset = 72 when any bottom bubble exists — the default layout has one, `:1427`, `NotchService.swift:3470`).

Header metrics (Windows case, `NotchSupport.swift:2343-2392`):
- `headerCameraGap = 0` (requires `isNotched`).
- `headerRowHeight` = 36 if `pageTop == .row` or the row must hold the sections button, else 0 (`:2377-2380`). `pageTop`: music `.wrapsCamera`, system `.none`, all others `.row` (`:212-219`); gallery/details/app panel force `.row`, empty island `.none` (`NotchService.swift:907-910`).
- `headerTopInset` (`:2369-2373`): row height 0 ⇒ **16**; capsule ⇒ `16 − (36−28)/2` = **12**; simulated notch ⇒ **0**.
- `pageTopInset = headerTopInset + 36 + 12` (row present) else `headerTopInset`.
- `headerChromeHeight = pageTopInset − headerTopInset + 16` ⇒ **64** with row, **16** without.
- `contentBudget` = compact **180**, spacious **264**, custom `customHeight − headerTopInset − headerChromeHeight` (`:2682-2688`); `pageBudget = max(budget, 320)` for presets (`:2691`).
- Horizontal content inset 28 each side; `contentWidth = expandedWidth − 56` (`:2680`).

Worked heights (C / N):

| Page | Content height rule | Island (spacious) |
|---|---|---|
| Lists (mixer, calendar, clipboard, captures, files, notifications, downloads, camera, scratchpad, sessions, watch) | = budget | 560 × 340 / 560 × 328 (compact preset: 480 × 256 / 480 × 244) |
| Controls (home), default controls, both levels fit in row | `NotchLayout.controls()` (`:608-633`) → 80 card + 10 + 74 rail = 164 | 400 × 240 / 400 × 228 |
| Music (wraps camera, no row) | player = 56 + 14 + (14+10 if playing) + 44 + 8 = 146 (122 idle) (`:660-668`); + 216 when lyrics/queue open (`:2694`) | 400 × 178 (both) |
| Timer, idle timer mode | ruler 82 (`:641-655`) | 400 × 158 / 400 × 146 |
| Timer, Pomodoro | 82 + 8 + 28 | 400 × 194 / 400 × 182 |
| System (no row) | rows·72 + gaps 10 + 2·hoverInset (`:2770-2774`) | 32 + content |
| Tools | rail rows of 72, spacing 6 (`:2775-2778`) | |
| Agents | measured cards, empty 140 (`:2781-2783`) | |
| Detail (metric/app panel) | `pageBudget` (320) or measured + 4 | |
| Sections gallery | `min(320, rows·86 + (rows−1)·8)` (`:2810-2815`) | 3 rows: 560 × 350 / 560 × 338 |
| Enlarged (sessions detail) | `min(0.8·H, H − 48 − 72)` | 1180 × 864 @1920×1080 |
| Command Bar | `(max(cam+80, 560+56), safeContentTop + barHeight(50 initially) + 16)` capped `(W−24, H−48)` (`NotchService.swift:980-984`, `CommandBarView.swift:32-34`) | 616 × 82 / 616 × 100 |

Minimum usable page heights are enforced *inside* the page via a vertical `ScrollView` (the silhouette does not grow) (`NotchView.swift:465-511`): controls layout height, timer layout, calendar `14 + 6·16`, clipboard 104, camera 144, mixer 104, music player height, files 96.

#### 1.4.3 Notices (closed)

`NotchNotice.preferredWings` (`NotchService.swift:38-59`), measured with SF 11 medium monospaced-digit:
- Level notices (volume, brightness, keyboard light, mic…): both wings = `ceil(13 + 18 + 8 + width(detail))` (`:48-51`, `levelInset = 13` `:69`).
- Text notices: leading = `clamp(ceil(textW + 18 + 8) + 16 + cameraGap, 36, 240)` (accessory max 160); an empty title gives leading = `fitted(18)` (`:56`); trailing = `clamp(ceil(detailW) + 16 + cameraGap, 36, max)` — **no** 18 + 8 mark room (`:58`). `cameraGap` = 16 battery, 6 for text notices, 0 levels/banners (`:78`). `minimumWing = 36` (`:65`).
- Notification banners: `NotchNotificationBannerLayout.wings` (`NotchNotificationSupport.swift:171-206`): icon 22, spacing 8, inset 16, air 6, wing range 36…190, title SF 11 semibold, message SF 11 regular; message wraps to 2 lines when `stripHeight ≥ 30` (never on Windows: 1 line). A banner replacing a banner keeps at least the previous wings (`NotchService.swift:2352-2354`) — capsule keeps at least previous width (`:1004-1010`).
- Each wing is capped to `(W − 24 − cam)/2` (`NotchSupport.swift:2622-2634`).
- The island centre shifts right by `surfaceShift = (trailing − leading)/2` (`NotchSupport.swift:2642-2645`, `NotchService.swift:972-976`) — hanging shape only; capsule never shifts.
- Capsule notices: `noticeContent` = 16 + 6 + titleW + (10 + detailW) or level row 16 + 6 + 56 + 6 + readingW(≥ width("100%")) (`NotchSupport.swift:1087-1098`).

#### 1.4.4 Activity picker

`NotchActivityPickerLayout` (`NotchSupport.swift:1203-1227`): `columns = min(3, count)`, rows = ceil(count/columns), labelWidth = widest title at SF 12 medium; `width = clamp(max(strip.w + 48, columns·(label+48) + (columns−1)·6 + 48), …, W − 24)`, `height = strip.h + rows·32 + (rows−1)·6 + 24 (+ 24 + 6 if combinations exist)`. Strip = widest/tallest strip among candidates so switching doesn't move the buttons (`NotchService.swift:545-560`).

### 1.5 Expanded inner layout

`NotchView.expanded` (`NotchView.swift:280-320`):
- Outer: padding 28 left/right, `headerTopInset` top, 16 bottom; frame = `expandedSize`.
- Page frame = `contentSize` (`NotchSupport.swift:2817-2820`), top offset `pageTopInset − headerTopInset`.
- Page clip `NotchPageClip` (`NotchView.swift:848-857`): the page may draw into the 28 pt margins, up to the island's top edge (behind the header) and into the bottom 16 pt; the silhouette clips the rest. The header row is drawn above the page.
- On a page with **no** top row, a level reading appears as a 170 × 28 black capsule in the top corner (trailing for music, else leading) (`NotchView.swift:299-306`).
- **Top row** (`NotchBandRow`, `NotchComponents.swift:446-480`): height 36; on Windows `besideCamera = false` ⇒ one row: leading controls left-aligned (fill), `Spacer(minLength: 12)`, trailing controls right-aligned with layout priority 1. Items spaced 6.
  - Leading (`NotchView.swift:420-433`): gallery open → `chevron.left` back + search; detail → back button "‹ Title"; else sections button (`square.grid.2x2`) **only if no floating Explore bubble exists** (`NotchService.swift:3473`). Default layout has the Explore bubble, so the row has no sections button.
  - Trailing (`NotchView.swift:374-382`): page items, then update control, then pin mark; `ViewThatFits` drops update label → update → pin in that order.
  - Level readings from volume/brightness keys temporarily **cover** one side of the row (`NotchView.swift:349-396`, `:513-528`), width `min(side, 220)` (home page: 160…220 to the trailing end, levels ease aside with `.smooth(0.25)`).
- Pages that scroll vertically get a 16 pt edge fade (`NotchScrollEdgeFade.swift`, ease-out 0.18 s when content passes an edge).

### 1.6 Window position

- Window frame for surface size S (`NotchSupport.swift:2822-2826`): `x = screen.midX + surfaceShift − S.w/2`, top = `screen.top + floatingDrop`. The island hangs from the **monitor's top edge** (not the work area).
- During a transition the OS window is enlarged to an **envelope** (union of start, target and overshoot, `NotchSupport.swift:2995-3002`, `NotchWindowHost.swift:277-285`) plus gutters (72 per side when quick-access bubbles show, +bottom inset), then shrunk at settle. Margins are whole points, frames are snapped to device pixels (`NotchWindowHost.swift:393-411`, `NotchSupport.swift:2990-2993`).
- Content is hosted on a fixed **stage** as large as the largest display, centred on the display's centre, so resizing the window never moves content (`NotchWindowHost.swift:756-774`, `:1746-1784`).

### 1.7 Quick-access bubbles (floating buttons beside the open island)

Shown only while `expanded`, not in Command Bar or capture controls, and only if ≥1 button configured (`NotchService.swift:2738-2739`). Config `NotchQuickAccessConfiguration` (`NotchSupport.swift:1418-1522`), JSON in `notchQuickAccessLayout`, max **3 per side**, labels ≤ 40 chars. Default (`:1422-1428`): **left** Explore (sections), Timer; **right** Settings, Mixer; **bottom** Music. A button whose section/feature is off is dropped from the shown list (`current()`, `:1484-1494`): the visible bubbles close up with no gap; only the *stored* order keeps its place for when it returns. The same filtered list decides the bottom inset and whether the top row needs a sections button (`NotchService.swift:3469-3473`).

Layout (`NotchSupport.swift:1538-1598`): diameter **44**, gap **12**, gutter **72**, row spacing **54**, hover margin **16**, hover-exit delay **0.18 s**, withdrawal **0.16 s**.
- Body = island minus shoulders: `x = islandX + shoulder(h)`, `w = W − 2·shoulder`, `y = 0`, `h = H` (`NotchWindowHost.swift:486-490`).
- Centre at progress p (0…1.1): `offset = −14 + p·(14 + 12 + 22)` → **34 pt outside the body edge at p = 1** (circle edge 12 pt from the island side). Left: `x = body.minX − offset`; right: `body.maxX + offset`; bottom: `y = body.maxY + offset`, buttons centred on `body.midX` spaced 54.
- Side column top: `sideTop = max(body.minY + 22 + 12, min(quickAccessCenterY, body.midY − span/2))`, `span = (maxCount−1)·54`; `quickAccessCenterY = max(headerTopInset + headerRowHeight/2, menuBarHeight + 6 + 22)` = **52** on Windows (`NotchSupport.swift:2389-2392`). Both side columns start level.
- Hover corridor per side = bounding box from island edge to circles, inset by −16 (`:1557-1571`); clicks use exact circles (`:1591-1597`).
- Visual (`NotchQuickAccessView.swift:110-161`): black circle, 0.75 pt stroke white α0.16 (selected or increased contrast α0.6), SF symbol 17 medium white; scale `0.4 + 0.6p`, opacity p. A black "drop" blob with a dissolving neck connects each circle to the island while it slides out (`NotchQuickAccessDrop`, `:224-260`): circle radius `22·(0.4+0.6p)`, neck half-height `r·max(0, 1 − p/0.84)`, neck anchored 2 pt inside the island edge, control points as in source.
- Selected states: Explore when gallery open (icon turns `arrow.uturn.backward`), Pin when pinned (`pin.fill`), module bubble when that page shows, keep-awake/mic/recording toggles (`:111-137`).

### 1.8 Colors, outline, fonts

- Island fill: **opaque black #000** (canvas layer `NotchWindowHost.swift:1069`, backdrop `NotchComponents.swift:564`). Foreground white; color scheme forced dark (`NotchView.swift:59`).
- A 0.01-alpha black fill under the whole shape keeps glass areas clickable (`NotchView.swift:45`).
- Outline (pref `notchOutlineEnabled`, default off) (`NotchWindowHost.swift:1116-1136`, `NotchLayout.outlineWidth = 2`): stroke 2 pt along the silhouette, only the **inner 1 pt** visible (clipped by the mask); colour white α0.65 (α0.85 with Increase Contrast); **systemOrange** while a timer strip is the visible compact activity (`NotchService.swift:2722-2723`) — with Increase Contrast always white (`NotchWindowHost.swift:1118`). Hanging shape: top 1 pt left open (`outlineTopGap = 1`, `:1088`, `:1155`); capsule: all round. Outline off: hairline 0.5 pt, α0 (α0.45 with Increase Contrast), and only drawn when island height > 64.
- Glass (macOS 26 Liquid Glass) and translucent (`NSVisualEffectView .hudWindow`) backgrounds are opt-in, default **off** (`Defaults.swift:1213`, `:1448`). Translucent tint: fully black over the strip height, then smooth-step ramp over 36 pt to `1 − 0.62·openness` (0.3 with Increase Contrast) (`NotchSupport.swift:3185-3206`); glass shade gradient `α = 1 − openness·0.45·t^2.5` (0.10 with Increase Contrast) (`NotchComponents.swift:593-600`); `NotchGlassFade` opens the glass over the last 48 pt of growth (`NotchSupport.swift:3120-3149`).
- Common control tones: hover wash white α0.09; disabled α0.4; pressed α0.8 (`NotchComponents.swift:10-35`); icon button 28×28, radius 9, glyph SF 12 medium α0.55 (selected white + bg α0.12) (`:144-168`); pill button height 28, bg α0.14 when prominent (`:232-250`); empty-state glyph 56×56 radius 18 bg α0.05, SF 26 light α0.65 (`:183-222`); meter track white α0.14, fill α0.9 (`:59-77`); slider track α0.14, fill α0.92 (`NotchLevelSlider.swift:175-204`); tile bg α0.045 / highlighted α0.12, stroke α0.04 / α0.55 (`NotchSectionsView.swift:123-126`).
- Fonts (SF Pro; tabular digits where noted): 9 medium (resting battery %), 10 medium rounded (tile shortcut), 11 medium (notice text, tile title), 11 semibold (banner/music title), 12 medium (capsule detail, menus), 12 semibold (capsule title), 12 medium mono-digits (level reading), 13 semibold mono-digits (capsule readings), 13 semibold (back button), 14 medium (peek grid icon), 16 semibold (peek title), 17 medium (bubble icons), 22 medium (tile icons), 26 light (empty glyph). Icons are SF Symbols (names quoted throughout the source).

---

## 2. State machine

### 2.1 State variables (`NotchService.swift:111-400`)

`expanded`, `peeking`, `pinned`, `notice` / `noticeExpanded` / `departingNotice`, `dragPlaceholder`, `captureControls` (+`Collapsed`), `showingSections` (+`sectionQuery`, `highlightedSection`, `sectionRow`), `showingAppPanel`, `selectedMetric`, `showingCommandBar`, `sessionsEnlarged`, `selected: NotchModule`, `hiddenInFullscreen`, `openedByHover`, `clickedSinceOpening`, `inside` (pointer over island), `hoverEmphasized`, `hoverState.suppressed`, `activitySelection`, `detailHasPage`, `pageLayers[module]`, `heldDrag`, plus session state (`locked`, `sleeping`, `displaysSleeping`, `onConsole`, `screenSaverRunning`; `canPresent = !locked && !sleeping && onConsole && !displaysSleeping`, `NotchSupport.swift:2836-2851`).

`keepsWorkingSurface` (`NotchService.swift:230-244`) = pinned, a menu tracking, a modal window/sheet, or a page-level modal (lyrics import, download folder chooser, scratchpad modal, calendar permission prompt, file interaction, capture preview, launcher editing/utility, camera permission). While true: no hover-close, no click-outside close, no activation close.

### 2.2 Presentation precedence (what is drawn)

`NotchView.surface` (`NotchView.swift:81-221`), first match wins:
1. `fullscreenCompact` → nothing (clear).
2. capture controls (collapsed strip `tool | camera gap | chevron.down`, or open controls).
3. `expanded` → Command Bar page, else expanded page (gallery / detail / module page / empty).
4. `dragPlaceholder` → drop hint (tray icon + "Drop…" 13 medium, dashed 1 pt white α0.24 rounded-16 border, min height 52, h-padding 18, top padding `safeContentTop`).
5. `notice` (or `departingNotice` unless peeking) → notification preview card (expanded) or compact notice (button: click = `activateNotice`).
6. `peeking` → row: navigation button (grid icon 14 medium α0.7 + destination title 16 semibold, height 36, radius 12) · spacer · `chevron.down` icon button "Open"; padding 28 h, top `safeContentTop`.
7. compact activity → activity picker (when hovered and ≥2 activities) or the activity's strip.
8. departing / lingering music strip (non-interactive) while the song leaves.
9. resting: capsule resting view, or hanging resting strip (idle wings).

Hidden-until-hover: when `hiddenUntilHover` is true the window animates to height 0 and is ordered out (`NotchService.swift:2691-2701`).

### 2.3 Opening modes (Settings › Behavior › Opening, `NotchSettings.swift:643-657`)

| Mode | `notchOpenOnHover` | `notchHoverExpands` | `notchHideUntilHover` | Behavior |
|---|---|---|---|---|
| Click to open (**default**) | false | (true) | false | hover only pulses (+10 pt per side); click opens |
| Hover preview | true | false | false | hover ≥ delay → **peek**; click/chevron opens |
| Hover expand | true | true | false | hover ≥ delay → open full page without focus (`open(takeFocus:false)`) |
| Hidden until hover | true | true | true | closed island invisible; pointer reaching its rect (collapsed size) opens it; optional `notchHiddenShowsActivity` shows it while an activity runs or a notice shows |

Note: migrated pre-existing users get `notchOpenOnHover = true` (`Defaults.swift:2093-2112`).

### 2.4 Transitions

`mutatePresentation` applies state changes with SwiftUI animations disabled and then calls `refreshPresentation(transitionContent:)` (`NotchService.swift:2597-2602`); the silhouette and content transitions are driven by the native host (Section 3).

| Trigger | Guard / notes | Result | Content transition |
|---|---|---|---|
| Click on activation area of closed island (`NotchService.swift:2771-2785`) | capture controls → expand them; capsule over music bars → play/pause; calendar activity → open Calendar at event | `toggle()` | reveal |
| Global hotkey (`toggleFromShortcut`, `:1221-1225`) | `acceptsUserInteraction` | toggle (or expand capture controls) | reveal / dismiss |
| Hover enter, closed (`hover`, `:1564-1663`) | not pinned / held drag / working surface; not suppressed; no notice (unless hidden-until-hover); `notchOpenOnHover`; picker not showing; for a live activity *with wings* only if hover-expands | after **`notchHoverDelay`** (default **250 ms**, 100…1000, step 50): hover-expand → `open(takeFocus:false)`; preview → `peeking = true` + haptic | reveal |
| Hover enter (any closed state, not opening) | `!opensOnHover`, no Reduce Motion | `hoverEmphasized` pulse | none (size spring) |
| Hover exit | `closesOnPointerExit = peeking || (expanded && openedByHover)` (`NotchSupport.swift:2009-2011`) or a held banner | after **`notchCloseDelay`** (default **180 ms**, 100…2000) when expanded / preview-expanded, **120 ms** in every other case (peek, or a held banner that is not expanded, which then gets its full duration again): collapse / release | dismiss |
| `open(module?)` (`:1411-1475`) | island enabled, not suspended | `expanded = true`, sets destination; installs click/key monitors; key focus if `takeFocus` | reveal if was closed; **replace** if already open and page changes |
| `collapse()` (`:1477-1506`) | not during capture controls / held drag | everything closed, `pinned = false`, hover suppressed if pointer inside | dismiss |
| Click outside (global + local mouse-down monitors, `:3721-3806`) | not over island, child windows, menu-bar status item, assistive keyboard; not `keepsWorkingSurface` | collapse | dismiss |
| Other app activated (`:3652-3665`) | `closesOnActivation = !openedByHover || clicked || !pointerInside` (`NotchSupport.swift:2018-2020`) | collapse | dismiss |
| Notice `show()` (`:2347-2379`) | `showsSystemFeedback`, event routed, `shouldReplace` | `notice = incoming`, auto-dismiss after event duration | reveal if none; **replace** if different event or preview open; **none + steady width** if same event (value bursts) |
| Notice timeout / `dismissNotice` (`:2503-2522`) | | `notice = nil` | **depart** (leaving content fades 160 ms, swapped at 200 ms) or dismiss if preview was open |
| Banner hovered (`holdNotification`, `:1716-1733`) | `notchOpenOnHover` | timer cancelled; after hover delay → `noticeExpanded` preview | reveal |
| Banner left (`releaseNotification`, `:1737-1741`) | | preview → dismiss; held → full duration again | |
| Click on notice (`activateNotice`, `:2381-2401`) | | opens the related page (download→Downloads, timer→Timer, accessory→System, notification→open source app, clipboard, agents, track→Music, mic→Mixer, watch, sessions; default Controls) | reveal |
| Select module while open | | `selected` changes | replace (0.18 s crossfade) |
| Back (`goBack`, `:2004-2009`) | detail/app panel | back to page | replace |
| Escape (`stepBack`, `:2014-2023`) | | detail → back if reached from a page, else collapse; page layer (e.g. month grid, mixer options) → close layer; else collapse | replace / dismiss |
| ⌘K / Explore bubble / grid button (`toggleSections`, `:1773-1785`) | | gallery on/off (`open(…, sections:)`) | replace |
| File drag starts (external, shelf on, drag-reveal on) (`:2036-2051`) | not expanded | `dragPlaceholder` | — (size spring) |
| File dragged over island | | `open(.files, takeFocus:false)` | reveal |
| Internal shelf drag leaves the island | after close delay | collapse (`:2057-2077`) | dismiss |
| Track change (`scheduleTrackNotice`, `:2406-2427`) | | **500 ms** debounce, then "track" notice; the strip keeps the old song meanwhile | |
| Timer mode change on Timer page | | replace (`:1550-1554`) | replace |
| Swipe/scroll gesture | see 2.7 | open / close / next/prev track | |
| Session needs permission (auto-expand pref) (`:4179-4211`) | | `open(.sessions, takeFocus:false)` | reveal |

`hoverState` (`NotchSupport.swift:722-730`): closing with the pointer inside sets `suppressed`; hover cannot open again until the pointer has left once. Resizes can emit spurious enter/exit; overlapping reports keep the first deadline (`NotchService.swift:1609`). Hover containment uses the **destination** rect, not the animating mask (`NotchWindowHost.swift:504-512`).

### 2.5 Activation area (the toggle button) (`NotchSupport.swift:2397-2410`, `NotchService.swift:2754-2785`)

- Closed, capsule: entire surface.
- Closed, simulated notch: entire surface; with a live activity only the central `cameraWidth` (135) column (wings host their own controls); offset by `−surfaceShift`.
- Peek: full width, a top band of height `min(safeContentTop, H)` (16 capsule, 34 simulated notch).
- Activity picker showing: only the strip area at the top of the picker (`NotchService.swift:2759-2763`).
- Expanded on Windows: **empty** (camera gap is 0) ⇒ the open island has no click-to-close area; close via click outside, Esc, gesture, context menu, hotkey, quick-access, hover exit (hover-opened).
- Notice / drag placeholder: none (the notice itself is a button).
- Capture controls collapsed: whole surface.

### 2.6 Keyboard (local monitor, island window key) (`NotchService.swift:3732-3796`)

| Keys (mac) | Windows proposal | Action |
|---|---|---|
| ⌘K | Ctrl+K | toggle sections gallery |
| ⌘, | Ctrl+, | open island settings (collapses first) |
| ⌥⌘ + `shortcutKey` | Ctrl+Alt + key | jump to section (`NotchSupport.swift:39-59`): controls c, mixer v, music m, clipboard b, captures s, files f, system i, tools t, calendar a, notifications n, timer r, camera w, downloads d, scratchpad p, agents g, sessions l, watch o |
| ⌃Tab / ⌃⇧Tab | Ctrl+Tab / Ctrl+Shift+Tab | next / previous section (wraps, `NotchSupport.swift:1700-1704`) |
| Esc | Esc | gallery open → close gallery; else `stepBack`. Ignored while an IME composes, in the Command Bar, mixer % field, scratchpad find bar |
| Gallery: Tab/⇧Tab, arrows, Return | same | move highlight (grid flow by `sectionColumns`; with a query ←/→ edit text, ↑/↓ step results), open highlighted |
| Clipboard page: ⌘1…⌘9 | Ctrl+1…9 | paste nth visible entry |
| Sessions page: ⌘Y / ⌘N | Ctrl+Y / Ctrl+N | allow / deny permission |
| Scratchpad: ⌘T, ⌘W, ⌘F, ⌘G, ⇧⌘G | Ctrl+… | pad tabs / find |
| Tools page | arrows | launcher grid navigation |
| Global hotkey, default ⌃⌥⌘I, disabled by default (`GlobalShortcut.swift:224-225`, `Defaults.swift:1521-1522`) | `RegisterHotKey`, e.g. Ctrl+Alt+Win+I (see Open questions) | toggle island |

All island shortcuts above are skipped while capture controls are up (`NotchService.swift:3745`). Focus ring hidden until the first key press after opening (`showsKeyboardFocus`, `NotchService.swift:3736-3738`).

### 2.7 Gestures (`NotchGestureSupport.swift`, `NotchService.swift:3852-3896`), pref `notchGesturesEnabled` (default on)

- Movement per event: `delta · (directionInverted ? 1 : −1) · (precise ? 1 : 24)` (`:47-50`).
- Axis lock after |pending| ≥ 4 and dominant by ×1.5; horizontal only for precise (trackpad) input over a music surface, not over controls/scroll views (`:120-128`).
- **Vertical ≥ 16**: positive & closed → open; negative & open → close (`:23`, `:150-156`). Vertical allowed: closed always; expanded only over the header row (or the music surface outside scroll views) (`:52-55`, `NotchSupport.swift:1821-1824`).
- **Horizontal ≥ 40** over music: next (negative) / previous track (`:133-136`).
- One action per scroll sequence; a phaseless sequence expires after a 0.35 s gap; momentum may finish an open/close flick only (`:74-87`).
- Gallery wheel paging (`NotchSectionScroll`, `NotchSectionPaging.swift:37-79`): first step 24, then a row pitch of 94 (86 + 8); a discrete wheel notch = 1 row; momentum ignored.

### 2.8 Context menu (secondary click on the open island, `NotchView.swift:437-461`)

Sections (or Back), Pin/Unpin, Settings, Update (if available), — divider —, Collapse.

### 2.9 Timer / delay summary

| Constant | Value | Ref |
|---|---|---|
| Hover open delay | 250 ms (100–1000, step 50) | `NotchSupport.swift:1660-1671` |
| Hover close delay (expanded / preview) | 180 ms (100–2000) | `:1674-1684`, `NotchService.swift:1661` |
| Hover close delay (peek) | 120 ms | `NotchService.swift:1661` |
| Notice durations | volume/brightness/keyboard light/mic/input source 1.6 s; system notification & track 3 s; timer/download/watch/sessions 6 s; agents 5 s; battery/accessory 4 s; clipboard 2.5 s; capture 12 s | `NotchSupport.swift:1633-1643` |
| Track notice debounce | 500 ms | `NotchService.swift:2426` |
| Capsule song title shown | 4 s | `NotchService.swift:405` |
| Music strip naming (pointer on cover) | 300 ms | `NotchSupport.swift:765` |
| Music play/pause optimistic state | 1.5 s | `NotchService.swift:3062` |
| Departure hidden → swap | 200 ms | `NotchSupport.swift:2863` |
| Pointer-follow to another display | 200 ms | `NotchService.swift:413` |
| Screen-parameter change debounce | 100 ms | `NotchService.swift:3360` |
| Capture controls hover-open / auto-collapse / collapse after leave | 250 ms / 3 s / 180 ms | `NotchService.swift:2202`, `:2160`, `:2184` |
| Own level adjustment suppression | 1 s | `NotchService.swift:4224` |
| Volume "ride" window (adaptive volume) | 2 s | `NotchSupport.swift:2115` |
| Mission Control poll | 80 ms (overview up / concealed) else 250 ms; probe every 500 ms; restore after 3 desktop readings; fade 140 ms | `NotchWindowHost.swift:534-621` |
| Menu-space AX timer (mac only) | 1 s ± 0.2 | `NotchService.swift:3334` |

---

## 3. Animations

### 3.1 Silhouette resize engine (the core "feel")

Implemented as a Core Animation path keyframe animation on the window's mask (`NotchWindowHost.swift:163-345`, math in `NotchSupport.swift:2860-3112`). Rules to reproduce:

1. **Width and height spring independently** (`NotchMotion.size`, `:2929-2936`). Spring per side chosen by direction (`:2909-2913`):

| Spring | duration / bounce | ζ | Framer `stiffness` / `damping` (mass 1) | Overshoot |
|---|---|---|---|---|
| growingWidth | 0.44 / 0.25 | 0.75 | **203.9 / 21.42** | 2.84 % |
| growingHeight | 0.38 / 0.22 | 0.78 | **273.4 / 25.79** | 1.99 % |
| shrinkingWidth | 0.30 / 0 | 1 | **438.6 / 41.89** | 0 |
| shrinkingHeight | 0.26 / 0 | 1 | **584.0 / 48.33** | 0 |
| steadyWidth | 0.50 / 0 | 1 | **157.9 / 25.13** | 0 |

   Growing: the island drops a little ahead of widening and overshoots; shrinking: pulls up ahead of narrowing and never passes the target.
2. **Overshoot limit 12 pt** (`:2905`, `:2889-2893`): if `travel·overshoot > 12`, bounce is reduced to `b' = 1 + ln(12/travel)/√(π² + ln²(12/travel))`. Example 106 → 560 (travel 454): b' = 0.244.
3. **Progress function** (exact, `:2871-2879`): underdamped `1 − e^(−ζω t)(cos ω_d t + (ζω/ω_d) sin ω_d t)`, critically damped `1 − e^(−ω t)(1 + ω t)`, ω = 2π/d, ω_d = ω√(1−ζ²).
4. **Steady width**: a notice re-shown with a new reading of the same event, or the music strip naming its song, eases width with `steadyWidth` and no swing, *only* if height is unchanged (`NotchWindowHost.swift:260-262`, `NotchService.swift:2363`, `:2650-2656`).
5. **Centre shift** (`surfaceShift` change): offset decays as `start·(1 − progress)` using the width spring (or steadyWidth when width is unchanged) (`NotchSupport.swift:2970-2974`).
6. **Settle** when width, height and offset all stay within **0.5 pt** of target (`:2907`, `:2951-2965`); keyframes sampled at **120 fps**, final frame exactly the target (`:2978-2985`). Simulation horizon 2 s.
7. **Retargeting** (`NotchMotionTimeline`, `:3011-3112`): if a new target arrives **in the same direction** on both axes (and no centre offset / steady mode), the motion **continues** by adding a new spring leg from the previous target to the new one, keeping velocity (sum of legs, clamped to `max(from, to) + 12`). If either axis **turns back**, the motion **restarts from the currently drawn size with zero velocity** (`NotchWindowHost.swift:269-272`). Framer Motion's default retarget keeps velocity in both cases; for reversals reset velocity to 0.
8. Representative timings (computed from the above): capsule rest 106×23.5 → spacious list 560×340: settle **562 ms**, "arrival" (all sides within 1 %) **250 ms**; 560×340 → rest: settle **437 ms**; simulated notch: hover pulse 135×24 → 155×29: **375 ms**; pulse back: **267 ms**; rest → peek 340×86: **537 ms** (capsule 106×23.5 → 340×68: **542 ms**; capsule pulse is width-only); rest wings 135 → 223: **500 ms**.
9. **What moves**: only the mask path and the black/glass backdrop follow the animation (the backdrop is driven one frame ahead by a display link, `NotchWindowHost.swift:1003-1013`, `:1413-1418`). **Content is laid out at the destination size immediately** and is revealed/cut by the moving mask. The OS window is resized only twice: to the envelope at start and to the target at settle.
10. Reduce Motion: no spring; the island jumps to its size (`NotchWindowHost.swift:190`, `:254-257`).
11. Quick-access bubbles appear at `arrivalTime(from,to)` (`NotchWindowHost.swift:316-319`); on withdrawal the island waits 0.16 s for them before shrinking (`:330-333`).

### 3.2 Content transitions (`NotchWindowHost.swift:1458-1630`)

Applied to the content layer only (never to the backdrop). All "opacity" values are a mask over content pixels.

| Kind | When | Opacity keyframes | Blur (gaussian radius) | Scale (from island top-centre) |
|---|---|---|---|---|
| **reveal** | closed → open, notice appears, peek, reveal from hidden | `[s, s, 1]` at t `[0, 0.35, 1]` of **450 ms**; segment 1 linear, segment 2 ease-out. `s = 0` (or current opacity if interrupted mid-fade) | `[12, 6, 0]` same key times (linear, ease-out) | **0.86 → 1** over 450 ms, `cubic-bezier(0.22, 1, 0.36, 1)` |
| **dismiss** | open → closed | `[0, 0, 1]` at `[0, 0.65, 1]` of **400 ms** (new closed content is invisible for 260 ms, then fades in over 140 ms) | `[6, 6, 0]` | none |
| **replace** | page → page while open, notice of another kind, timer mode | crossfade (snapshot fade) **180 ms**, ease-in-out | none | none |
| **depart** | a notice or the music strip leaves while the shape closes | current → 0 in **160 ms**, ease-in, held at 0; at **200 ms** the view swaps content; new content fades 0 → 1 over **140 ms** ease-out starting **60 ms** later | none | none |
| reveal, shape snaps | Reduce Motion or window was hidden | `[0, 0, 1]` at `[0, 0.25, 1]` of **200 ms** | none | none |

Blur/scale are removed when the transition ends. A reversed transition resumes blur/scale from the presented values (`:1506-1509`).

### 3.3 Other animations

| Element | Animation | Framer equivalent | Ref |
|---|---|---|---|
| Button press scale 0.965 (hover 1.022, only `lifts` buttons) | `spring(response .28, damping .7)` | k 503.6, c 31.42 | `NotchComponents.swift:28-32` |
| Button hover | `spring(response .32, damping .75)` | k 385.5, c 29.45 | same |
| Quick-access reveal | `Spring(0.38, 0.12)` delayed by arrival time | k 273.4, c 29.10 | `NotchQuickAccessView.swift:44-46` |
| Quick-access hide | ease-in 160 ms | | same |
| Quick-access re-layout | the island's side spring | | `:18-28` |
| Activity picker choice change | `.smooth(0.26)` + `matchedGeometryEffect("choice")` sliding white highlight + strip `.blurReplace` | k 584, c 48.33; use `layoutId` | `NotchService.swift:573-584`, `NotchView.swift:917-924`, `:183` |
| Gallery row paging | `Spring(0.3, 0)` on y offset (−row·94) | k 438.6, c 41.89 | `NotchSectionsView.swift:61-65` |
| Gallery dots | ease-out 150 ms | | `:93` |
| Gallery search unfold | ease-out 150 ms (field 1 pt → max 150) | | `:214-216` |
| Notice wing change | `Spring(0.5, 0)` (steadyWidth) | k 157.9, c 25.13 | `NotchNoticeView.swift:59-61` |
| Notice detail / meter value | ease-out 140 ms; numeric-text roll | | `:122`, `:138` |
| Clock digits | numeric roll, `.smooth(0.3)` | k 438.6, c 41.89 | `NotchComponents.swift:81-94` |
| Icon symbol swap | `.smooth(0.24)` | k 685.4, c 52.36 | `NotchComponents.swift:156-157` |
| Top-row levels moving aside | `.smooth(0.25)` | k 631.7, c 50.27 | `NotchView.swift:386` |
| Resting wings step aside (companion) | ease-in-out 200 ms | | `NotchView.swift:719` |
| Companion ↔ activity swap | ease-in-out 200 ms; `notchFadeThrough` (opacity `max(0, 2p − 1)`) | | `NotchView.swift:27-34`, `NotchComponents.swift:126-142` |
| Capsule music bars ↔ play button | ease-out 150 ms | | `NotchCapsuleViews.swift:324` |
| Rail scroll-to | ease-out 160 ms | | `NotchComponents.swift:341` |
| Scroll edge fade | ease-out 180 ms, 16 pt | | `NotchScrollEdgeFade.swift:46-56` |
| Slider external change | 220 ms, `1 − (1−p)³` | | `NotchSliderEditing.swift:29-47` |
| Mission Control hide/show | alpha fade 140 ms | | `NotchWindowHost.swift:616-621` |
| Decorative loops (bars, pulses) | stepped at ≤ 30 fps | `requestAnimationFrame` throttled | `NotchDecorativeClock.swift:16` |

`.transition(.opacity)` on notice buttons and resting views (`NotchView.swift:165`, `:216-219`). No other `matchedGeometryEffect` in the shell.

---

## 4. Live activity / compact priority

- Candidates in fixed order (`NotchSupport.swift:1791-1799`): **timer → watch → sessions → downloads → agents → calendar → music → keepAwake**. Availability (`NotchService.swift:485-525`): timer session shown; watch active; Claude sessions running with counts; active downloads (route on); agents live (pref `notchAgentsLiveActivity`); calendar countdown in window (countdown/time-left prefs); music playing (`notchShowPlayingMusic` & music section & idle content not none, or companion resting); keep-awake session (`notchKeepAwakeActivity`).
- **Winner**: the user's pick from the activity picker if still available, else the first candidate (`NotchActivitySelection`, `NotchSupport.swift:915-945`). The pick survives gaps (song change) and resets only when no activity remains. A new song waiting for its notice is excluded until the notice (`NotchService.swift:527-530`).
- **Picker**: hovering the closed island with ≥ 2 candidates shows the picker instead of opening (`NotchService.swift:539-543`); the strip stays on top, choices below in up to 3 columns (white pill = chosen, `matchedGeometryEffect`), plus a "Combine" menu.
- **Combinations** (companion takes the left wing, primary keeps its reading on the right) only when explicitly chosen (`NotchSupport.swift:1806-1819`): primary timer → downloads, or (timer running) agents / calendar / music; primary calendar → downloads, agents, music.
- **Slots** (hanging shape): left wing = mark/cover/companion mark, right wing = reading/bars; camera gap in between (Windows: 135 pt of black). Capsule: one row end-to-end.
- **Resting content** when no activity (`notchIdleContent`, default `music`; `NotchSupport.swift:1956-1982`): `none` / `battery` / `music` / `agents`. `music` only shows while playing; battery needs a battery.
- **Notices** are not queued: one notice at a time. A new one replaces the current if `incoming.priority ≥ current.priority`; while a banner preview is held only the same kind or strictly higher priority may replace it (`NotchSupport.swift:2094-2098`). Priorities (`:1624-1631`): 3 = volume, brightness, keyboard light, microphone, input source; 2 = capture, timer, watch, sessions; 1 = battery, system notification, accessory, agents; 0 = clipboard, download, track. While the island is expanded, dragging or showing capture controls a notice is still *accepted* and times out normally, but it is drawn behind the open page (no content transition, `noticeCanPresent`, `NotchService.swift:2359-2360`, `:2531-2533`); only level notices surface, in the top row (below). Opening the island discards a mirrored banner (`:1467`). The microphone notice is refused outright while open (`:2475`). All notices are suppressed in hidden-until-hover mode unless "shows activity" (`showsSystemFeedback`, `:1099-1101`).
- Level notices while the island is open are drawn in the top row instead (header feedback), except on Controls where the slider itself moves (`NotchView.swift:513-528`).

---

## 5. Sections navigation

- Module list = stored order (`notchModuleOrder`, comma list) then remaining `NotchModule.allCases` (`NotchSupport.swift:9-10`, `:1846-1862`): controls, mixer, music, clipboard, captures, files, system, tools, calendar, notifications, timer, camera, downloads, scratchpad, agents, sessions, watch; minus `notchHiddenModules`, unavailable features, and per-section enable keys (timer, camera, calendar, notifications, agents, sessions, watch).
- Opening destination (`reopeningDestination`, `NotchService.swift:1369-1387`): a held banner → Notifications; else visible compact activity's page if `notchOpensActivity` (default on); else if `notchReturnHome` → `notchHomeModule` (a module, `appPanel`, or `explore` = gallery); else last page.
- **Gallery** (`NotchSectionsView.swift`): tiles min 92 × **86**, spacing 8, radius 16, columns = `railCapacity(contentWidth − 12, 92, 8)` (spacious: 5), visible rows `≤ floor((320 + 8)/94)` = 3, rows step whole with a dot indicator (6 pt dots in 12 pt targets, spacing 5, α0.9 current / α0.28) on the right in a 12 pt column. Tile: SF symbol 22 medium (music pink, calendar red, timer orange, others white α0.85), title 11 medium ≤ 2 lines, shortcut label "⌥⌘X" 10 rounded α0.6 (Windows: "Ctrl+Alt+X"), current section has a checkmark (10 pt, α0.7, top-right inset 7). Search lives in the top row (magnifier that unfolds on hover or typing; typing filters immediately).
- Gallery is reached by: Explore bubble (default left), the grid button in the row (only if no Explore bubble), ⌘K, context menu, peek's navigation button.
- Settings gear: there is **no gear in the island**; Settings is a default **right bubble**, the context menu, and ⌘, (`NotchView.swift:275-279`).
- Detail pages (metric, app panel) show "‹ Title" back in the row; Escape/back return to the page if entered from it (`detailHasPage`).
- Sessions detail can **enlarge** the island (1.4.2) while its focused session is shown (`NotchService.swift:1529-1539`).

---

## 6. Window behavior

### 6.1 macOS facts

- `NSPanel`, borderless, **non-activating**, non-opaque, clear background, no shadow, dark appearance, `hidesOnDeactivate = false`, no animation behavior (`NotchWindowHost.swift:61-75`). Accepts key focus only while open (`acceptsKeyFocus`, `:880-883`, `NotchService.swift:1441`, `:1500`); never main. First click acts (`acceptsFirstMouse`, `:1790`, `:1801-1803`). Accessibility subrole "unknown" so tiling WMs ignore it (`:895`).
- **Level**: status bar + 1 (above status items, below native menus) (`:878`); `floating` when hidden-in-fullscreen + simulated cutout + not covering menus (`NotchService.swift:2714-2718`); shielding + 1 during screen-capture selection (`:2114`). Alerts/save panels from inside the island are raised to island level + 1 (`NotchComponents.swift:900-963`, `NotchIslandAlert.swift`).
- **Spaces**: `canJoinAllSpaces, fullScreenAuxiliary, stationary, ignoresCycle` (`NotchWindowHost.swift:873-875`) **and** a private CGS Space of its own so desktop swipes and Show Desktop don't move it (`NotchOverlaySpace.swift`).
- **Fullscreen**: shown over fullscreen apps by default. Opt-in `notchHideInFullscreen` (default off): when the island's display shows a fullscreen Space (`SpaceWindowBridge.swift:104-109`), it collapses, clears notices and (non-notched) hides at rest; hover (if enabled) and the hotkey still open it (`NotchService.swift:3527-3549`, `:432-447`).
- **Mission Control**: hides itself (alpha 0, ignores mouse) while an overview is up, detected by a window-list scan + a frame probe; restores after three desktop readings (`NotchWindowHost.swift:426-621`, `:776-870`).
- **Pass-through**: per-pixel — the window server sends clicks on fully transparent pixels to windows below; views return `nil` from `hitTest` outside the *visible* (animated) path (`NotchWindowHost.swift:1289-1312`) or outside bubble circles (`:995-1000`). Hover tracking area = destination rect (`:1776-1782`).
- **Screen-edge clicks**: the menu bar owns the top pixel row, so global mouse monitors turn a press/release in the top 1 + gap pt over the island into an open (`NotchService.swift:3219-3291`). On Windows the island window owns that row; not needed.
- **Captures**: `sharingType = .readOnly` or `.none` per `notchShowInCaptures` (default on) (`NotchService.swift:3501`).
- **Multi-display** (`notchDisplay`, default `automatic`; `NotchSupport.swift:110-114`, `:2151-2176`, `NotchService.swift:3406-3503`):
  - `automatic`: built-in notched → any notched → main. **Windows: main (primary) display.**
  - `builtIn`: built-in display; lid closed → island withdrawn; Mac without built-in → main.
  - `main`: display with the menu bar (Windows: primary monitor).
  - `pointer`: moves (closed and settled only) to the display the pointer stays on for 200 ms.
  - `all`: pointer behavior + a **mirror** window on every other display showing the closed island (strip or resting), no hover/notices; clicking a mirror moves the island there and opens it (`NotchService.swift:3092-3217`, `NotchMirrorView.swift`).
- **Display change**: `didChangeScreenParameters` → 100 ms debounce → full resync / geometry rebuild; missing chosen display ⇒ withdraw (`NotchService.swift:3349-3361`, `:3446-3458`).
- **Sleep / lock / user switch**: system sleep, display sleep, lock, session resign ⇒ `canPresent = false` ⇒ the whole presentation is torn down (window closed); wake/unlock ⇒ rebuilt by `syncWithPreferences` (`NotchService.swift:3568-3636`, `:3678-3719`). Screen saver affects only the lock-screen overlay. Timers keep running through display sleep.
- Volume/brightness hardware keys are intercepted only while the island can show feedback (`NotchVolumeKeyGate.swift`; Option+Shift = fine 1/64 steps, else 1/16).

### 6.2 Windows 11 equivalents

| macOS mechanism | Windows 11 proposal |
|---|---|
| Borderless non-activating `NSPanel` | Tauri window `decorations:false, transparent:true, shadow:false, skipTaskbar:true, alwaysOnTop:true, resizable:false, focus:false`; add `WS_EX_TOOLWINDOW` (no Alt-Tab/taskbar) and `WS_EX_NOACTIVATE`; answer `WM_MOUSEACTIVATE` with `MA_NOACTIVATE` so clicks act without taking focus. On `open(takeFocus:true)` clear `WS_EX_NOACTIVATE` and `SetForegroundWindow` (allowed during user input / hotkey). On collapse restore `WS_EX_NOACTIVATE` and return focus to the previous foreground window. |
| Level status bar + 1 | `HWND_TOPMOST`; re-assert `SetWindowPos(HWND_TOPMOST)` on `EVENT_SYSTEM_FOREGROUND` (other topmost apps can cover it). Menus/dialogs it opens must be topmost too (island level + 1). |
| canJoinAllSpaces / own Space | Virtual desktops: verify a topmost tool window stays on desktop switches; if not, pin via undocumented `IVirtualDesktopPinnedApps::PinWindow`, or re-`MoveWindowToDesktop` on desktop change (registry `VirtualDesktops\CurrentVirtualDesktop`). |
| Show Desktop stationary | Verify Win+D keeps a topmost tool window; it should not be minimizable (no `WS_MINIMIZEBOX`). |
| Mission Control concealment | Not needed: Task View / Alt+Tab draw above topmost windows (verify). Keep a hook to fade (140 ms) if Task View is detected (`EVENT_SYSTEM_SWITCHSTART/END`). |
| Per-pixel pass-through + `hitTest` | WebView2 swallows clicks on transparent pixels, so toggle `set_ignore_cursor_events`: a Rust low-level mouse hook (`WH_MOUSE_LL`) or a 60–120 Hz `GetCursorPos` poll *while the cursor is within the window rect* tests the point against the shape JS publishes each frame (W, H, shift, floatingGap, bubble centres, open child popovers) using the formulas of §1.3 (destination rect for hover, visible path for clicks, capsule top strip counts). Ignore = outside; accept = inside. |
| Window envelope resize | Prefer a **fixed** transparent window per display: width = monitor width (or `limit + 2·72`), height = `0.8·H + 72` max island height; never resize during motion (avoids WebView2 resize lag). Animate the shape in CSS/SVG only. |
| Hover tracking areas / global move monitors | Same Rust hook emits enter/exit to JS; emulate "hidden until hover" (window has `WS_EX_TRANSPARENT`, invisible) by testing the collapsed rect in the hook. |
| Click outside (global monitors) | `WH_MOUSE_LL` button-down outside shape ⇒ collapse (respect `keepsWorkingSurface`, own child windows). Also `EVENT_SYSTEM_FOREGROUND` to another process ⇒ apply `closesOnActivation`. |
| Fullscreen Space detection | `SHAppBarMessage(ABM_NEW)` appbar callback `ABN_FULLSCREENAPP` per monitor, plus `SHQueryUserNotificationState` (`QUNS_BUSY`, `QUNS_RUNNING_D3D_FULL_SCREEN`, `QUNS_PRESENTATION_MODE`) and foreground-window-rect == `rcMonitor` (excluding `Progman`/`WorkerW`/shell). Re-check on `EVENT_SYSTEM_FOREGROUND` / `EVENT_OBJECT_LOCATIONCHANGE`. |
| Monitor enumeration / built-in / main | `EnumDisplayMonitors` + `GetMonitorInfo` (`MONITORINFOF_PRIMARY` = main); built-in via `QueryDisplayConfig` output technology `INTERNAL` / `DISPLAYPORT_EMBEDDED` / `UDI_EMBEDDED`; DPI via `GetDpiForMonitor`; lid via `RegisterPowerSettingNotification(GUID_LIDSWITCH_STATE_CHANGE)`. |
| Pointer follow | Cursor hook + `MonitorFromPoint`, 200 ms dwell. |
| Screen parameters change | `WM_DISPLAYCHANGE`, `WM_DPICHANGED`, `WM_SETTINGCHANGE(SPI_SETWORKAREA)`, debounce 100 ms. |
| Sleep / display sleep / lock / user switch | `WM_POWERBROADCAST` (`PBT_APMSUSPEND`, `PBT_APMRESUMEAUTOMATIC`), `RegisterPowerSettingNotification(GUID_CONSOLE_DISPLAY_STATE)`, `WTSRegisterSessionNotification` (`WTS_SESSION_LOCK/UNLOCK`, `WTS_CONSOLE_DISCONNECT/CONNECT`), screensaver `SPI_GETSCREENSAVERRUNNING`. |
| Global hotkey (`QuickToolHotkey`) | `RegisterHotKey` (report failure like `shortcutRegistrationFailed`). |
| Show in captures (`sharingType`) | `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` when off, `WDA_NONE` when on. |
| Haptics (`NSHapticFeedbackManager .alignment`) | No equivalent on most hardware: no-op (keep the call sites). |
| Scroll phases / precise deltas | WebView `wheel` events: no phases ⇒ always use the phaseless path (0.35 s sequence gap). Precise = touchpad: heuristics (`deltaMode == 0` and small non-multiple-of-120 deltas, or `WM_POINTER` touchpad). Inverted-direction flag: read Precision Touchpad "scroll direction" setting, else assume false. |
| Liquid Glass / NSVisualEffectView | No shaped behind-window blur in WebView2 (`backdrop-filter` cannot see other windows; DWM Mica/Acrylic applies to the whole window rect). Default black is exact; mark glass/translucent as unsupported or approximate (see risks). |
| `NSMenu` context menus | Native popup menu (Tauri `Menu.popup`) with dark theme, topmost. |
| AX menu-bar measurement | Not applicable: treat as "covers menus" with empty bar room. |
| SF Pro / SF Symbols | Segoe UI Variable (Text ≤ 13, Display ≥ 14) with `font-variant-numeric: tabular-nums`; icon set mapping is a separate task. |

---

## 7. Settings (island preferences)

UI: Settings › Dynamic Island, tabs **Layout · Content · Activity · Behavior · Companion** (`NotchSettingsTabRow.swift:6-8`), master switch in the header, "open island" button (`NotchSettings.swift:145-187`). Any change triggers `syncWithPreferences()` (debounced to the next run-loop turn, `NotchService.swift:3640-3650`).

### 7.1 Shell preferences

| Key | Type | Default | Effect | Ref |
|---|---|---|---|---|
| `notchEnabled` | Bool | false | master switch | `Defaults.swift:1514` |
| `notchSize` | String `compact\|spacious\|custom` | spacious (users configured before the profile marker: spacious if unset; users with the marker but no saved size: compact, `Defaults.swift:2073-2117`) | width 480/560/custom; budget 180/264 | `:1440`, `:2070-2120` |
| `notchCustomWidth` | Double 360…600 step 10 | 440 | custom width | `:1443` |
| `notchCustomHeight` | Double 260…640 step 10 | 480 | custom max height | `:1444` |
| `notchSilhouette` | `capsule\|notch` | capsule | shape on displays without notch | `:1516` |
| `notchCapsuleFitWidth` | Double −40…40 step 2 | 0 | capsule resting/camera width | `:1517` |
| `notchCapsuleFitHeight` | Double −4…4 step 1 | 0 | capsule strip height | `:1518` |
| `notchCapsuleFitDrop` | Double 0…20 step 1 | 0 | capsule distance from top | `:1519` |
| `notchCameraFitWidth/Height`, `notchHideMenuBarGap` | Double / Bool | 0 / false | notched Macs only | `:1442-1446` |
| `notchOutlineEnabled` | Bool | false | 2 pt inner outline | `:1441` |
| `notchTranslucentBackground` | Bool | false | translucent open island | `:1448` |
| `notchLiquidGlassEnabled` | Bool | false | macOS 26 glass | `:1213` |
| `notchQuickAccessLayout` | Data (JSON) | empty → default 5 bubbles | floating bubbles; legacy `notchQuickAccessSide/Second/Third` | `:1539`, `NotchSupport.swift:1464-1482` |
| `notchOpenOnHover` | Bool | false | hover opens/peeks | `:1520` |
| `notchHoverExpands` | Bool | true | hover opens fully vs peek | `:1457` |
| `notchHideUntilHover` | Bool | false | invisible until hovered (needs open-on-hover) | `:1524` |
| `notchHiddenShowsActivity` | Bool | false | hidden island still shows activities/notices | `:1525` |
| `notchHoverDelay` | Double 0.10…1.0 step 0.05 | 0.25 | hover delay | `:1532` |
| `notchCloseDelay` | Double 0.10…2.0 step 0.05 | 0.18 | hover-opened close delay; slider shown only in hover-expand / hidden-until-hover modes (`NotchSettings.swift:654-657`); a peek always closes after 0.12 s | `:1533` |
| `notchShortcutEnabled` / `notchShortcut` | Bool / shortcut | false / ⌃⌥⌘I | global toggle | `:1521-1522` |
| `notchGesturesEnabled` | Bool | true | scroll/swipe gestures | `:1458` |
| `notchHapticFeedback` | Bool | true | haptic on open/peek/paging | `:1447` |
| `notchReturnHome` / `notchHomeModule` | Bool / String | false / "controls" | reopening destination (module, `appPanel`, `explore`) | `:1534-1535` |
| `notchOpensActivity` | Bool | true | opening shows the visible activity's page | `:1536` |
| `notchHideInFullscreen` | Bool | false | hide over fullscreen apps | `:1523` |
| `notchDisplay` | `automatic\|builtIn\|main\|pointer\|all` | automatic | display choice | `:1515` |
| `notchCoversMenus` | Bool | true | island may cover menus (Windows: always) | `:1526` |
| `notchShowInCaptures` | Bool | true | window visible in screenshots | `:1551` |
| `notchHiddenModules` / `notchModuleOrder` | CSV | "" / "" | section visibility / order | `:1537-1538` |
| `notchIdleContent` | `none\|battery\|music\|agents` | music | resting content | `:1435` |
| `notchShowPlayingMusic` | Bool | true | music as live activity | `:1432` |
| `notchKeepAwakeActivity` | Bool | false | keep-awake live activity | `:1455` |
| `notchHiddenControls` / `notchControlOrder` | CSV | `keyboardLight,microphone,screenshot,recording,speedTest,panel,commandBar,scratchpad` / "" | Controls page items | `:1436-1439`, `NotchSupport.swift:1285` |
| Feedback toggles `notchVolume`, `notchBrightness`, `notchKeyboardLight`, `notchMicrophone`, `notchInputSource`, `notchBattery`, `notchClipboard`, `notchCapture`, `notchTrackChange`, `notchAccessoriesEnabled` | Bool | all true | which notices route (`NotchSupport.swift:2022-2051`) | `:1459`, `:1473`, `:1540-1549` |
| Section enables `notchTimerEnabled`, `notchCameraEnabled`, `notchCalendarEnabled`, `notchNotificationsEnabled`, `notchDownloadsEnabled`, `notchWatchEnabled`, `notchAgentsEnabled`, `notchSessionsEnabled` | Bool | true ×6, agents false, sessions false | section + its events | `:1460-1514` |
| Destinations `notchAppPanel`, `notchQuickPanel`, `notchClipboardWindow`, `notchShelf`, `notchDragReveal`, `notchCaptureControls`, `notchScratchpad`, `notchHidesMenuBarIcon` | Bool | false, true, true, true, true, true, true, false | route other app surfaces into the island | `:1449-1456`, `:1546` |
| Low battery `notchLowBatteryTint`, `…Threshold`, `…Early`, `…EarlyThreshold`, `…MenuBar` | Bool/Int | false, 10, false, 20, true | resting battery colour | `:1527-1531` |
| `notchLockScreen`, `notchLockSounds` | Bool | false, false | lock-screen overlay (other spec) | `:1552-1553` |
| `notchMascot*`, `notchCommandBar*` | various | mascot off | companion / command bar (other specs) | `:1554-1564` |

### 7.2 Section-content preferences (owned by other specs, listed for completeness)

Timer (`notchTimerMode` timer, minutes 15, sound on, hide countdown off, Pomodoro 25/5/15/4/4), calendar (countdown off, time-left off, week numbers off, excluded []), agents (providers on, period today, limit display remaining, focus most used, live on, readout elapsed, finish/limit alerts on, budget 0), sessions (live on, auto-expand on, content preview off, large text 15, sidebar shown, buddies on), music (lyrics on, online lyrics off, browser covers off, live equalizer off, queue on, other players on, preferred player auto), watch (on, sound on, condition `changes`), clipboard card size comfortable, `notchDismissNativeNotifications` off (`Defaults.swift:1461-1513`).

### 7.3 Strings

`Core/NotchStrings.swift` (`struct NotchStrings` of `let … : String`, factory `FeatureStrings.notch(_ language:)` at `:101` switching over 15 `AppLanguage` cases), `NotchGestureStrings` (title/description/hint), `NotchTourStrings` (preview/caption). Translations are handled elsewhere.

---

## 8. Acceptance checks worth porting

From the dev probe (`NotchPresentationProbe.swift:42-760`), as test ideas for the Windows build:
- the backdrop always equals the animated silhouette; content transitions never fade the backdrop;
- opening has intermediate frames, starts blurred/small and ends sharp at scale 1; growth stays anchored at the island's top centre;
- the silhouette never detaches from the top edge and never exceeds its reserved area; the hanging shape is upright (shoulder point inside at top, outside at bottom);
- transparent areas of the reserved window never accept clicks; visible content does; a stationary pointer at the screen's top edge is "inside" during motion;
- a burst of same-kind notices does not restart the resize; a reversal never jumps to an endpoint; a carried opening never narrows before reaching its width;
- a departing notice stays visible while the shape closes and is gone when swapped;
- settled glass is fully open; closed compact states never keep glass;
- the transparent gap beside a bubble passes clicks through; a click near a bubble's rim lands on the bubble.

---

## 9. Open questions / risks

1. **No menu bar on Windows.** The Mac capsule sits *inside* a 24 pt menu bar; on Windows a top-centre pill would float over maximized windows' title bars and browser tabs. Options: (a) accept overlap (like Mac apps covering menus, `notchCoversMenus = true`), (b) register an AppBar to reserve a 24 DIP top strip, (c) default to hidden-until-hover. Decide before tuning `B = 24` and `barEdge`.
2. **Virtual bar height.** `B = 24` and `barEdge = 1/dpr` reproduce a Retina Mac exactly (capsule 106 × 23.5). At 100 % scale the capsule becomes 104 × 23. Confirm whether "1:1" means 1:1 DIPs or 1:1 physical pixels.
3. **Capsule vs simulated notch default.** Default is capsule; the simulated notch at 135 × 24 with concave ears may be the more recognizable "Dynamic Island" on Windows. Product decision.
4. **Hit-testing latency.** Toggling `set_ignore_cursor_events` from a cursor hook adds a frame of latency and can drop the first click when the pointer enters and clicks fast; also WebView2 cannot receive hover while ignoring events, so hover must be computed in Rust. Prototype early.
5. **Retarget semantics.** Framer Motion keeps velocity on reversal; macOS restarts reversals from rest. Needs a custom spring driver (or reset velocity) to match.
6. **Glass / translucent backgrounds** have no faithful shaped behind-window blur in WebView2. Either drop the options on Windows or approximate (semi-opaque black, no blur).
7. **Virtual desktops / Win+D / Task View** behavior of a topmost tool window must be verified on Windows 11 23H2/24H2; pinning across desktops may need undocumented COM interfaces that change between builds.
8. **Fullscreen detection** heuristics (borderless-windowed games, browser F11, PowerPoint) differ from macOS fullscreen Spaces; `ABN_FULLSCREENAPP` and `SHQueryUserNotificationState` disagree in edge cases.
9. **Keyboard mapping.** ⌘→Ctrl collides with page shortcuts (Ctrl+K in text fields, Ctrl+1…9). The global default ⌃⌥⌘I has no clean Windows twin; Ctrl+Alt+Win+I proposed but unverified for conflicts.
10. **Focus model.** The Mac island is non-activating but becomes key when opened by click; reproducing "keys go to the island without stealing the app's activation look" is not possible on Windows. The previous foreground window must be restored on collapse.
11. **Ambiguities in source**: the NSAnimationContext timing curve for the Mission Control fade is the system default (not specified in code); `NotchSessionState.screenSaverRunning` only matters for the lock-screen overlay; `notchMusicActivity` and `notchHideInCaptures` are registered but unused in the current code.
12. **Fonts & icons**: SF Pro / SF Symbols cannot ship on Windows; metric differences will change every text-measured width (notice wings, capsule widths). Measure with the actual Windows font at runtime (canvas `measureText`) exactly where Swift measures with `NSFont`.
13. Text measurement is used for layout (capsule widths, wings) on every reading change; re-measure only when the reading's *shape* changes (`NotchAgentSupport.readingShape`) as the Mac does, to avoid jitter.
