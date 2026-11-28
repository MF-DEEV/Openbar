// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors
import { describe, expect, it } from "vitest";
import { restartDone } from "./jetbrains";

describe("JetBrains restart notes", () => {
  it("stays while a process it waited on runs", () => {
    expect(restartDone([400, 401], [401, 900])).toBe(false);
  });
  it("goes once the IDE restarted or quit", () => {
    expect(restartDone([400, 401], [900])).toBe(true);
    expect(restartDone([400], [])).toBe(true);
  });
  it("leaves notes that wait on nothing, such as failures", () => {
    expect(restartDone(undefined, [])).toBe(false);
  });
});
