# App notifications in the Windows island

The island mirrors new Windows desktop notifications from Discord, Messenger,
and other applications. The compact banner shows the source app's icon and name
and the sender/title supplied by Windows. Its expanded preview and the searchable
Notifications history also show the message text supplied by Windows. Apps may
redact or omit text; Openbar does not read accounts or invent missing details.
The source application's native icon is preferred; Discord and Messenger have
bundled fallback icons.

The latest 200 notifications are stored locally in
`%APPDATA%\com.openbar.island\notifications\history.json`, including icons,
source, sender, message and time. Writes are atomic. A file that cannot be read
is preserved, and the page reports a saving error. Clear All and Dismiss change
this local history; they leave Windows Notification Center unchanged. Dismissed
Windows IDs are remembered so clearing history does not re-import those toasts
after restarting the app.

Holding the pointer on a banner keeps it on screen, and after the hover delay it
opens in place with the app, time and sender, Open, Dismiss and the inbox count.
Discord notifications also offer Reply. It smoothly grows a draft field inside
the preview; Copy & open copies the draft and opens Discord, where the person
pastes and sends it. Open launches the installed Discord through its registered
`discord://` protocol. The Windows notification listener cannot activate another
app's reply action or send a Discord message; no message is sent by Openbar.
With opening set to click, ordinary sections still open on click while a held
notification can expand on hover. Leaving closes the preview; the notification
stays in the inbox. Clear All on the Notifications page asks first.

An incoming notification waits for a currently visible session/timer notice
instead of being dropped by its higher priority. The pending queue is bounded to
10 fresh messages and expires after 30 seconds; all captured messages remain in
history even when their compact banner is no longer relevant. Opening the history
shows incoming entries directly rather than replaying their banners afterwards.

Openbar needs a registered package identity, Notifications enabled in Settings →
Content, and access allowed from the island's Notifications page. Desktop
notifications must also be enabled in the source application. Browser-hosted
services follow the browser's Windows notification identity and permissions;
the island does not infer a service from message contents.
The listener is created on Tauri's UI thread; its asynchronous read completes
on the notifications worker. Some sparse-package installs return `E_INVALIDARG`
when the worker calls the listener directly even after Windows grants consent.

## Package identity (notifications and calendar)

Windows hands notifications (`userNotificationListener`) and the appointment
store (`appointments`, the Calendar section) only to an app with a package
identity. Openbar gets one from a **sparse package**: a signed `.msix` with no
files of its own, whose manifest (`packaging/AppxManifest.xml`) names
`Openbar.exe` in the install folder as its external location. The exe's own
manifest (`src-tauri/windows-app.manifest`, `<msix … packageName="Openbar.Island"/>`)
names the package back. No Developer Mode is needed for a signed package.
Keep the app visible in the package manifest so Windows can list it in
notification access settings (`AppListEntry="none"` must not be set).

### Building it

1. On Windows with the Windows SDK, `tools/package-identity/build-sparse.ps1`
   stamps the app version into the manifest, packs it with `makeappx` and signs
   it with `signtool`, using a self-signed `CN=Openbar` code-signing
   certificate. Its subject must equal the manifest's `Publisher`; the script
   checks. The PFX lives outside the repository and is passed through
   `OPENBAR_SIGNING_PFX` and `OPENBAR_SIGNING_PASSWORD`; `-NewCertificate`
   makes it the first time. Keep that PFX: installs that trusted it accept only
   packages it signed. Output: `packaging/out/Openbar.msix` and the public
   `Openbar.cer` (both ignored by git).
2. From the Mac, `tools/package-identity/build-sparse-vm.sh` copies the inputs
   into the build VM, runs the script there over ssh (the password travels on
   stdin) and copies the two files back.
