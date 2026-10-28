// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { acceleratorFrom, formatAccelerator } from "./shortcut";

const press = (code: string, mods: Partial<Record<"ctrlKey" | "altKey" | "shiftKey" | "metaKey", boolean>> = {}) => ({
  code,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...mods,
});

describe("recorded shortcuts", () => {
  it("joins the modifiers and the key's code in the plugin's order", () => {
    expect(acceleratorFrom(press("KeyS", { shiftKey: true, altKey: true, ctrlKey: true }))).toBe("Ctrl+Alt+Shift+KeyS");
    expect(acceleratorFrom(press("F5", { metaKey: true }))).toBe("Super+F5");
    expect(acceleratorFrom(press("Digit2", { altKey: true }))).toBe("Alt+Digit2");
  });

  it("waits while only modifiers are down and refuses keys without Ctrl, Alt or Win", () => {
    expect(acceleratorFrom(press("ControlLeft", { ctrlKey: true }))).toBeNull();
    expect(acceleratorFrom(press("KeyS"))).toBeNull();
    expect(acceleratorFrom(press("KeyS", { shiftKey: true }))).toBeNull();
    expect(acceleratorFrom(press("IntlBackslash", { ctrlKey: true }))).toBeNull();
  });

  it("reads like Windows writes shortcuts", () => {
    expect(formatAccelerator("Ctrl+Alt+Shift+KeyS")).toBe("Ctrl+Alt+Shift+S");
    expect(formatAccelerator("Super+Ctrl+ArrowUp")).toBe("Ctrl+Win+Up");
    expect(formatAccelerator("Alt+Backquote")).toBe("Alt+`");
    expect(formatAccelerator("CONTROL+F12")).toBe("Ctrl+F12");
  });
});
