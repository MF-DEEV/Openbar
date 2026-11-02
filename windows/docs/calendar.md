# Calendar on Windows

The Calendar section includes a local Vorssaint calendar, available without a
Windows package identity. Use **+** to create an event and click its card to edit
the title, start/end, all-day dates, location, notes and colour. Deletion asks for
confirmation. Events appear in the month/week view and existing countdowns.

Events are saved in `%APPDATA%\com.vorssaint.island\calendar\events.json`.
The native store serializes mutations, validates events and atomically replaces
the file. An unreadable file is reported and never overwritten by a save.
Local events stay on this device; they do not send invitations or sync to an account.

Scroll the month grid to browse earlier or later months. Native scroll snapping
keeps the movement smooth; only five month panels are kept in memory and recycled
after scrolling settles. Adjacent months are read ahead for event markers. The
last viewed month, selected day and view are remembered across app restarts;
these view preferences are separate from the event file on disk.

The existing optional Windows appointment reader remains available to packaged
builds with the appointments capability. System permission is tracked separately
from local availability. Both sources respect the calendar visibility settings;
system appointments open in the default calendar app.

Validation: `npm test`, `npm run typecheck`,
`node scripts/check-island-layout.mjs --calendar`, and
`cargo test --lib calendar` in `src-tauri`.
