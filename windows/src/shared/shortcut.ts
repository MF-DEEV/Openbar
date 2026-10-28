// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Global shortcuts as the backend's shortcut plugin reads them: modifiers,
// then the key's code, joined by "+" ("Ctrl+Alt+Shift+KeyS"). A recorded
// shortcut needs Ctrl, Alt or Win, as the Mac's needs Command, Option or
// Control, so typing into a field can never set one off.

const MODIFIERS = ["Ctrl", "Alt", "Shift", "Super"] as const;

/** Key codes the plugin knows (global-hotkey's parse_key), beyond letters, digits and F-keys. */
const NAMED: Record<string, string> = {
  Backquote: "`",
  Backslash: "\\",
  BracketLeft: "[",
  BracketRight: "]",
  Comma: ",",
  Equal: "=",
  Minus: "-",
  Period: ".",
  Quote: "'",
  Semicolon: ";",
  Slash: "/",
  Space: "Space",
  Tab: "Tab",
  Enter: "Enter",
  Backspace: "Backspace",
  Delete: "Delete",
  Insert: "Insert",
  Home: "Home",
  End: "End",
  PageUp: "Page Up",
  PageDown: "Page Down",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  Pause: "Pause",
  PrintScreen: "Print Screen",
};

function keyLabel(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  if (/^Numpad[0-9]$/.test(code)) return `Num ${code.slice(6)}`;
  return NAMED[code] ?? null;
}

type KeyLike = Pick<KeyboardEvent, "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">;

/**
 * The shortcut a key press makes, or null while only modifiers are down,
 * for a key the plugin can't take, or without Ctrl, Alt or Win.
 */
export function acceleratorFrom(event: KeyLike): string | null {
  if (keyLabel(event.code) === null) return null;
  if (!event.ctrlKey && !event.altKey && !event.metaKey) return null;
  const mods = [
    event.ctrlKey && "Ctrl",
    event.altKey && "Alt",
    event.shiftKey && "Shift",
    event.metaKey && "Super",
  ].filter(Boolean);
  return [...mods, event.code].join("+");
}

/** How a stored shortcut reads: "Ctrl+Alt+Shift+S". */
export function formatAccelerator(accelerator: string): string {
  const parts = accelerator.split("+").filter((p) => p.length > 0);
  const key = parts.at(-1) ?? "";
  const mods = parts
    .slice(0, -1)
    .map((p) => {
      const upper = p.toUpperCase();
      if (upper === "CONTROL" || upper === "CTRL") return "Ctrl";
      if (upper === "OPTION" || upper === "ALT") return "Alt";
      if (upper === "SHIFT") return "Shift";
      return "Win";
    })
    .sort((a, b) => order(a) - order(b));
  return [...mods, keyLabel(key) ?? key].join("+");
}

const order = (label: string) => {
  const i = MODIFIERS.indexOf((label === "Win" ? "Super" : label) as (typeof MODIFIERS)[number]);
  return i < 0 ? MODIFIERS.length : i;
};
