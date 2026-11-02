// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import { afterEach, expect, it, vi } from "vitest";
import { rememberPlace, restoredPlace } from "./viewPrefs";
afterEach(() => vi.unstubAllGlobals());
it("restores the month, selected day and view after reopening", () => {
  let saved: string | null = null;
  vi.stubGlobal("localStorage", {
    setItem: (_: string, value: string) => {
      saved = value;
    },
    getItem: () => saved,
  });
  const place = {
    focus: new Date(2027, 4, 1).getTime(),
    selectedDay: new Date(2027, 4, 16).getTime(),
    showingMonth: true,
  };
  rememberPlace(place);
  expect(restoredPlace()).toEqual(place);
  saved = '{"focus":1e99,"selectedDay":null,"showingMonth":true}';
  expect(restoredPlace(123)).toEqual({ focus: 123, selectedDay: null, showingMonth: true });
});
it("does not interfere with the calendar when browser storage is unavailable", () => {
  vi.stubGlobal("localStorage", {
    getItem: () => {
      throw Error("blocked");
    },
    setItem: () => {
      throw Error("full");
    },
  });
  expect(() => rememberPlace({ focus: 123, selectedDay: null, showingMonth: true })).not.toThrow();
  expect(restoredPlace(123).focus).toBe(123);
});
