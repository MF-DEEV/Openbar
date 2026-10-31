// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// With "All displays", renders the closed island off-screen with the real
// strips, whatever the island itself is showing, and sends its markup to the
// mirrors on the other displays (NotchService.syncMirrors, spec 01-shell §6.1).
// Nothing is rendered or sent while there is no other display.

import { useEffect, useId, useRef, useState } from "react";
import { bridge, type MirrorFrame } from "../shared/bridge";
import { type Geometry, hitShape, islandPath } from "./geometry";
import type { IdleContent } from "./settings";
import { mirrorSurface, useIsland } from "./store";
import { DrawnSize } from "./surface/DrawnSize";
import { ActivityPicker } from "./views/ActivityPicker";
import { Resting } from "./views/Closed";
import { mascotShows, useMascot } from "../mascot/service";

/** Four times a second is enough for a clock, and cheap beside a full second page. */
const SEND_EVERY_MS = 250;

/** The element's markup with each canvas as a picture of its current frame. */
export function markup(element: Element): string {
  const copy = element.cloneNode(true) as Element;
  const canvases = element.querySelectorAll("canvas");
  copy.querySelectorAll("canvas").forEach((canvas, i) => {
    const source = canvases[i]!;
    const image = document.createElement("img");
    image.alt = "";
    image.className = source.className;
    image.style.cssText = source.style.cssText;
    if (!source.style.width) image.style.width = `${source.getBoundingClientRect().width}px`;
    if (!source.style.height) image.style.height = `${source.getBoundingClientRect().height}px`;
    try {
      image.src = source.toDataURL();
    } catch {
      // A canvas that can't be read leaves its slot empty.
    }
    canvas.replaceWith(image);
  });
  return copy.outerHTML;
}

function pageStyles(): { styles: string[]; css: string } {
  const links = document.querySelectorAll<HTMLLinkElement>('link[rel="stylesheet"]');
  const tags = document.querySelectorAll("style");
  return {
    styles: Array.from(links, (link) => link.href),
    css: Array.from(tags, (tag) => tag.textContent ?? "").join("\n"),
  };
}

export function MirrorSource({ geometry, idleContent }: { geometry: Geometry; idleContent: IdleContent }) {
  const [mirroring, setMirroring] = useState(false);
  useEffect(() => {
    let stale = false;
    const off = bridge.onMirroring(setMirroring);
    void bridge.mirroring().then((on) => {
      if (!stale && on) setMirroring(true);
    });
    return () => {
      stale = true;
      void off.then((f) => f());
    };
  }, []);
  return mirroring ? <Source geometry={geometry} idleContent={idleContent} /> : null;
}

function Source({ geometry, idleContent }: { geometry: Geometry; idleContent: IdleContent }) {
  const state = useIsland();
  const mascot = useMascot();
  const surface = mirrorSurface(state, geometry, idleContent, mascotShows(state, geometry, mascot));
  const holder = useRef<HTMLDivElement>(null);
  const clipId = `mirror-clip-${useId().replace(/[^a-zA-Z0-9]/g, "")}`;
  const size = surface?.size;
  const hit = size ? hitShape(geometry, size) : null;
  const frameInfo = useRef<Pick<MirrorFrame, "width" | "hit"> | null>(null);
  frameInfo.current =
    size && hit
      ? {
          width: size.width,
          hit: {
            x: hit.rect.x,
            y: geometry.floatingDrop + hit.rect.y,
            width: hit.rect.width,
            height: hit.rect.height,
            topRadius: hit.topRadius,
            bottomRadius: hit.bottomRadius,
          },
        }
      : null;

  useEffect(() => {
    let last = "";
    const send = () => {
      const element = holder.current?.firstElementChild;
      const info = frameInfo.current;
      const frame: MirrorFrame | null = element && info ? { html: markup(element), ...pageStyles(), ...info } : null;
      const key = JSON.stringify(frame);
      if (key === last) return;
      last = key;
      void bridge.setMirrorFrame(frame);
    };
    send();
    const timer = setInterval(send, SEND_EVERY_MS);
    return () => {
      clearInterval(timer);
      void bridge.setMirrorFrame(null);
    };
  }, []);

  const d = size ? islandPath(geometry, size) : "";
  return (
    <div ref={holder} className="island-mirror-source" aria-hidden inert>
      {size ? (
        <div className="island-frame" style={{ width: size.width, height: size.height, top: geometry.floatingDrop }}>
          <svg className="island-backdrop" width={size.width} height={size.height} aria-hidden>
            <clipPath id={clipId}>
              <path d={d} />
            </clipPath>
            <path d={d} fill="#000" />
            {state.settings.outline ? (
              <path
                d={d}
                fill="none"
                stroke={surface?.activity === "timer" ? "rgba(255,159,10,0.65)" : "rgba(255,255,255,0.65)"}
                strokeWidth={2}
                clipPath={`url(#${clipId})`}
              />
            ) : null}
          </svg>
          <div className="island-content" style={{ clipPath: `path("${d}")` }}>
            <DrawnSize.Provider value={size}>
              {surface?.activity ? (
                <ActivityPicker geometry={geometry} size={size} visible={false} />
              ) : (
                <Resting size={size} geometry={geometry} idleContent={idleContent} />
              )}
            </DrawnSize.Provider>
          </div>
        </div>
      ) : null}
    </div>
  );
}
