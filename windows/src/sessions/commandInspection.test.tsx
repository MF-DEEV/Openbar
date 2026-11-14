import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, expect, it, vi } from "vitest";
import { bridge } from "../shared/bridge";
import { useIsland } from "../island/store";
import { PermissionCard } from "./PermissionCard";
import { demoSnapshot } from "./demo";
import { useSessions } from "./store";
import { performPermissionKey } from "./permissionKeys";
import { automaticallyAllows } from "./autoAllow";
import { hidesAutoApproval } from "./presentation";
import type { Session } from "./types";
import { S } from "./strings";

vi.mock("./store", async (original) => {
  const actual = await original<typeof import("./store")>();
  const store = actual.useSessions;
  return {
    ...actual,
    useSessions: Object.assign(
      (select: (s: ReturnType<typeof store.getState>) => unknown) => select(store.getState()),
      store,
    ),
  };
});

function request(restricted = true, complete = true, id = "A"): Session {
  const base = demoSnapshot().sessions[0]!;
  return {
    ...base,
    id,
    phase: "waitingPermission",
    permission: {
      ...base.permission!,
      requestId: id,
      detail: restricted ? "preview\n… 100 more characters" : "echo ok",
      questions: [],
      offersAlways: true,
      command: {
        revision: `revision-${id}`,
        complete,
        truncated: restricted,
        remaining: restricted ? 100 : 0,
        requiresInspection: restricted,
        available: complete,
      },
    },
  };
}
function receive(...sessions: Session[]) {
  useSessions.getState().receive({ ...demoSnapshot(), sessions });
  useSessions.setState({
    shownAt: Object.fromEntries(sessions.map((s) => [s.permission!.requestId, Date.now() / 1000 - 2])),
  });
}
const invoke = vi.spyOn(bridge.sessions, "decide");

beforeEach(() => {
  invoke.mockReset().mockResolvedValue(true);
  useSessions.setState({ autoAllow: {}, shownAt: {}, focused: null });
  useIsland.setState({ expanded: true, selected: "sessions", showingSections: false });
});

it("short commands allow Allow, Always and Deny without inspection", async () => {
  const s = request(false);
  receive(s);
  for (const decision of ["allow", "always", "deny"] as const)
    expect(await useSessions.getState().decide(s, decision)).toBe(true);
  expect(invoke).toHaveBeenCalledTimes(3);
  const html = renderToStaticMarkup(<PermissionCard session={s} onOpen={() => {}} />);
  expect(html).not.toContain("disabled=");
  expect(html).not.toContain(S.showCommand);
});

it("collapsed long commands block Allow and Always; Deny stays available", async () => {
  const s = request();
  receive(s);
  expect(await useSessions.getState().decide(s, "allow")).toBe(false);
  expect(await useSessions.getState().decide(s, "always")).toBe(false);
  expect(invoke).not.toHaveBeenCalled();
  const html = renderToStaticMarkup(<PermissionCard session={s} onOpen={() => {}} />);
  expect(html).toContain(S.showCommand);
  expect(html).toContain(S.commandCut);
  expect(html).toContain(S.shortcutsHintInspect);
  expect(html).toMatch(/class="s-button is-allow"[^>]*disabled=""/);
  expect(html).toMatch(/class="s-button is-secondary"[^>]*aria-keyshortcuts="Control\+t"[^>]*disabled=""/);
  expect(html).not.toMatch(/class="s-button is-deny"[^>]*disabled/);
  expect(await useSessions.getState().decide(s, "deny")).toBe(true);
});

it("inspection passes request identity and backend capability, never a frontend unlocked flag", async () => {
  const s = request();
  receive(s);
  expect(await useSessions.getState().decide(s, "allow", undefined, "backend-issued-token")).toBe(true);
  expect(invoke).toHaveBeenLastCalledWith(s.id, "A", "allow", undefined, "revision-A", "backend-issued-token");
  // Backend regressions separately prove forged tokens are rejected by the actual answer boundary.
});

it("missing full content stays blocked even if frontend supplies a token", async () => {
  const s = request(true, false);
  receive(s);
  expect(await useSessions.getState().decide(s, "allow", undefined, "anything")).toBe(false);
  expect(await useSessions.getState().decide(s, "always", undefined, "anything")).toBe(false);
  const html = renderToStaticMarkup(<PermissionCard session={s} onOpen={() => {}} />);
  expect(html).toContain(S.commandIncomplete);
  expect(html).toContain(S.shortcutsHintDenyOnly);
  expect(html).not.toContain(S.showCommand);
  expect(await useSessions.getState().decide(s, "deny")).toBe(true);
});

it("stale UI actions cannot answer replacements, changed commands, expired requests or another session", async () => {
  const a = request();
  const b = request(true, true, "B");
  receive(a, b);
  expect(await useSessions.getState().decide(b, "allow")).toBe(false);
  receive({ ...a, permission: { ...a.permission!, command: { ...a.permission!.command!, revision: "changed" } } }, b);
  expect(await useSessions.getState().decide(a, "allow", undefined, "old-token")).toBe(false);
  receive(b);
  expect(await useSessions.getState().decide(a, "allow", undefined, "old-token")).toBe(false);
  expect(invoke).not.toHaveBeenCalled();
});

it("keyboard and auto-allow cannot bypass a truncated command", async () => {
  const s = { ...request(), provider: "codex" as const };
  receive(s);
  useSessions.setState({ autoAllow: { [s.id]: true } });
  expect(await performPermissionKey(s, "y")).toBe(false);
  expect(await performPermissionKey(s, "always")).toBe(false);
  expect(automaticallyAllows(s)).toBe(false);
  expect(hidesAutoApproval(s, { [s.id]: true })).toBe(false);
  expect(invoke).not.toHaveBeenCalled();
  expect(await performPermissionKey(s, "n")).toBe(true);
});

it("Deny waits the settle moment like every answer, then works for incomplete commands", async () => {
  const s = request(true, false);
  receive(s);
  useSessions.setState({ shownAt: { [s.permission!.requestId]: Date.now() / 1000 } });
  expect(await useSessions.getState().decide(s, "deny")).toBe(false);
  useSessions.setState({ shownAt: { [s.permission!.requestId]: Date.now() / 1000 - 2 } });
  expect(await useSessions.getState().decide(s, "deny")).toBe(true);
});

it("a tool with no arguments is allowed as it stands", async () => {
  const s = request(false);
  s.permission = { ...s.permission!, detail: "", command: { ...s.permission!.command!, remaining: 0 } };
  receive(s);
  expect(await useSessions.getState().decide(s, "allow")).toBe(true);
});
