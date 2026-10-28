# App notifications in the Windows island

The island mirrors new Windows desktop notifications from Discord, Messenger,
and other applications. The compact banner shows the source app's icon and name,
and the sender/title supplied by Windows. Message bodies are not collected or
displayed, including in the expanded Notifications section, tooltips and accessible
labels. The source application's
native icon is preferred; Discord and Messenger have bundled fallback icons.

Vorssaint needs a registered package identity, Notifications enabled in Settings →
Content, and access allowed from the island's Notifications page. Desktop
notifications must also be enabled in the source application. Browser-hosted
services follow the browser's Windows notification identity and permissions;
the island does not infer a service from message contents.

## Package identity (notifications and calendar)

Windows hands notifications (`userNotificationListener`) and the appointment
store (`appointments`, the Calendar section) only to an app with a package
identity. Vorssaint gets one from a **sparse package**: a signed `.msix` with no
files of its own, whose manifest (`packaging/AppxManifest.xml`) names
`Vorssaint.exe` in the install folder as its external location. The exe's own
manifest (`src-tauri/windows-app.manifest`, `<msix … packageName="Vorssaint.Island"/>`)
names the package back. No Developer Mode is needed for a signed package.

### Building it

1. On Windows with the Windows SDK, `tools/package-identity/build-sparse.ps1`
   stamps the app version into the manifest, packs it with `makeappx` and signs
   it with `signtool`, using a self-signed `CN=Vorssaint` code-signing
   certificate. Its subject must equal the manifest's `Publisher`; the script
   checks. The PFX lives outside the repository and is passed through
   `VORSSAINT_SIGNING_PFX` and `VORSSAINT_SIGNING_PASSWORD`; `-NewCertificate`
   makes it the first time. Keep that PFX: installs that trusted it accept only
   packages it signed. Output: `packaging/out/Vorssaint.msix` and the public
   `Vorssaint.cer` (both ignored by git).
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
   prompt** and adds `Vorssaint.cer` to `LocalMachine\TrustedPeople` (never to
   the root store); the script refuses any certificate other than a
   `CN=Vorssaint` code-signing one;
3. registers the package for the installing user:
   `Add-AppxPackage -Path …\identity\Vorssaint.msix -ExternalLocation <install folder>`.
   A package registered before with the same version (an earlier build or a
   development registration) is removed and registered again.

Each step may fail, for example when the UAC prompt is declined, without
failing the install: the island runs without an identity, and the Notifications
and Calendar pages say that access is unavailable. Failures are written to
`%TEMP%\Vorssaint-identity.log`. Updating keeps the package; uninstalling
removes it (`Remove-AppxPackage`). The certificate stays trusted after
uninstalling, since removing it would need another UAC prompt; remove it by
hand from `certlm.msc` › Trusted People if wanted.

To check a PC:

```powershell
Get-AppxPackage -Name Vorssaint.Island | Select-Object Name, Version, InstallLocation, IsDevelopmentMode
Get-ChildItem Cert:\LocalMachine\TrustedPeople | Where-Object Subject -eq 'CN=Vorssaint'
```

### Development builds

For a local development build on a PC that already has Windows Developer Mode
enabled, register the manifest against the built executable instead:

```powershell
.\tools\register-notifications.ps1 -ExecutablePath .\src-tauri\target\debug\Vorssaint.exe
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

See [Discord's status documentation](https://support.discord.com/hc/en-us/articles/227779547-Changing-Online-Status)
and [notification settings](https://support.discord.com/hc/en-us/articles/215253258-Notifications-Settings-101).

Only notifications arriving after monitoring starts are mirrored. Presentation
and inactive-session suppression reported by Windows prevents new island
notifications, and those arrivals are consumed instead of replayed later.
Notification metadata remains in memory. Locking the PC, disabling the island/Notifications
section, or losing notification permission clears the inbox. Resuming monitoring
primes the current Windows notification IDs so old messages are not imported.

## Manual verification on Windows

1. Enable desktop notifications in Discord, select Online, and receive a direct
   message while another app has focus. Confirm the island shows Discord's icon,
   and the sender. Confirm the message text is absent from both the banner and
   the expanded Notifications page, including tooltips.
2. Select Do Not Disturb in Discord and receive another direct message. Confirm
   neither a Windows desktop notification nor an island banner appears.
3. Return to Online and receive a new message. Confirm it appears once, without
   replaying messages suppressed during DND. Repeat with Idle and Invisible.
4. Receive a notification from Messenger or another messaging app. Verify its
   app icon/name and the sender/title provided in its Windows notification.
5. Disable Notifications in Vorssaint, then re-enable it. Confirm the inbox clears
   and no notifications from the disabled interval are imported. Repeat across
   locking and unlocking Windows.

Automated tests cover app identity/icon fallback, sender/title preservation,
banner/card rendering, initial priming, quiet-mode suppression without replay,
disabled delivery, and inbox clearing. Actual Discord status behavior requires
the manual desktop test above; it is supplied by Discord itself.
