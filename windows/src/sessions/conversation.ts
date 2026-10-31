// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { dateTimeFormat } from "../i18n/index";

import type { ChatMessage } from "./types";

/** Matches NotchClaudeSessionsSupport.answers(in:): only ended turns have summaries. */
export function answerMarks(chat: ChatMessage[]) {
  const answers = new Set<number>();
  const onTheWay = new Set<number>();
  let answered = false;
  for (let i = chat.length - 1; i >= 0; i--) {
    const message = chat[i]!;
    if (message.role === "user") answered = false;
    else if (message.endsTurn) {
      answers.add(message.id);
      answered = true;
    } else if (answered) onTheWay.add(message.id);
  }
  return { answers, onTheWay };
}

export function messageTime(at: number) {
  return dateTimeFormat({ hour: "2-digit", minute: "2-digit" }).format(new Date(at * 1000));
}
