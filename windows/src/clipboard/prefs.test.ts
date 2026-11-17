// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { resolve } from "../shared/prefs";

describe("clipboard preferences", () => {
  it("start with the Mac's defaults: history off, 50 entries, kept forever", () => {
    const { island } = resolve({});
    expect(island.clipboardHistoryEnabled).toBe(false);
    expect(island.clipboardHistoryLimit).toBe(50);
    expect(island.clipboardHistoryRetentionDays).toBe(0);
    expect(island.clipboardHistoryIncludeImagesFiles).toBe(true);
    expect(island.clipboardFilterSensitiveText).toBe(false);
    expect(island.clipboardIgnoredApps).toEqual([]);
    expect(island.clipboardCardSize).toBe("comfortable");
    expect(island.clipboardNotice).toBe(true);
  });

  it("keep only the Mac's choices and real app names", () => {
    const good = resolve({
      island: {
        clipboardHistoryLimit: 10000,
        clipboardHistoryRetentionDays: 30,
        clipboardCardSize: "compact",
        clipboardIgnoredApps: ["KeePass.exe"],
      },
    }).island;
    expect([good.clipboardHistoryLimit, good.clipboardHistoryRetentionDays, good.clipboardCardSize]).toEqual([
      10000,
      30,
      "compact",
    ]);
    const odd = resolve({
      island: {
        clipboardHistoryLimit: 33,
        clipboardHistoryRetentionDays: 2,
        clipboardCardSize: "huge" as "compact",
        clipboardIgnoredApps: ["KeePass.exe", "", 4 as unknown as string, "  "],
      },
    }).island;
    expect(odd.clipboardHistoryLimit).toBe(50);
    expect(odd.clipboardHistoryRetentionDays).toBe(0);
    expect(odd.clipboardCardSize).toBe("comfortable");
    expect(odd.clipboardIgnoredApps).toEqual(["KeePass.exe"]);
  });
});
