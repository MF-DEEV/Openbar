# App notifications in the Windows island

The island mirrors new Windows desktop notifications from Discord, Messenger,
and other applications. The compact banner shows the source app's icon and name,
and the sender/title supplied by Windows. Message bodies are not collected or
displayed, including in the expanded Notifications section, tooltips and accessible
labels. The source application's
native icon is preferred; Discord and Messenger have bundled fallback icons.

Vorssaint needs a registered package identity, Notifications enabled in Settings →
Content, and access allowed from the island's Notifications page. The current
unsigned NSIS installer does **not** carry the signed identity package, so simply
reinstalling that installer will not enable this feature. Desktop
notifications must also be enabled in the source application. Browser-hosted
services follow the browser's Windows notification identity and permissions;
the island does not infer a service from message contents.

For a local development build on a PC that already has Windows Developer Mode
enabled, register the existing sparse manifest against the built executable:

```powershell
.\tools\register-notifications.ps1 -ExecutablePath .\src-tauri\target\debug\Vorssaint.exe
```

Restart that executable afterwards, open Notifications, and allow access. The
helper does not change Developer Mode or certificate trust. Shipping this to
regular users still requires a signed identity package in the installer.

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
