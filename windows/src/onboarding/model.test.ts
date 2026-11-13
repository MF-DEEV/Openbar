// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { claudeTile, codexTile, hookAgentTile, shouldConnectClaude, STEPS } from "./model";

describe("first-run agent tiles", () => {
  it("reads Claude Code from its presence and hooks", () => {
    expect(claudeTile(null, null, false)).toBe("checking");
    expect(claudeTile(true, null, false)).toBe("checking");
    expect(claudeTile(true, { installed: false, error: null }, false)).toBe("connecting");
    expect(claudeTile(true, { installed: true, error: null }, false)).toBe("connected");
    expect(claudeTile(false, { installed: false, error: null }, false)).toBe("missing");
    expect(claudeTile(true, { installed: false, error: null }, true)).toBe("failed");
    // Hooks someone put in by hand count even where no claude command is found.
    expect(claudeTile(false, { installed: true, error: null }, false)).toBe("connected");
  });

  it("connects Claude Code once, only when it is here and lacks the hooks", () => {
    expect(shouldConnectClaude(true, { installed: false, error: null }, false)).toBe(true);
    expect(shouldConnectClaude(true, { installed: false, error: null }, true)).toBe(false);
    expect(shouldConnectClaude(true, { installed: true, error: null }, false)).toBe(false);
    expect(shouldConnectClaude(false, { installed: false, error: null }, false)).toBe(false);
    expect(shouldConnectClaude(null, null, false)).toBe(false);
  });

  it("reads Codex from its app-server", () => {
    expect(codexTile(null)).toBe("checking");
    expect(codexTile("connected")).toBe("connected");
    expect(codexTile("notRunning")).toBe("ready");
    expect(codexTile("notFound")).toBe("missing");
  });

  it("reads the hook agents from their hooks", () => {
    expect(hookAgentTile(undefined, false)).toBe("checking");
    expect(hookAgentTile({ agent: "gemini", state: "installed", present: true }, false)).toBe("connected");
    expect(hookAgentTile({ agent: "gemini", state: "missing", present: true }, false)).toBe("connecting");
    expect(hookAgentTile({ agent: "gemini", state: "outdated", present: true }, false)).toBe("connecting");
    expect(hookAgentTile({ agent: "cursor", state: "missing", present: false }, false)).toBe("missing");
    expect(hookAgentTile({ agent: "copilot", state: "unreadable", present: true }, false)).toBe("failed");
    expect(hookAgentTile({ agent: "opencode", state: "missing", present: true, failure: "denied" }, false)).toBe(
      "failed",
    );
    expect(hookAgentTile({ agent: "opencode", state: "missing", present: true }, true)).toBe("failed");
  });

  it("walks the same steps as the Mac, minus what Windows has no use for", () => {
    expect(STEPS).toEqual(["welcome", "agents", "tryIt", "done"]);
  });
});
