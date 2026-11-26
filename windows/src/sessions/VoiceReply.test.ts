// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { afterEach, describe, expect, it, vi } from "vitest";

const open = vi.fn();
vi.mock("../shared/bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../shared/bridge")>();
  return { ...actual, bridge: { ...actual.bridge, voice: { ...actual.bridge.voice, openWindowsSettings: open } } };
});

const { fix } = await import("./VoiceReply");

const LINUX = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko)";

afterEach(() => {
  vi.unstubAllGlobals();
  open.mockClear();
});

describe("the microphone problem's Open Settings (#190)", () => {
  it("opens Windows' privacy switch or sound page", () => {
    fix("microphoneDenied")?.();
    fix("noMicrophone")?.();
    expect(open.mock.calls).toEqual([["microphone"], ["sound"]]);
  });

  it("opens the desktop's sound settings on Linux, which has no privacy switch", () => {
    vi.stubGlobal("navigator", { userAgent: LINUX });
    fix("noMicrophone")?.();
    fix("microphoneDenied")?.();
    expect(open.mock.calls).toEqual([["sound"], ["sound"]]);
    expect(fix("modelMissing")).not.toBeNull();
    expect(fix("busy")).toBeNull();
  });
});
