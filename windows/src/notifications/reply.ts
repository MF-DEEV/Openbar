// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useState } from "react";
import { t } from "../i18n";
import { bridge } from "../shared/bridge";
import { useNotifications } from "./store";

/** Codes notifications_send_discord rejects with (discord_reply.rs); anything else reads as a generic failure. */
const REPLY_ERRORS = {
  noChat: "windows.notifications.replyErrorNoChat",
  tooLong: "windows.notifications.replyErrorTooLong",
  openFailed: "windows.notifications.replyErrorOpenFailed",
  inspectFailed: "windows.notifications.replyErrorInspectFailed",
  accessibilityOff: "windows.notifications.replyErrorAccessibilityOff",
  wrongChat: "windows.notifications.replyErrorWrongChat",
  draftInDiscord: "windows.notifications.replyErrorDraftInDiscord",
  focusLost: "windows.notifications.replyErrorFocusLost",
  typingInterrupted: "windows.notifications.replyErrorTypingInterrupted",
  inputRefused: "windows.notifications.replyErrorInputRefused",
  gone: "windows.notifications.replyErrorGone",
} as const;

export function replyErrorText(code: string): string {
  return Object.hasOwn(REPLY_ERRORS, code)
    ? t(REPLY_ERRORS[code as keyof typeof REPLY_ERRORS])
    : t("windows.notifications.replyErrorGeneric");
}

/** The draft, status and send action shared by the island preview and the inbox card. */
export function useDiscordReply(id: number) {
  const replying = useNotifications((s) => s.replyTo === id);
  const draft = useNotifications((s) => s.replyDrafts[id] ?? "");
  const error = useNotifications((s) => s.replyErrors[id] ?? "");
  const [busy, setBusy] = useState(false);
  const store = useNotifications.getState;

  const send = async () => {
    const text = draft.trim();
    if (!text || busy) return;
    setBusy(true);
    store().setReplyError(id, "");
    try {
      await bridge.notifications.sendDiscord(id, text);
      store().setReplyDraft(id, "");
      useNotifications.setState({ replyTo: null });
    } catch (reason) {
      store().setReplyError(id, String(reason));
    } finally {
      setBusy(false);
    }
  };

  return {
    replying,
    draft,
    busy,
    status: error ? replyErrorText(error) : t("windows.notifications.replyHint"),
    setDraft: (text: string) => store().setReplyDraft(id, text),
    toggle: () => useNotifications.setState({ replyTo: replying ? null : id }),
    close: () => useNotifications.setState({ replyTo: null }),
    send,
  };
}
