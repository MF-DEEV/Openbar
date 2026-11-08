// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { expect, it } from "vitest";
import { t } from "../i18n";
import { replyErrorText } from "./reply";

it("translates the codes discord_reply.rs rejects with and falls back for anything else", () => {
  expect(replyErrorText("draftInDiscord")).toBe(t("windows.notifications.replyErrorDraftInDiscord"));
  expect(replyErrorText("typingInterrupted")).toBe(t("windows.notifications.replyErrorTypingInterrupted"));
  expect(replyErrorText("accessibilityOff")).toBe(t("windows.notifications.replyErrorAccessibilityOff"));
  expect(replyErrorText("wrongChat")).toBe(t("windows.notifications.replyErrorWrongChat"));
  expect(replyErrorText("Error: IPC failed")).toBe(t("windows.notifications.replyErrorGeneric"));
  expect(replyErrorText("toString")).toBe(t("windows.notifications.replyErrorGeneric"));
});
