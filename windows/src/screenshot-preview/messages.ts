// SPDX-License-Identifier: GPL-3.0-or-later

// Backend result strings are protocol values, never QR/user text. Resolve at
// render time so an already open screenshot preview switches language too.

import { t, tf, type StringKey } from "../i18n";

import keys from "./message-keys.json";
export type PreviewStatus = string | { key: StringKey };
export function statusText(status: PreviewStatus): string {
  if (typeof status !== "string") return t(status.key);
  const key = (keys as Record<string, StringKey>)[status];
  if (key) return t(key);
  if (status.startsWith("Saved to ")) return tf("screenshot.savedHUDFormat", status.slice(9));
  for (const [pattern, target] of Object.entries(keys)) {
    if (pattern.endsWith("%@") && status.startsWith(pattern.slice(0, -2))) {
      return tf(target as StringKey, status.slice(pattern.length - 2));
    }
  }
  return status;
}
