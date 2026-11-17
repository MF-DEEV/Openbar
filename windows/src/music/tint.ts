// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The accent a cover lends the player (NotchArtworkTint.from): its average
// color, stretched, or none for a neutral cover so the island stays white.

export function tintFromAverage(r: number, g: number, b: number): string | null {
  const [cr, cg, cb] = [r / 255, g / 255, b / 255];
  const max = Math.max(cr, cg, cb);
  const min = Math.min(cr, cg, cb);
  if (max <= 0.05 || (max - min) / max < 0.12) return null;
  const stretch = (c: number) => Math.round(((c - min) / (max - min)) * 0.86 * 255 + 0.06 * 255);
  return `rgb(${stretch(cr)}, ${stretch(cg)}, ${stretch(cb)})`;
}

const cache = new Map<string, Promise<string | null>>();

/** Averages the image down to one pixel; memoized per cover. */
export function tintOf(src: string): Promise<string | null> {
  let pending = cache.get(src);
  if (!pending) {
    pending = new Promise((resolve) => {
      const img = new Image();
      img.onload = () => {
        try {
          const canvas = document.createElement("canvas");
          canvas.width = canvas.height = 1;
          const ctx = canvas.getContext("2d", { willReadFrequently: true })!;
          ctx.drawImage(img, 0, 0, 1, 1);
          const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data;
          resolve(tintFromAverage(r!, g!, b!));
        } catch {
          resolve(null);
        }
      };
      img.onerror = () => resolve(null);
      img.src = src;
    });
    if (cache.size > 16) cache.clear();
    cache.set(src, pending);
  }
  return pending;
}
