// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A history for designing the Clipboard section in a plain browser
// (?clipboard). Pictures and icons come as data URLs there.

import type { ClipboardEntry, ClipboardSnapshot } from "./types";

function picture(w: number, h: number, from: string, to: string): string {
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const g = c.getContext("2d")!;
  const grad = g.createLinearGradient(0, 0, w, h);
  grad.addColorStop(0, from);
  grad.addColorStop(1, to);
  g.fillStyle = grad;
  g.fillRect(0, 0, w, h);
  g.fillStyle = "rgba(255,255,255,0.8)";
  g.font = `600 ${Math.round(h / 5)}px sans-serif`;
  g.fillText("Screenshot", w * 0.08, h * 0.55);
  return c.toDataURL("image/png");
}

function entry(partial: Partial<ClipboardEntry> & Pick<ClipboardEntry, "id">, minutesAgo: number): ClipboardEntry {
  const text = partial.text ?? "";
  return {
    kind: "text",
    text,
    preview: text.replace(/[\r\n\t]+/g, " ").trim(),
    cardPreview: text.trim(),
    copiedAt: Date.now() - minutesAgo * 60_000,
    pinned: false,
    filePaths: [],
    imagePath: null,
    imageWidth: null,
    imageHeight: null,
    sourceExe: null,
    sourceName: null,
    ...partial,
  };
}

export function demoHistory(): ClipboardSnapshot {
  const code = picture(32, 32, "#3b82f6", "#1e3a8a");
  return {
    captured: null,
    entries: [
      entry({ id: "p1", text: "ssh deploy@build.example.net -p 2222", pinned: true }, 600),
      entry(
        {
          id: "t1",
          text: "export function filterEntries(entries, query) {\n  const tokens = normalized(query).split(/\\s+/);\n  return entries.filter((e) => tokens.every((t) => e.text.includes(t)));\n}",
          sourceExe: code,
          sourceName: "Visual Studio Code",
        },
        1,
      ),
      entry({ id: "c1", text: "#FF6B35" }, 3),
      entry(
        {
          id: "i1",
          kind: "image",
          imagePath: picture(640, 360, "#ff5f6d", "#7b2ff7"),
          imageWidth: 1920,
          imageHeight: 1080,
          preview: "1920×1080",
          cardPreview: "1920×1080",
        },
        8,
      ),
      entry(
        {
          id: "f1",
          kind: "files",
          filePaths: ["C:\\Users\\Dawid\\Documents\\Quarterly report – final version (signed).pdf"],
          preview: "Quarterly report – final version (signed).pdf",
          cardPreview: "Quarterly report – final version (signed).pdf",
        },
        15,
      ),
      entry(
        {
          id: "t2",
          text: "Thanks for the update! I’ll review the pull request this afternoon and leave comments on the clipboard changes, especially the paste flow into elevated apps.",
        },
        42,
      ),
      entry(
        {
          id: "f2",
          kind: "files",
          filePaths: ["C:\\a\\one.txt", "C:\\a\\two.txt", "C:\\a\\three.txt"],
          preview: "one.txt, two.txt, three.txt",
          cardPreview: "one.txt, two.txt, three.txt",
        },
        90,
      ),
      entry({ id: "t3", text: "https://github.com/MF-DEEV/vorssaint" }, 200),
    ],
  };
}
