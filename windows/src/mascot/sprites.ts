// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Openbar

// The companion's sprites, shared with the Mac app: Resources/Companion holds
// them, rendered in Blender by Tools/companion. Vite gives each its own URL,
// so the island loads only those a look shows.

const files = import.meta.glob("../../../Resources/Companion/*.webp", {
  eager: true,
  query: "?url",
  import: "default",
}) as Record<string, string>;

const urls = new Map<string, string>();
for (const [path, url] of Object.entries(files)) {
  const name = path.slice(path.lastIndexOf("/") + 1, -".webp".length);
  urls.set(name, url);
}

/** The URL of the sprite `name` (as model.ts names them), or null when there is none. */
export const spriteUrl = (name: string | null): string | null => (name ? (urls.get(name) ?? null) : null);

/** Every sprite name there is, for the tests. */
export const spriteNames = () => [...urls.keys()];
