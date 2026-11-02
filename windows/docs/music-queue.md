# Up next on Windows

The Music page has an **Up next** toggle next to **Lyrics**, following the
macOS `NotchMusicView` / `NotchQueueView` layout. It expands a panel below the
player with up to five upcoming songs in their actual order. Lyrics and the
queue are mutually exclusive. Escape closes the queue before the island.
Expansion follows the island's existing size animation with a 280 ms fade/slide
transition and respects Reduce Motion. Rows use 40 px covers and stable per-song identities. When
skipping, existing rows slide upward and the new final row fades in. The panel
and retained cover images stay mounted while the next snapshot loads.

The refresh icon stays unchanged during background polling; only a manual
refresh animates it. Each native reading supplies five visible songs and up
to five lookahead songs. Their covers are downloaded and decoded in advance,
with shared image URLs coalesced across songs on the same album. On a known
skip the fifth visible slot is filled immediately from this prepared buffer,
before the next native reading completes. Ready covers have no second fade-in
or deferred image-loading phase when their row becomes visible. An unexpected
queue edit, a rapid jump beyond the buffer or a missing/offline image can still
require a fresh lookup; the app does not substitute another song's cover.

## Automatic cover artwork

While Up next is open, titles and
the primary artist of upcoming songs are sent to Apple's public iTunes Search
API; matched artwork loads from Apple's image CDN. No account token is used.
Songs absent from that catalog use Deezer's public search and cover CDN. Its
metadata request runs through a fixed-host native HTTPS command because the
browser API does not enable CORS; no account, cookie or token is used. Title
and primary artist must match before a returned cover is displayed.
This is independent of the local Spotify queue reader. There is no artwork
toggle, provider label or external cover link in the panel. A cover may represent a different release
of that recording because Spotify's local queue does not supply album IDs.

Matching requires both title and artist; remixes with a different title are
excluded. Missing matches, offline responses and image failures retain the
music placeholder. Requests are coalesced, results cached in memory, and lookups
are limited to 60 per minute. Featured-artist title formatting is normalized,
but remix/live/version suffixes remain significant. Cover downloads are paused
when the queue is closed (already-started requests may complete).

The list has five fixed 48 px slots and clips animated content without becoming
a scroll container. Rows move using transforms and opacity, so departing rows
cannot increase the list height or introduce scrollbars. Retained rows and images
keep their DOM identity across skips. Brief accessibility failures retain the
last validated list for up to six seconds with 500 ms recovery reads; sustained
failures show the unavailable state rather than stale rows indefinitely.

## Supported source

The initial provider supports **Spotify desktop on Windows**, with its Queue
button named in English or Polish. It uses Windows UI Automation and Spotify's
accessibility tree, including when Spotify is minimized. Opening Up next may
open Spotify's own queue side panel in the background; it never restores its
window, sends playback commands or changes queue order. It requests the
standard Chromium accessibility handshake when necessary. No Spotify account
token, browser cookies or external login is used for the queue itself.

Windows' SMTC interface exposes the current song and transport actions, but no
general upcoming-items API. Other sources therefore show the existing localized
"This player does not share its upcoming songs" state and an Open Player action.
An unreadable Spotify queue also shows that state; it must never be confused
with a genuinely empty queue. This provider depends on Spotify's exposed queue
structure and may need updating when Spotify changes it.

The reader scopes rows to `queue-panel`, verifies its Now Playing title against
the selected SMTC song, and returns up to five visible rows plus five lookahead
rows for artwork preparation. If minimized
Spotify retains its previous current-song row, a unique occurrence of the real
current song in the exposed queue anchors the result. Ambiguous or missing
matches return unavailable. At most 24 upcoming rows are inspected to handle
this cached-queue case. It preserves duplicates and explicit queue ordering.
Results are validated again against the selected source/song before delivery.
The frontend rejects late requests after a source/song change or panel close.
The panel refreshes every 2.5 seconds only while mounted; native reads are
serialized and accessibility connection/transaction timeouts are bounded.

## Validation

- `npm test`: includes request ownership, five-row limit, real empty queue and
  stale/cancelled snapshot cases in `queueStore.test.ts`.
- `node scripts/check-island-layout.mjs --music-queue`: browser regression for
  five ordered fixture rows, fitting the panel, opening/closing, Escape,
  mutual exclusion with Lyrics, no playback commands, and stopped polling.
  It writes a visual fixture screenshot under the worktree's build output.
- `node scripts/check-island-layout.mjs --music-queue-artwork`: verifies five
  real online covers, larger sizing, a skip transition with retained DOM nodes
  and images, and absence of a loading/empty-panel flash.
- `Vorssaint.exe --music-queue-probe <output.json> <current-title>`: optional
  on-demand native diagnostic for the current Spotify desktop queue. It does
  not start the island or alter playback. A title mismatch returns unavailable.

References: [Windows SMTC session API](https://learn.microsoft.com/en-us/uwp/api/windows.media.control.globalsystemmediatransportcontrolssession)
and [Chromium accessibility handshake](https://www.chromium.org/developers/design-documents/accessibility/).
