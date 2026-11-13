// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// Asks GitHub whether a newer version exists. The page does the request
// through the web view's own network stack, so the system's proxy and TLS
// settings apply; it sends no cookies, no account and nothing about this PC.

import { getVersion } from "@tauri-apps/api/app";
import { create } from "zustand";
import { onLinux } from "../shared/platform";
import { FEED_URL, newestRelease, releasePrefix, type Release } from "./releases";

/** The feed is a few dozen releases; a body this large is not it. */
const LIMIT = 1 << 20;
const TIMEOUT = 15_000;

export type UpdateStatus = "idle" | "checking" | "upToDate" | "available" | "failed";

interface UpdatesState {
  status: UpdateStatus;
  /** The newest release for this app that is newer than the running build. */
  latest: Release | null;
  /** When the last check ended, in milliseconds. */
  checkedAt: number | null;
  /** What went wrong, short and not meant for translation (an HTTP status or an error's name). */
  failure: string | null;
  /** Ends with the status; a check already under way is joined, not repeated. */
  check(): Promise<UpdateStatus>;
}

class Failure extends Error {}

async function load(): Promise<unknown> {
  const abort = new AbortController();
  const timer = setTimeout(() => abort.abort(), TIMEOUT);
  try {
    const response = await fetch(FEED_URL, {
      signal: abort.signal,
      cache: "no-store",
      credentials: "omit",
      redirect: "error",
      referrerPolicy: "no-referrer",
    });
    if (response.status !== 200) throw new Failure(`HTTP ${response.status}`);
    if (Number(response.headers.get("content-length") ?? 0) > LIMIT) throw new Failure("too large");
    const text = await response.text();
    if (text.length > LIMIT) throw new Failure("too large");
    return JSON.parse(text);
  } finally {
    clearTimeout(timer);
  }
}

function reason(error: unknown): string {
  if (error instanceof Failure) return error.message;
  return error instanceof Error ? error.name : "error";
}

let running: Promise<UpdateStatus> | null = null;

export const useUpdates = create<UpdatesState>()((set) => ({
  status: "idle",
  latest: null,
  checkedAt: null,
  failure: null,
  check() {
    if (running) return running;
    set({ status: "checking" });
    running = (async (): Promise<UpdateStatus> => {
      try {
        const latest = newestRelease(await load(), await getVersion(), releasePrefix(onLinux()));
        const status = latest ? "available" : "upToDate";
        set({ status, latest, checkedAt: Date.now(), failure: null });
        return status;
      } catch (error) {
        set({ status: "failed", checkedAt: Date.now(), failure: reason(error) });
        return "failed";
      } finally {
        running = null;
      }
    })();
    return running;
  },
}));
