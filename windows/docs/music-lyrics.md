# Windows Music lyrics

Open Music and select **Lyrics** to show the lyrics panel. LRCLIB lookup runs
automatically when the SMTC song changes and uses title, artist, album and
rounded duration when available. Missing title or artist skips the request.
Lookup falls back to search, prefers timed lyrics, checks recording metadata
and duration (within two seconds), and retains plain lyrics when available.
Instrumental, unavailable, loading and connection failure states share the
same panel; failed or unavailable lookups can be retried.

The frontend uses the existing SMTC samples, playback rate and extrapolated
position. Highlighting wakes at verse boundaries, stops while paused, and
reschedules after seeks, playback updates or timing changes. Players without
a duration show readable lyrics without a synchronized highlight.

Scrolling with the mouse wheel, scrollbar, touch or keyboard pauses automatic
following while highlighting continues. When the active verse leaves the view,
the **Synchronizuj** button returns to it and resumes automatic following.
Click a timed verse to seek to its timestamp plus the saved timing adjustment
and resume following. Paused playback resumes after a successful seek. This
action is available only when the player supports seeking.

Use the file icon to import a UTF-8 `.lrc` file up to 128 KB. Multiple timestamps,
fractional seconds, metadata, BOM, CRLF, empty timed verses and `[offset:…]`
are supported. Equal timestamps combine into one verse. Like macOS, positive
file offsets advance timestamps; positive user timing delays highlighting.
Timing controls move by 0.5 seconds within ±10 seconds; clicking the number
resets it. Imported lyrics override automatic results, including pending
requests. Song switches cancel requests and reject older response generations.

Imported files and offsets persist in the WebView's local storage under
`vorssaint.music.lyrics.v1`, for the most recently modified 100 recording
identities (title, artist, album and rounded duration). They survive restarting
the app and are shared across players reporting identical metadata. Storage
failures keep the current lyrics usable and show a save error. Automatic
results stay in a bounded 50-record memory cache: one day for lyrics, one
minute for misses, 15 seconds for errors. Requests time out after 15 seconds;
network errors and rate limits do not trigger search requests.

Validation:

- `npm test` covers parsing, synchronization, metadata validation, stale responses,
  caches, timeouts, imports and persistent offsets.
- `npm run test:music-lyrics` runs the real React interface in headless Edge and
  checks file input, highlighting, scrolling, timing, pause/resume, seeking and
  panel bounds. It uses synthetic fixtures exclusively in test code and writes
  `../build/music-lyrics.png`.
