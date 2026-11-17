// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { afterEach, expect, it, vi } from "vitest";
import { onLinux, voiceHeard } from "./platform";

const LINUX = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko)";
const WINDOWS =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36 Edg/140.0.0.0";

afterEach(() => vi.unstubAllGlobals());

it("tells WebKitGTK on Linux from WebView2 on Windows by the agent", () => {
  vi.stubGlobal("navigator", { userAgent: LINUX });
  expect(onLinux()).toBe(true);
  expect(voiceHeard()).toBe(false);
  vi.stubGlobal("navigator", { userAgent: WINDOWS });
  expect(onLinux()).toBe(false);
  expect(voiceHeard()).toBe(true);
});

it("takes a page with no navigator for no Linux", () => {
  vi.stubGlobal("navigator", undefined);
  expect(onLinux()).toBe(false);
  expect(voiceHeard()).toBe(true);
});
