// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What the closed island shows: nothing at rest, a notice, or the hover
// preview row (NotchView.surface; spec 01-shell §2.2).

import { ChevronDown, LayoutGrid, ArrowDownToLine } from "lucide-react";
import { useContext } from "react";
import { t } from "../../i18n";
import { contentInsets, edgeInset, capsuleBody, capsuleRadius, type Geometry } from "../geometry";
import type { Size } from "../motion";
import { type Notice, useIsland } from "../store";
import { IconButton } from "./Controls";
import { DrawnSize } from "../surface/DrawnSize";
import { AudioIcons } from "../../microphone/Microphone";

export function Resting(props: { size: Size }) {
  const size = useContext(DrawnSize) ?? props.size;
  return (
    <button
      type="button"
      className="n-activation"
      style={{ width: size.width, height: size.height }}
      aria-label={t("island.open")}
      onClick={() => useIsland.getState().clickClosed()}
    />
  );
}

/** Match the notice to the strip while leaving space around the curved ends. */
function noticeMetrics(geometry: Geometry, size: Size) {
  const body = geometry.floatingGap !== null ? capsuleBody(size.width, size.height, geometry.floatingGap) : null;
  const height = body?.height ?? size.height;
  const imageSize = Math.min(22, Math.max(12, height - 10));
  const fontSize = Math.min(12, Math.max(10, height * 0.44));
  const padding = body
    ? Math.max(20, body.x + 10 + capsuleRadius(height) * 0.3)
    : Math.max(20, edgeInset(size, imageSize, 4, 8, geometry.input) + 4);
  return { imageSize, fontSize, padding };
}

/** One line of text either side of the camera: title left, detail right. */
export function NoticeView(props: { notice: Notice; geometry: Geometry; size: Size }) {
  const { notice, geometry } = props;
  const size = useContext(DrawnSize) ?? props.size;
  const camera = geometry.floatingGap === null ? geometry.cameraWidth : 0;
  const metrics = noticeMetrics(geometry, size);
  return (
    <button
      type="button"
      className="n-notice"
      aria-label={[notice.title, notice.detail].filter(Boolean).join(": ")}
      title={[notice.title, notice.detail].filter(Boolean).join("\n")}
      style={{
        width: size.width,
        height: size.height,
        columnGap: camera,
        paddingInline: metrics.padding,
        fontSize: metrics.fontSize,
        lineHeight: "1.25",
      }}
      onClick={() => useIsland.getState().clickClosed()}
    >
      <span className="n-notice-title" style={{ color: notice.tint }}>
        {notice.audio ? <AudioIcons muted={notice.audio.muted} deafened={notice.audio.deafened} /> : null}
        {notice.kind === "downloads" ? <ArrowDownToLine size={14} aria-hidden="true" /> : null}
        {notice.icon ? <notice.icon size={14} strokeWidth={2} aria-hidden="true" /> : null}
        {notice.image ? (
          <img
            key={notice.image}
            className="n-notice-image"
            src={notice.image}
            alt=""
            onError={(event) => {
              const image = event.currentTarget;
              if (notice.imageFallback && image.getAttribute("src") !== notice.imageFallback) {
                image.src = notice.imageFallback;
              } else {
                image.style.visibility = "hidden";
              }
            }}
            style={{ width: metrics.imageSize, height: metrics.imageSize }}
          />
        ) : null}
        <span>{notice.title}</span>
      </span>
      {notice.detail ? (
        <span
          className="n-notice-detail"
          style={{ fontSize: Math.max(9, metrics.fontSize - 1), color: notice.audio?.muted ? "#ff453a" : undefined }}
        >
          {notice.detail}
        </span>
      ) : null}
      {notice.progress !== undefined ? (
        <span
          className={`n-download-progress${notice.progress === null ? " is-indeterminate" : ""}`}
          aria-hidden="true"
        >
          <span style={notice.progress === null ? undefined : { width: `${notice.progress * 100}%` }} />
        </span>
      ) : null}
    </button>
  );
}

/** Measures a notice's natural width so the island fits it (NotchNotice.preferredWings). */
export function noticeWidth(notice: Notice, geometry: Geometry): number {
  const ctx = measureContext();
  const metrics = noticeMetrics(geometry, { width: geometry.screen.width - 24, height: geometry.stripHeight });
  ctx.font = `500 ${metrics.fontSize}px 'Inter Variable', Inter, sans-serif`;
  const title =
    ctx.measureText(notice.title).width +
    (notice.image ? metrics.imageSize + 8 : 0) +
    (notice.icon ? 14 + 8 : 0) +
    (notice.audio ? (notice.audio.deafened ? 47 : 24) : 0);
  ctx.font = `500 ${Math.max(9, metrics.fontSize - 1)}px 'Inter Variable', Inter, sans-serif`;
  const detail = notice.detail ? ctx.measureText(notice.detail).width : 0;
  const maxWing = (geometry.screen.width - 24 - geometry.cameraWidth) / 2;
  if (geometry.floatingGap !== null) {
    return Math.min(
      368,
      geometry.screen.width - 24,
      Math.ceil(2 * metrics.padding + title + (detail ? 12 + detail : 0)),
    );
  }
  const leading = Math.min(Math.max(Math.ceil(title + metrics.padding + 10), 36), Math.min(240, maxWing));
  const trailing = Math.min(Math.max(Math.ceil(detail + metrics.padding + 10), 36), Math.min(240, maxWing));
  return geometry.cameraWidth + 2 * Math.max(leading, trailing);
}

let canvas: HTMLCanvasElement | undefined;
function measureContext(): CanvasRenderingContext2D {
  canvas ??= document.createElement("canvas");
  return canvas.getContext("2d")!;
}

/** The hover preview: where opening leads, and a chevron to open. */
export function Peek(props: { geometry: Geometry; size: Size }) {
  const size = useContext(DrawnSize) ?? props.size;
  const selected = useIsland((s) => s.selected);
  return (
    <div
      className="n-peek"
      style={{
        width: size.width,
        height: size.height,
        paddingTop: props.geometry.safeContentTop,
        paddingInline: contentInsets(props.geometry, size).horizontal,
      }}
    >
      <button
        type="button"
        className="n-peek-nav"
        onClick={() => useIsland.getState().open(undefined, { sections: true })}
      >
        <LayoutGrid size={14} strokeWidth={2} />
        <span>{t(`module.${selected}`)}</span>
      </button>
      <IconButton icon={ChevronDown} label={t("island.open")} onClick={() => useIsland.getState().open()} />
    </div>
  );
}
