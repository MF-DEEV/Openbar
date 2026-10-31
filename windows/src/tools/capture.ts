// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { tf } from "../i18n/index";

// The capture tools from the island's side: collapse, wait for the window
// to settle (the island must not be in the frozen picture, NotchService
// perform), run the selection, then say what happened in a notice, as the
// Mac's HUD does (ScreenshotService, ScreenTextService, ColorSamplerService).

import { type Notice, useIsland } from "../island/store";
import { reactMascot } from "../mascot/service";
import { bridge } from "../shared/bridge";
import { S } from "./strings";
import type { CaptureOutcome, CapturePurpose } from "./types";

/** The island's collapse spring, with room for the window to repaint. */
const SETTLE_MS = 350;

let running = false;

/** "Screenshot 2026-10-27 140322", Windows' own naming, in local time. */
export function screenshotName(date: Date): string {
  const two = (n: number) => String(n).padStart(2, "0");
  const day = `${date.getFullYear()}-${two(date.getMonth() + 1)}-${two(date.getDate())}`;
  const time = `${two(date.getHours())}${two(date.getMinutes())}${two(date.getSeconds())}`;
  return `${S.screenshot} ${day} ${time}`;
}

/** The folder's own name ("Screenshots"), not its whole path. */
const folderName = (path: string | null) => path?.split(/[\\/]/).filter(Boolean).pop() ?? "";

/** A small square of the picked colour for the notice's picture slot. */
export function swatch(hex: string): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22"><rect width="22" height="22" rx="6" fill="${hex}" stroke="rgba(255,255,255,0.3)"/></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

export function noticeFor(outcome: CaptureOutcome): Notice | null {
  const base = { id: `capture:${Date.now()}`, kind: "capture", priority: 1, duration: 3 } as const;
  switch (outcome.kind) {
    case "cancelled":
      return null;
    case "screenshot": {
      if (outcome.preview) return null;
      const folder = folderName(outcome.folder);
      const saved = outcome.path !== null;
      const detail =
        saved && outcome.copied ? S.savedAndCopied(folder) : saved ? S.savedTo(folder) : S.screenshotCopied;
      return {
        ...base,
        title: S.screenshot,
        detail: outcome.previewError ? tf("windows.tools.detail", detail, outcome.previewError) : detail,
        image: outcome.thumbnail ?? undefined,
      };
    }
    case "text":
      return { ...base, title: S.textCopied, detail: outcome.text.split("\n")[0]!.slice(0, 60) };
    case "noText":
      return { ...base, title: S.noText };
    case "noLanguage":
      return { ...base, title: S.noLanguage, detail: S.noLanguageDetail, duration: 5 };
    case "color":
      return { ...base, title: outcome.value, detail: S.colorCopied, image: swatch(outcome.hex) };
    case "failed":
      return { ...base, title: S.captureFailed };
  }
}

/** One capture at a time; a second click while one runs does nothing. */
export async function runCapture(purpose: CapturePurpose) {
  if (running) return;
  running = true;
  try {
    const island = useIsland.getState();
    if (island.expanded) {
      island.collapse();
      await new Promise((resolve) => setTimeout(resolve, SETTLE_MS));
    }
    let outcome: CaptureOutcome | undefined;
    try {
      outcome = await bridge.capture.run(purpose, screenshotName(new Date()));
    } catch (error) {
      outcome = { kind: "failed", message: String(error) };
    }
    const notice = outcome ? noticeFor(outcome) : null;
    if (notice) useIsland.getState().showNotice(notice);
    if (outcome?.kind === "screenshot") reactMascot("flash", 1);
  } finally {
    running = false;
  }
}
