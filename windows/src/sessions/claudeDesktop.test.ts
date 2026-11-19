// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { DEFAULT_SESSION_PREFS } from "../shared/prefs";
import { S } from "./strings";

describe("Claude Desktop in Sessions (#246)", () => {
  it("answers a Code tab or Cowork session's request in the app", () => {
    expect(S.answerInSession({ provider: "claude", origin: "cowork" })).toBe("Answer this one in Claude Desktop.");
    expect(S.answerInSession({ provider: "claude", origin: "desktop" })).toBe("Answer this one in Claude Desktop.");
    expect(S.answerInSession({ provider: "claude" })).toBe(S.answerInTerminal);
    expect(S.answerInSession({ provider: "gemini" })).toBe(S.answerIn("gemini"));
  });

  it("labels the rows with the app's two kinds of session", () => {
    expect(S.originMark("desktop")).toBe("Desktop");
    expect(S.originMark("cowork")).toBe("Cowork");
    expect(S.originHelp("cowork")).toContain("Cowork");
  });

  it("lists the Code tab by default and reads Cowork only once turned on", () => {
    expect(DEFAULT_SESSION_PREFS.claudeDesktop).toBe(true);
    expect(DEFAULT_SESSION_PREFS.claudeCowork).toBe(false);
  });
});
