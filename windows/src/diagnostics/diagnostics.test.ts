// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { MODULES } from "../island/modules";
import { resolve } from "../shared/prefs";
import { baseName, features, issueUrl, PARTS, request } from "./diagnostics";

const source = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");

describe("diagnostics export", () => {
  it("lists the parts the backend writes, in its order, with the same switches", () => {
    const report = source("../../src-tauri/src/diagnostics/report.rs");
    const all = report.match(/pub const ALL: \[Part; 6\] = \[([^\]]+)\]/)?.[1] ?? "";
    const backend = [...all.matchAll(/Part::(\w+)/g)].map((m) => m[1].toLowerCase());
    expect(PARTS.map((p) => p.id)).toEqual(backend);
    const optional = report.match(/matches!\(self, ([^)]+)\)/)?.[1] ?? "";
    expect(PARTS.filter((p) => p.optional).map((p) => p.id)).toEqual(
      [...optional.matchAll(/Part::(\w+)/g)].map((m) => m[1].toLowerCase()),
    );
  });

  it("lists the same parts as the Mac app", () => {
    const swift = source("../../../Sources/Openbar/Services/Diagnostics/DiagnosticsReport.swift");
    const cases = swift.match(/case (system, [^\n]+)/)?.[1] ?? "";
    expect(PARTS.map((p) => p.id)).toEqual(cases.split(",").map((c) => c.trim()));
  });

  it("asks only for the optional parts switched on", () => {
    const prefs = resolve(null);
    const body = request(new Set(["system", "events", "logs"]), prefs, "en-US");
    expect(body.parts).toEqual(["events", "logs"]);
    expect(body.language).toBe("en-US");
    expect(body.channel).toBe(prefs.island.betaUpdates ? "beta" : "stable");
    expect(request(new Set(), prefs, "de").parts).toEqual([]);
  });

  it("names every section and switch, and nothing else", () => {
    const prefs = resolve(null);
    const list = features(prefs);
    for (const module of MODULES) expect(list.some((f) => f.id === module.id && f.group === "island")).toBe(true);
    expect(list.find((f) => f.id === "island")?.on).toBe(prefs.island.enabled);
    expect(list.every((f) => typeof f.on === "boolean")).toBe(true);
    // Only switches: no folders, names or shortcuts.
    expect(list.some((f) => f.id === "downloadFolders" || f.id === "shortcut" || f.id === "language")).toBe(false);
  });

  it("fills in the bug form and names the file to drag in", () => {
    const url = new URL(
      issueUrl("bug", { version: "0.4.0", system: "Windows 11 Pro 24H2", linux: false, attachment: "d.zip" }),
    );
    expect(url.pathname).toBe("/MF-DEEV/Openbar/issues/new");
    expect(url.searchParams.get("template")).toBe("bug_report.yml");
    expect(url.searchParams.get("version")).toBe("windows-v0.4.0");
    expect(url.searchParams.get("os_version")).toBe("Windows 11 Pro 24H2");
    expect(url.searchParams.get("platform")).toBe("Windows");
    expect(url.searchParams.get("diagnostics")).toBe("Drag d.zip here to attach it.");
    const linux = new URL(issueUrl("bug", { version: "0.4.0", system: "", linux: true }));
    expect(linux.searchParams.get("version")).toBe("linux-v0.4.0");
    expect(linux.searchParams.has("platform") || linux.searchParams.has("diagnostics")).toBe(false);
    const feature = new URL(issueUrl("feature", { version: "0.4.0", system: "x", linux: false }));
    expect([...feature.searchParams.keys()]).toEqual(["template"]);
    expect(baseName(String.raw`C:\Users\x\Desktop\openbar-diagnostics-20261116-1530.zip`)).toBe(
      "openbar-diagnostics-20261116-1530.zip",
    );
    expect(baseName("/home/x/d.zip")).toBe("d.zip");
  });
});
