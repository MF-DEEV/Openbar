// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// Asks GitHub whether a newer version exists, and installs it on Windows.
// The page does the request through the web view's own network stack, so the
// system's proxy and TLS settings apply; it sends no cookies, no account and
// nothing about this PC. The install itself, with its signature check, is
// updater.rs.

import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { inTauri } from "../shared/bridge";
import { onLinux } from "../shared/platform";
import { loadPrefs } from "../shared/prefs";
import { openRelease } from "./open";
import { FEED_URL, newestRelease, releasePrefix, type Release } from "./releases";

/** The feed is a few dozen releases; a body this large is not it. */
const LIMIT = 1 << 20;
const TIMEOUT = 15_000;

export type UpdateStatus = "idle" | "checking" | "upToDate" | "available" | "downloading" | "installing" | "failed";

interface UpdatesState {
  status: UpdateStatus;
  /** The newest release for this app that is newer than the running build. */
  latest: Release | null;
  /** When the last check ended, in milliseconds. */
  checkedAt: number | null;
  /** What went wrong, short and not meant for translation (an HTTP status or an error's name). */
  failure: string | null;
  /** The download's share so far, 0–1; null while its size is unknown. */
  progress: number | null;
  /** Why the last install stopped: "signature" for a refused download, else a short reason. */
  installError: string | null;
  /** Ends with the status; a check already under way is joined, not repeated. */
  check(): Promise<UpdateStatus>;
  /** Downloads, verifies and installs `latest`; the app closes and comes back. On Linux, opens its page. */
  install(): Promise<void>;
}

class Failure extends Error {}

async function feedUrl(): Promise<string> {
  if (!inTauri) return FEED_URL;
  try {
    return (await invoke<{ feed: string }>("updates_endpoints")).feed;
  } catch {
    return FEED_URL;
  }
}

async function betas(): Promise<boolean> {
  try {
    return (await loadPrefs()).island.betaUpdates;
  } catch {
    return true;
  }
}

async function load(): Promise<unknown> {
  const abort = new AbortController();
  const timer = setTimeout(() => abort.abort(), TIMEOUT);
  try {
    const response = await fetch(await feedUrl(), {
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

export const useUpdates = create<UpdatesState>()((set, get) => ({
  status: "idle",
  latest: null,
  checkedAt: null,
  failure: null,
  progress: null,
  installError: null,
  check() {
    if (running) return running;
    const busy = get().status;
    if (busy === "downloading" || busy === "installing") return Promise.resolve(busy);
    set({ status: "checking" });
    running = (async (): Promise<UpdateStatus> => {
      try {
        const latest = newestRelease(await load(), await getVersion(), releasePrefix(onLinux()), await betas());
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
  async install() {
    const { latest, status } = get();
    if (!latest || status === "downloading" || status === "installing") return;
    // The Linux packages can't replace themselves; their page has the new ones.
    if (!inTauri || onLinux()) {
      openRelease(latest);
      return;
    }
    set({ status: "downloading", progress: null, installError: null });
    const stop = await listen<{ received: number; total: number | null }>("updates://progress", ({ payload }) => {
      const progress = payload.total ? Math.min(1, payload.received / payload.total) : null;
      set(progress === 1 ? { status: "installing", progress } : { progress });
    });
    try {
      // On success the app closes while the installer runs; this never returns.
      await invoke("updates_install", { tag: latest.tag });
    } catch (error) {
      set({ status: "available", progress: null, installError: typeof error === "string" ? error : "error" });
    } finally {
      stop();
    }
  },
}));
