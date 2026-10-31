# Music sources (issue #44)

The Music page's sliders button opens a dark options panel. **Volume** is at
the top, followed by the currently discovered Windows media sessions,
**Automatic**, **Preferred player** and **Include other players**. A manual choice keeps following that player while
paused. If it closes or has no track, Automatic fills the gap; the manual
choice is retained and resumes when the player returns. This temporary choice
lasts until Vorssaint restarts, matching macOS.

Automatic prefers playing music apps, then playing other media apps when
enabled, then a previously followed paused non-music player, then paused music
apps. Previous/system ownership breaks ties. **Include other players** defaults
to on and only filters Automatic: explicit choices always take precedence.

**Preferred player** is separate: it chooses the application to open while
nothing plays, rather than forcing automatic playback to follow that app.
Automatic uses the last music player and then an installed music app. These
two preferences persist through the existing settings service and can also
be changed in Settings → Content → Music.

Installed music apps are discovered through the Windows Apps folder, with the
existing music-app classifier because Windows has no universal music category.
Every live SMTC source remains selectable, including unknown players and
browsers. A saved unavailable preference remains in Settings and falls back
while its application is missing. Discovery adds no polling loop; it runs at
startup and when opening a player picker/settings view.

The existing media reader publishes the chosen metadata, cover and position
to both the expanded page and strip. Transport commands carry that displayed
source's ID and reject stale contexts rather than re-running automatic selection
and controlling another app. Volume and open-player APIs retain their source IDs.

Tests: `npm test`, `cargo test --lib`, and
`node scripts/check-island-layout.mjs --music-sources` from `windows`.
The existing lyrics, layout and playback tests remain applicable.
