// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import cases from "../../src-tauri/src/sessions/safe_command_cases.json";
import {
  CHOICES_KEPT,
  CHOICES_KEY,
  cleanedCommand,
  loadChoices,
  offersSafeCommands,
  projectKey,
  safeList,
  saveChoices,
  type SafeEntry,
} from "./newSessionChoices";

function memory(): Storage {
  const data = new Map<string, string>();
  return {
    getItem: (k) => data.get(k) ?? null,
    setItem: (k, v) => void data.set(k, v),
    removeItem: (k) => void data.delete(k),
    clear: () => data.clear(),
    key: (i) => [...data.keys()][i] ?? null,
    get length() {
      return data.size;
    },
  };
}

describe("new session choices (#270)", () => {
  it("keeps each project's choices and drops the least recent past the limit", () => {
    const store = memory();
    saveChoices("C:\\Code\\App\\", { agent: "codex", safeCustom: ["make check"] }, 1, store);
    expect(loadChoices("c:\\code\\app", store)).toMatchObject({ agent: "codex", safeCustom: ["make check"], used: 1 });
    expect(loadChoices("C:\\Code\\Other", store)).toBeNull();
    for (let i = 0; i < CHOICES_KEPT; i++) saveChoices(`/w/p${i}`, { agent: "claude" }, 10 + i, store);
    expect(Object.keys(JSON.parse(store.getItem(CHOICES_KEY)!))).toHaveLength(CHOICES_KEPT);
    expect(loadChoices("C:\\Code\\App", store)).toBeNull();
  });

  it("reads broken storage as nothing", () => {
    const store = memory();
    store.setItem(CHOICES_KEY, "[1,2]");
    expect(loadChoices("/w/a", store)).toBeNull();
    store.setItem(CHOICES_KEY, "{oops");
    expect(loadChoices("/w/a", store)).toBeNull();
  });

  it("keys POSIX paths as they are and Windows paths without case", () => {
    expect(projectKey("/Users/Me/App/")).toBe("/Users/Me/App");
    expect(projectKey("D:\\Work\\API")).toBe("d:\\work\\api");
  });

  it("cleans commands as the shared cases say", () => {
    for (const c of cases.cleaned) expect(cleanedCommand(c.text), c.text).toBe(c.expect);
  });

  it("lists what the session starts with", () => {
    const suggested: SafeEntry[] = [
      { command: "npm run test", kind: "test" },
      { command: "git status", kind: "git" },
      { command: "git log", kind: "git" },
    ];
    expect(safeList(true, "claude", suggested, ["git log"], ["make check"], true)).toEqual([
      "npm run test",
      "git status",
      "make check",
    ]);
    expect(safeList(true, "codex", suggested, [], [], false)).toEqual(["npm run test"]);
    expect(safeList(false, "claude", suggested, [], ["x"], true)).toEqual([]);
    expect(safeList(true, "gemini", suggested, [], ["x"], true)).toEqual([]);
    expect(offersSafeCommands("claude") && offersSafeCommands("codex") && !offersSafeCommands("vibe")).toBe(true);
  });
});
