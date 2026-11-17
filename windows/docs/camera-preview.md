# Camera preview

The island's Camera page shows a live, video-only preview through WebView2's
existing `getUserMedia` support. The bar below the image selects a discovered
camera or the system default, refreshes the device list, and toggles a mirrored
preview. The image keeps its aspect ratio and is centered with a 20% zoom,
retaining side margins for narrower cameras while slightly cropping the top and
bottom. Camera selection uses the existing settings service
(`island.cameraDeviceId`); mirroring lasts for the current page.
No images are recorded or uploaded, and microphone access is not requested.

The page releases its stream when the island closes, the person selects another
section, the sections gallery opens, or the document becomes hidden. Outstanding
permission replies are discarded and their tracks stopped. Device acquisition is
serialized across page transitions to avoid competing opens of the same device.
A missing remembered camera falls back to the default. Device changes refresh the
picker. Disconnections, denied access and busy devices display recoverable errors.
Windows/browser permissions remain in control; no permission override is added.

Verification:

- `npm run typecheck`
- `npm test` (543 tests passed, including 13 camera lifecycle/preference tests)
- `npm run build`
- `node scripts/check-island-layout.mjs --camera` (synthetic video, selection,
  persisted preferences, mirror, six layouts, menu Escape, errors/retry, stream
  cleanup and delayed permission replies)

Native WebView2 verification confirmed live frames from Web Camera (640×480) and
OBS Virtual Camera (2560×1440), switching through the page's picker, release of the
previous stream, outside-click dismissal of Music, and Escape dismissal of Camera
with hardware release. Camera formats use device defaults: negotiating a suggested
720p format caused `NotReadableError` on this machine's webcam driver.
