# Screenshot preview (Issue #29)

Capturing a screenshot still saves it to Pictures/Screenshots and copies it
to the clipboard. The expanded island presents the screenshot on a black
surface, with a toolbar above a centered image and the usual side buttons,
matching the macOS presentation. It replaces the island's capture notice.

If the island is disabled, unavailable, on another monitor or handling another
screenshot action (or retaining a shared screenshot), the preview falls back to
a small window at the bottom right
of the capture monitor's work area. Placement uses physical coordinates and
that monitor's scale factor. A failed island presentation falls back after
three seconds; a floating WebView that cannot show the image within ten seconds
falls back to the original notice with an error. New captures replace idle
transient previews; pinned or busy previews retain their own image.

- **Discard** moves the automatically saved screenshot to the Recycle Bin,
  then closes the preview. It refuses to recycle a file changed by another
  app. Clipboard contents and additional copies made with Save As are kept.
- **QR** scans the original pixels locally and displays decoded text for
  inspection/copying. It never opens detected links automatically.
- **Copy Text** uses the existing Windows OCR engine and installed languages.
- **Save** opens a native PNG Save As dialog. **Copy** recopies the image.
- **Share** opens Windows Share using the existing Files integration. It pins
  the preview so its backing file survives while the share target reads it.
  Availability depends on Windows and installed sharing targets; failures are
  reported in the preview.
- **Pin** transfers the image from the island to an independent, always-on-top
  window and closes the island presentation. Drag its title area to move it;
  the image remains proportional. Unpinning the floating preview starts its
  countdown. A failed transfer leaves the original island preview available.
- Drag the **image or grip** with the left mouse button into another app to
  copy the actual screenshot PNG using the existing native OLE file drag.
- **X/Escape** closes the preview while keeping saved files. Ctrl+C copies
  the image and Ctrl+S opens Save As while the preview has keyboard focus.

Settings → Content → Tools configures the preview duration (including
“Until closed”) and whether losing focus closes it. The preview does not take
focus when shown; the focus-loss option applies after the user activates it.
Hovering, recognition, saving, dragging and QR inspection pause the countdown
without resetting its remaining time. Pinning overrides expiry/focus loss.
Window destruction removes listeners, timers and its temporary PNG. In-flight
native actions retain ownership until completion.

Upload links and an image editor are outside this implementation's scope.

## Validation

Automated tests cover countdown expiry, repeated hover/busy pauses, pin/unpin,
persistent previews, disposal, consecutive previews, duplicate notice
suppression, settings validation, work-area/DPI placement, QR decoding from
RGBA pixels, and backing-file ownership/cleanup. Existing capture, OCR,
clipboard and shortcut tests remain in the normal suites.

After rebasing onto `main` at `76fa0f17` on 2026-10-29, 523 frontend tests and
411 Rust tests passed, with six pre-existing ignored Rust tests.
TypeScript, frontend build, native build,
Clippy with warnings denied, rustfmt and Prettier for changed files passed.
The full-repository Prettier check reports pre-existing formatting/CRLF issues
in other files. Browser automation also validates the embedded preview across
32 combinations of size, silhouette and DPI, plus Escape dismissal and CSS
isolation from the scratchpad. Run it with
`node scripts/check-island-layout.mjs --screenshot-preview`; the visual artifact
is `build/screenshot-preview-macos.png` at the repository root.

A live WebView2 smoke test against the original installed feature build also
passed: real capture/selection, automatic PNG save, clipboard copy, embedded
image identity, Pin transfer to a separate window, another capture while the
pin remains open, Discard, hover pause/resume and auto-dismiss. The diagnostic
browser port was enabled only for this validation and removed by restarting
the app normally afterward. Physical multi-monitor/DPI combinations and
external drag destinations still need interactive checks.

On this Windows machine, the Rust test harness initially failed to load
`TaskDialogIndirect` because it lacked the Common Controls v6 manifest.
Applying `src-tauri/windows-app.manifest` to the generated test EXE with the
Windows SDK's `mt.exe -manifest ... -outputresource:...;#1` allowed the tests
to run. This changes only a build artifact; the application already embeds
that manifest. The full suite needs execution outside the restricted sandbox
for its existing child-process and named-pipe tests.

Interactive acceptance checks still require a Windows desktop:

- Take an area/full-display screenshot through existing Tools/shortcuts;
  verify the saved PNG, clipboard image, preview image and absence of a second
  island notification.
- Repeat on monitors left/above/right of the primary monitor, with taskbars
  and 100%, 125%, 150%, 200% scaling. Check positioning and image proportions.
- Take rapid captures, including while another preview is pinned, recognizing
  text, saving or dragging. Confirm images/actions never switch captures.
- Hover, leave, pin, unpin, activate and switch apps with both focus-loss
  settings; test “Until closed” and closing during/after actions.
- Use OCR with/without an installed language, QR/no-QR captures, clipboard
  contention, cancelled Save As, inaccessible destinations, Share with/without
  available targets, and Discard/Recycle Bin restore.
- Drag into Explorer and an image-accepting external app, including Escape
  cancellation; compare the dropped PNG to the captured image.
