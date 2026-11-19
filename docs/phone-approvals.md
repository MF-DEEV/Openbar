# Phone approvals through Telegram (#214)

When you step away from the computer, a session that asks for permission just
waits until you come back. Phone approvals send that request to your phone
through a Telegram bot you make yourself, with the same buttons the island
has. A tap answers it on the computer, and the session goes on.

It is the one Openbar feature that sends data off your computer, so it is off
until you set it up, and Settings says what is sent and to whom before you
start. On Mac and Windows; not on Linux yet.

## Setting it up

Settings → Dynamic Island → Content → Sessions → Phone approvals (Telegram):

1. In Telegram, open [BotFather](https://t.me/BotFather) and send `/newbot`.
   Give the bot a name. BotFather replies with a token.
2. Paste the token into Settings and press **Use token**. Openbar checks it
   with Telegram and keeps it in the keychain on the Mac, or in Credential
   Manager on Windows. It never goes into a settings file.
3. Openbar shows a one-time code and a QR code. Scan the QR code with your
   phone, or send the code to your bot. The chat that sends the right code is
   linked, and the bot says so.

The code works for ten minutes. After five wrong codes it stops working and
you make a new one. Use a separate bot on each computer: Telegram lets only
one program read a bot's messages at a time.

## When messages go out

- When you're away: no keyboard or mouse input for the minutes you set (five
  by default), or the screen locked. Sitting at the computer, nothing goes to
  the phone. A request that was already waiting when you left goes out once
  you count as away.
- Always, if you turn on **Send even while I'm at the computer**.
- During **Quiet hours** messages still arrive, just without a sound.
- Requests the island answers by itself (auto-mode sessions, Always proceed,
  Codex's Always for the turn, safe commands) never reach the phone.

## What a message holds

- The agent, the project folder's name and the session's name.
- The request: the command (the island's view of it, with hidden characters
  shown as `⟦U+…⟧`), up to about 3,500 characters with a note saying how many
  more are on the computer; the files a change touches with up to 40 diff
  lines; a question with its options; or the plan.
- With **Don't send code** on, only the agent, the project, the session's name
  and the kind of request ("wants to run a command", "asks you a question").
  Allow, Always and Deny still work. A question then has only Skip, and you
  answer it on the computer.
- With **Tell me when a session finishes or stops on an error** on (the
  default), a message when a turn ends, with the session's last reply when
  content previews are on and Don't send code is off.

## Answering

- **Allow**, **Deny**, and **Always** where the island offers it (Claude
  Code's rules for next time; Codex's Always allows the rest of the turn, as
  the island's button does).
- A plan: **Keep planning**, or **Approve** into the mode the island puts
  first, or another mode it offers.
- A question: tap an option. For several questions, or several answers, tap
  each and then **Send**. To write your own answer, reply to the message.
- A reply typed to the bot goes into the session like one typed in the island.
  Reply to a session's message so it goes there; a plain message goes to the
  only session that can take one.

Once answered, the message says how and when ("Allowed from your phone ·
14:02") and loses its buttons. If you answered on the computer first, the
message says so and its buttons stop working. A request that changed since
the message was sent is never answered by its old buttons.

Every answer from the phone goes the island's way, marked as coming from the
phone. A denial tells Claude Code "Denied from the phone, through Openbar.", so
the session's own history shows it, and the session's page in the island lists
it under **From your phone**.

## Safety

- Only the Telegram account that sent the code, in its private chat with the
  bot, is listened to. Messages and taps from anyone else, or from a group,
  get no answer at all.
- Each button carries a number for this run of Openbar. A tap on a message
  sent before Openbar restarted does nothing. Taps and texts sent while
  Openbar wasn't listening (closed, or phone approvals switched off) are
  dropped when it starts listening again, so they never act later.
- A phone answer must match the request as its message showed it: if the
  command changed, the old buttons don't answer the new one.
- On the island, a command too long to show whole, or holding hidden
  characters, can only be allowed after opening its full view. From the phone
  it can be allowed from the preview the message shows, which you chose when
  you linked it. A command Openbar couldn't read in full is never allowed from
  either.
- **Unlink** in Settings stops sending and reading at once and deletes the
  token. The bot stays in Telegram until you delete it in BotFather.

## Code

| | Mac | Windows |
| --- | --- | --- |
| Settings | `UI/Settings/PhoneApprovalsSettings.swift` | `windows/src/settings/PhoneApprovals.tsx` |
| Messages, buttons, away, codes | `Services/PhoneApprovals/PhoneApprovalsSupport.swift` | `windows/src-tauri/src/phone/messages.rs` |
| Bridge (updates, sending, answers) | `Services/PhoneApprovals/TelegramBridge.swift` | `windows/src-tauri/src/phone/bridge.rs` |
| Bot API | `Services/PhoneApprovals/TelegramAPI.swift` | `windows/src-tauri/src/phone/telegram.rs` |
| Token | `Services/PhoneApprovals/KeychainSecret.swift` | `windows/src-tauri/src/phone/secret.rs` |

Both platforms check their messages against the same cases,
`windows/src-tauri/src/phone/phone_cases.json`.

## Testing without Telegram

`OPENBAR_TELEGRAM_API` points both apps at a stand-in Bot API (for example
`http://127.0.0.1:8793`). Developer builds on the Mac also read a stand-in
bot's token from `OPENBAR_TELEGRAM_TEST_TOKEN`, so a test copy never touches
the keychain. The end-to-end runs used real Claude Code and Codex against
local model stand-ins, never a real account.
