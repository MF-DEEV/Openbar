// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { PreviewCountdown } from "./countdown";

describe("screenshot preview lifetime", () => {
  beforeEach(() => vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "performance"] }));
  afterEach(() => vi.useRealTimers());

  it("expires once after the configured duration", () => {
    const close = vi.fn();
    const timer = new PreviewCountdown(3, close);
    timer.update(false, false);
    vi.advanceTimersByTime(2999);
    expect(close).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    timer.update(false, false);
    vi.advanceTimersByTime(3000);
    expect(close).toHaveBeenCalledTimes(1);
  });

  it("pauses while hovered or busy and resumes with the time remaining", () => {
    const close = vi.fn();
    const timer = new PreviewCountdown(8, close);
    timer.update(false, false);
    vi.advanceTimersByTime(3000);
    timer.update(true, false);
    vi.advanceTimersByTime(60000);
    expect(close).not.toHaveBeenCalled();
    timer.update(false, false);
    vi.advanceTimersByTime(2000);
    timer.update(true, false);
    vi.advanceTimersByTime(60000);
    timer.update(false, false);
    vi.advanceTimersByTime(2999);
    expect(close).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(close).toHaveBeenCalledTimes(1);
  });

  it("protects pinned previews and resumes when unpinned", () => {
    const close = vi.fn();
    const timer = new PreviewCountdown(3, close);
    timer.update(false, false);
    vi.advanceTimersByTime(1000);
    timer.update(false, true);
    timer.update(true, true);
    timer.update(false, true);
    vi.advanceTimersByTime(60000);
    expect(close).not.toHaveBeenCalled();
    timer.update(false, false);
    vi.advanceTimersByTime(2000);
    expect(close).toHaveBeenCalledTimes(1);
  });

  it("never expires persistent previews and cancels timers on disposal", () => {
    const close = vi.fn();
    const persistent = new PreviewCountdown(0, close);
    persistent.update(false, false);
    const transient = new PreviewCountdown(3, close);
    transient.update(false, false);
    transient.dispose();
    transient.update(false, false);
    vi.advanceTimersByTime(3600000);
    expect(close).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("keeps rapid consecutive previews independent", () => {
    const oldClose = vi.fn();
    const nextClose = vi.fn();
    const old = new PreviewCountdown(3, oldClose);
    old.update(false, true);
    const next = new PreviewCountdown(3, nextClose);
    next.update(false, false);
    vi.advanceTimersByTime(3000);
    expect(oldClose).not.toHaveBeenCalled();
    expect(nextClose).toHaveBeenCalledTimes(1);
    old.dispose();
    next.dispose();
  });
});