3. `bunx tauri build … --config src-tauri/tauri.identity.conf.json` bundles them,
   with `tools/package-identity/identity.ps1`, under `identity\` in the install
   folder. Without that file the build stays as before and installs no identity.

### What the installer does

The installer runs per user (`installMode: currentUser`). After installing,
`src-tauri/installer-hooks.nsh`:

1. checks whether this PC already trusts the certificate
   (`identity.ps1 -Action Trusted`);
2. if not, starts `identity.ps1 -Action Trust` elevated, which shows **one UAC
   prompt** and adds `Openbar.cer` to `LocalMachine\TrustedPeople` (never to
   the root store); the script refuses any certificate other than a
   `CN=Openbar` code-signing one;
3. registers the package for the installing user:
   `Add-AppxPackage -Path …\identity\Openbar.msix -ExternalLocation <install folder>`.
   A package registered before with the same version (an earlier build or a
   development registration) is removed and registered again.

Each step may fail, for example when the UAC prompt is declined, without
failing the install: the island runs without an identity, and the Notifications
and Calendar pages say that access is unavailable. Failures are written to
`%TEMP%\Openbar-identity.log`. Updating keeps the package; uninstalling
removes it (`Remove-AppxPackage`). The certificate stays trusted after
uninstalling, since removing it would need another UAC prompt; remove it by
hand from `certlm.msc` › Trusted People if wanted.

To check a PC:

```powershell
Get-AppxPackage -Name Openbar.Island | Select-Object Name, Version, InstallLocation, IsDevelopmentMode
Get-ChildItem Cert:\LocalMachine\TrustedPeople | Where-Object Subject -eq 'CN=Openbar'
```

### Development builds

For a local development build on a PC that already has Windows Developer Mode
enabled, register the manifest against the built executable instead:

```powershell
.\tools\register-notifications.ps1 -ExecutablePath .\src-tauri\target\debug\Openbar.exe
```

Restart that executable afterwards, then open Notifications or Calendar and allow
access. The helper does not change Developer Mode or certificate trust. A
development registration made before the `appointments` capability was added
lacks it; register again so Calendar can ask for access.

## Discord status and muted conversations

Discord's **Do Not Disturb** status suppresses its desktop notifications. Because
the island uses those notifications, Discord messages suppressed by DND, disabled
desktop notifications, or a muted conversation do not create island banners.
Online, Idle, and Invisible retain Discord's normal desktop notification behavior.
The island does not log into Discord or read its account token or message history,
and it does not independently query the account's presence.

The Discord card uses a branded icon and a compact reply field. Enter or Send
opens the desktop Discord client, checks that its visible message editor belongs
to the conversation named by the notification, types the reply, and presses
Enter. Shift+Enter inserts a line break. If the conversation cannot be matched,
no send key is pressed and OpenBar keeps the draft for another attempt. OpenBar
does not read Discord account tokens. Windows' basic toast text lacks a channel
identifier, so the matching conversation must already be selected in Discord.
An image preview appears when the notification text includes a direct Discord CDN
attachment URL. Windows' notification listener exposes text and the app icon, but
not image elements from another app's toast, so an attachment omitted from the
notification text cannot be displayed in the island.

See [Discord's status documentation](https://support.discord.com/hc/en-us/articles/227779547-Changing-Online-Status)
and [notification settings](https://support.discord.com/hc/en-us/articles/215253258-Notifications-Settings-101).

The initial scan imports notifications currently in Windows Notification Center
into history without showing old banners. Only new arrivals interrupt the island.
Presentation and inactive-session suppression reported by Windows prevents new
island banners; quiet arrivals can still enter history without being replayed
later. Locking the PC or disabling the section hides the visible inbox and stops
collection, while retaining the saved history. Resuming monitoring primes the
current IDs so old messages never become new banners.

## Manual verification on Windows

1. Enable desktop notifications in Discord, select Online, and receive a direct
   message while another app has focus. Confirm the island shows Discord's icon,
   and the sender. Hover to read the message, then find its app, sender, body and
   timestamp in Notifications. Search by app, sender or message content.
2. Select Do Not Disturb in Discord and receive another direct message. Confirm
   neither a Windows desktop notification nor an island banner appears.
3. Return to Online and receive a new message. Confirm it appears once, without
   replaying messages suppressed during DND. Repeat with Idle and Invisible.
4. Receive a notification from Messenger or another messaging app. Verify its
   app icon/name and the sender/title provided in its Windows notification.
5. Restart Openbar and confirm history is retained without replaying banners.
   Dismiss a history entry, restart again and confirm it stays dismissed. Clear
   All must ask for confirmation. Lock/unlock and disable/re-enable the section:
   history is retained, and old messages are not replayed as new banners.

Automated tests cover app identity/icon fallback, sender/title preservation,
banner/card rendering, initial priming, quiet-mode suppression without replay,
disabled delivery, disk round trips and unreadable-file preservation. Actual Discord status behavior requires
the manual desktop test above; it is supplied by Discord itself.
