// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(async () => "0.3.0") }));

import { getVersion } from "@tauri-apps/api/app";
import { FEED_URL } from "./releases";
import { useUpdates } from "./store";

const initial = useUpdates.getState();
const feed = (...tags: string[]) => tags.map((tag_name) => ({ tag_name }));

function answer(body: unknown, init: { status?: number; headers?: Record<string, string> } = {}) {
  const text = typeof body === "string" ? body : JSON.stringify(body);
  return vi.fn(async () => new Response(text, { status: init.status ?? 200, headers: init.headers }));
}

beforeEach(() => {
  useUpdates.setState(initial, true);
  vi.mocked(getVersion).mockResolvedValue("0.3.0");
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("check", () => {
  it("looks for the Linux packages' releases in WebKitGTK on Linux", async () => {
    vi.stubGlobal("navigator", {
      userAgent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko)",
    });
    vi.stubGlobal("fetch", answer(feed("windows-v0.5.0", "linux-v0.4.0")));
    expect(await useUpdates.getState().check()).toBe("available");
    expect(useUpdates.getState().latest).toEqual({
      version: "0.4.0",
      url: "https://github.com/MF-DEEV/Openbar/releases/tag/linux-v0.4.0",
    });
  });

  it("finds a newer windows release", async () => {
    const fetch = answer(feed("v3.5.0", "windows-v0.3.0", "windows-v0.4.0"));
    vi.stubGlobal("fetch", fetch);
    expect(await useUpdates.getState().check()).toBe("available");
    expect(useUpdates.getState()).toMatchObject({
      status: "available",
      latest: { version: "0.4.0", url: "https://github.com/MF-DEEV/Openbar/releases/tag/windows-v0.4.0" },
      failure: null,
    });
    expect(useUpdates.getState().checkedAt).toBeGreaterThan(0);
    // Nothing of the person goes with it: no cookies, no cache, no referrer, no redirects.
    expect(fetch).toHaveBeenCalledWith(
      FEED_URL,
      expect.objectContaining({
        credentials: "omit",
        cache: "no-store",
        redirect: "error",
        referrerPolicy: "no-referrer",
      }),
    );
  });

  it("says when this build is the newest", async () => {
    vi.stubGlobal("fetch", answer(feed("windows-v0.3.0", "windows-v0.2.0")));
    expect(await useUpdates.getState().check()).toBe("upToDate");
    expect(useUpdates.getState()).toMatchObject({ status: "upToDate", latest: null });
  });

  it("joins a check that is already under way", async () => {
    const fetch = answer(feed("windows-v0.4.0"));
    vi.stubGlobal("fetch", fetch);
    const [first, second] = [useUpdates.getState().check(), useUpdates.getState().check()];
    expect(useUpdates.getState().status).toBe("checking");
    expect(await Promise.all([first, second])).toEqual(["available", "available"]);
    expect(fetch).toHaveBeenCalledTimes(1);
    await useUpdates.getState().check();
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("fails with a short reason, and works again afterwards", async () => {
    vi.stubGlobal("fetch", answer({ message: "rate limited" }, { status: 403 }));
    expect(await useUpdates.getState().check()).toBe("failed");
    expect(useUpdates.getState()).toMatchObject({ status: "failed", failure: "HTTP 403" });

    vi.stubGlobal(
      "fetch",
      vi.fn(async () => Promise.reject(new TypeError("Load failed"))),
    );
    await useUpdates.getState().check();
    expect(useUpdates.getState().failure).toBe("TypeError");

    vi.stubGlobal("fetch", answer("<html>not json</html>"));
    await useUpdates.getState().check();
    expect(useUpdates.getState().failure).toBe("SyntaxError");

    vi.stubGlobal("fetch", answer(feed("windows-v0.4.0")));
    expect(await useUpdates.getState().check()).toBe("available");
    expect(useUpdates.getState().failure).toBeNull();
  });

  it("keeps a known update through a failed check", async () => {
    vi.stubGlobal("fetch", answer(feed("windows-v0.4.0")));
    await useUpdates.getState().check();
    vi.stubGlobal("fetch", answer("", { status: 500 }));
    await useUpdates.getState().check();
    expect(useUpdates.getState()).toMatchObject({ status: "failed", latest: { version: "0.4.0" } });
  });

  it("refuses an answer that is not a plain 200 or far too large", async () => {
    vi.stubGlobal("fetch", answer(feed("windows-v0.4.0"), { status: 202 }));
    await useUpdates.getState().check();
    expect(useUpdates.getState().failure).toBe("HTTP 202");

    vi.stubGlobal("fetch", answer(feed("windows-v0.4.0"), { headers: { "content-length": String(5 << 20) } }));
    await useUpdates.getState().check();
    expect(useUpdates.getState().failure).toBe("too large");

    vi.stubGlobal("fetch", answer(" ".repeat((1 << 20) + 1) + "[]"));
    await useUpdates.getState().check();
    expect(useUpdates.getState().failure).toBe("too large");
    expect(useUpdates.getState().latest).toBeNull();
  });

  it("gives up on a server that does not answer", async () => {
    vi.useFakeTimers();
    vi.stubGlobal(
      "fetch",
      vi.fn(
        (_url: string, init: RequestInit) =>
          new Promise((_resolve, reject) =>
            init.signal?.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError"))),
          ),
      ),
    );
    const done = useUpdates.getState().check();
    await vi.advanceTimersByTimeAsync(14_000);
    expect(useUpdates.getState().status).toBe("checking");
    await vi.advanceTimersByTimeAsync(2_000);
    expect(await done).toBe("failed");
    expect(useUpdates.getState().failure).toBe("AbortError");
  });

  it("fails when the app cannot tell its own version", async () => {
    vi.stubGlobal("fetch", answer(feed("windows-v0.4.0")));
    vi.mocked(getVersion).mockRejectedValue(new Error("not in a Tauri window"));
    expect(await useUpdates.getState().check()).toBe("failed");
    expect(useUpdates.getState().latest).toBeNull();
  });
});
