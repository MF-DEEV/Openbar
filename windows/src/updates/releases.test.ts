// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import { describe, expect, it } from "vitest";
import { newestRelease, parseVersion } from "./releases";

const release = (tag: string, extra: Record<string, unknown> = {}) => ({ tag_name: tag, ...extra });

describe("parseVersion", () => {
  it("reads a version with or without pre-release parts", () => {
    expect(parseVersion("0.3.0")).toEqual({ core: [0, 3, 0], pre: [] });
    expect(parseVersion(" 1.20.3-beta.2 ")).toEqual({ core: [1, 20, 3], pre: ["beta", "2"] });
  });

  it("refuses anything else", () => {
    for (const text of ["", "1.2", "v1.2.3", "1.2.3.4", "1.2.3-", "1.2.3-a..b", "1.2.x", "1e3.0.0", "1.2.3+build"]) {
      expect(parseVersion(text), text).toBeNull();
    }
  });
});

describe("newestRelease", () => {
  const url = (tag: string) => `https://github.com/MF-DEEV/Openbar/releases/tag/${tag}`;

  it("offers the newest windows release that is newer than this build", () => {
    const feed = [release("windows-v0.3.0"), release("windows-v0.5.1"), release("windows-v0.4.9")];
    expect(newestRelease(feed, "0.3.0")).toEqual({ version: "0.5.1", url: url("windows-v0.5.1") });
  });

  it("compares numbers, not text", () => {
    expect(newestRelease([release("windows-v0.10.0")], "0.9.0")?.version).toBe("0.10.0");
    expect(newestRelease([release("windows-v0.9.0")], "0.10.0")).toBeNull();
  });

  it("says nothing when this build is the newest or newer", () => {
    expect(newestRelease([release("windows-v0.3.0")], "0.3.0")).toBeNull();
    expect(newestRelease([release("windows-v0.2.0")], "0.3.0")).toBeNull();
    expect(newestRelease([], "0.3.0")).toBeNull();
  });

  it("leaves out the Mac app's releases, demos, drafts and tags it cannot read", () => {
    const feed = [
      release("v9.0.0"),
      release("claude-sessions-demo"),
      release("windows-v9.0.0", { draft: true }),
      release("windows-v9.1"),
      release("windows-vlatest"),
      release("windows-v1.0.0/../../x"),
      { tag_name: 5 },
      null,
      "windows-v9.9.9",
    ];
    expect(newestRelease(feed, "0.3.0")).toBeNull();
    expect(newestRelease([...feed, release("windows-v0.4.0")], "0.3.0")?.version).toBe("0.4.0");
  });

  it("does not trust the release page address the feed gives", () => {
    const feed = [release("windows-v0.4.0", { html_url: "https://example.com/evil" })];
    expect(newestRelease(feed, "0.3.0")?.url).toBe(url("windows-v0.4.0"));
  });

  it("keeps pre-releases for builds that are pre-releases", () => {
    const feed = [release("windows-v0.4.0-rc.1")];
    expect(newestRelease(feed, "0.3.0")).toBeNull();
    expect(newestRelease(feed, "0.4.0-beta.1")?.version).toBe("0.4.0-rc.1");
    // The final release of the same version beats its candidates, for both builds.
    const both = [release("windows-v0.4.0-rc.1"), release("windows-v0.4.0")];
    expect(newestRelease(both, "0.3.0")?.version).toBe("0.4.0");
    expect(newestRelease(both, "0.4.0-beta.1")?.version).toBe("0.4.0");
    // A build that is a pre-release of a version is not offered its own final release's older candidates.
    expect(newestRelease([release("windows-v0.4.0-beta.1")], "0.4.0-beta.2")).toBeNull();
  });

  it("orders pre-release parts the way Semantic Versioning does", () => {
    const order = [
      "1.0.0-alpha",
      "1.0.0-alpha.1",
      "1.0.0-alpha.beta",
      "1.0.0-beta",
      "1.0.0-beta.2",
      "1.0.0-beta.11",
      "1.0.0-rc.1",
      "1.0.0",
    ];
    for (let i = 1; i < order.length; i++) {
      const newer = newestRelease([release(`windows-v${order[i]}`)], order[i - 1]!);
      expect(newer?.version, `${order[i]} after ${order[i - 1]}`).toBe(order[i]);
      expect(
        newestRelease([release(`windows-v${order[i - 1]}`)], order[i]!),
        `${order[i - 1]} not after ${order[i]}`,
      ).toBeNull();
    }
  });

  it("gives nothing for a feed that is not a list or a build with no version", () => {
    expect(newestRelease({ message: "Not Found" }, "0.3.0")).toBeNull();
    expect(newestRelease([release("windows-v0.4.0")], "dev")).toBeNull();
  });
});
