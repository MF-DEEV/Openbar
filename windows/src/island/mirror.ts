// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A mirror window: draws what the island page sends, using the page's own
// styles, and passes on a click (NotchMirrorView.swift; spec 01-shell §6.1).
// The island's stores stay in one page, so this one holds no state of its own.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { MirrorFrame } from "../shared/bridge";
import { insetUsableEdge, islandEdge, placeIsland, usableDisplayArea } from "./placement";

const root = document.getElementById("root")!;
const inline = document.head.appendChild(document.createElement("style"));
let drawn: string | null = null;
let sheets = "";

function draw(frame: MirrorFrame | null) {
  const key = frame?.styles.join("\n") ?? sheets;
  if (key !== sheets) {
    sheets = key;
    document.head.querySelectorAll("link[data-mirror]").forEach((link) => link.remove());
    for (const href of frame!.styles) {
      const link = document.head.appendChild(document.createElement("link"));
      link.rel = "stylesheet";
      link.href = href;
      link.dataset["mirror"] = "";
    }
  }
  if (frame && inline.textContent !== frame.css) inline.textContent = frame.css;
  const html = frame?.html ?? "";
  // Unchanged markup is left alone, so its CSS animations keep running.
  if (html !== drawn) {
    drawn = html;
    root.innerHTML = html;
  }
  const element = root.querySelector<HTMLElement>(".island-frame");
  if (element && frame?.position) {
    const screen = { width: innerWidth, height: innerHeight };
    const usable = usableDisplayArea(screen);
    const edge = islandEdge(frame.position, screen, usable);
    const place = placeIsland(frame.position, screen,
      { width: frame.width, height: frame.height ?? element.getBoundingClientRect().height }, insetUsableEdge(usable, edge, frame.edgeGap ?? 0, screen));
    element.style.left = `${place.left}px`;
    element.style.top = `${place.top + (frame.height ?? 0) - element.getBoundingClientRect().height}px`;
    element.style.transform = "none";
  }
}

document.addEventListener("click", () => void invoke("island_mirror_open"));
void listen<MirrorFrame | null>("island://mirror-frame", (event) => draw(event.payload));
void invoke<MirrorFrame | null>("island_mirror_frame").then((frame) => {
  if (drawn === null) draw(frame);
});
