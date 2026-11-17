// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { expect, it } from "vitest";
import { answerMarks } from "./conversation";
import type { ChatMessage } from "./types";

const message = (id: number, role: ChatMessage["role"], endsTurn = false): ChatMessage => ({
  id,
  role,
  endsTurn,
  text: "text",
  at: id,
});

it("marks final answers and dims only commentary from completed turns", () => {
  const marks = answerMarks([
    message(1, "user"),
    message(2, "assistant"),
    message(3, "assistant", true),
    message(4, "user"),
    message(5, "assistant"),
  ]);
  expect([...marks.answers]).toEqual([3]);
  expect([...marks.onTheWay]).toEqual([2]);
});

it("keeps summaries from turns that began without a user prompt", () => {
  const marks = answerMarks([message(1, "assistant"), message(2, "assistant", true), message(3, "assistant", true)]);
  expect([...marks.answers].sort()).toEqual([2, 3]);
  expect([...marks.onTheWay]).toEqual([1]);
});
