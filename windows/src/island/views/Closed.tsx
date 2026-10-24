// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// What the closed island shows: nothing at rest, a notice, or the hover
// preview row (NotchView.surface; spec 01-shell §2.2).

import { ChevronDown, LayoutGrid } from "lucide-react";
import { t } from "../../i18n";
import type { Geometry } from "../geometry";
import type { Size } from "../motion";
import { type Notice, useIsland } from "../store";
import { IconButton } from "./Controls";

export function Resting(props: { size: Size }) {
  return (
    <button
      type="button"
      className="n-activation"
      style={{ width: props.size.width, height: props.size.height }}
      aria-label={t("island.open")}
      onClick={() => useIsland.getState().clickClosed()}
    />
  );
}

/** One line of text either side of the camera: title left, detail right. */
export function NoticeView(props: { notice: Notice; geometry: Geometry; size: Size }) {
  const { notice, geometry, size } = props;
  const camera = geometry.floatingGap === null ? geometry.cameraWidth : 0;
  return (
    <button
      type="button"
      className="n-notice"
      style={{ width: size.width, height: size.height, columnGap: camera }}
      onClick={() => useIsland.getState().clickClosed()}
    >
      <span className="n-notice-title">{notice.title}</span>
      {notice.detail ? <span className="n-notice-detail">{notice.detail}</span> : null}
    </button>
  );
}

/** Measures a notice's natural width so the island fits it (NotchNotice.preferredWings). */
export function noticeWidth(notice: Notice, geometry: Geometry): number {
  const ctx = measureContext();
  ctx.font = "500 11px 'Inter Variable', Inter, sans-serif";
  const title = ctx.measureText(notice.title).width;
  const detail = notice.detail ? ctx.measureText(notice.detail).width : 0;
  const maxWing = (geometry.screen.width - 24 - geometry.cameraWidth) / 2;
  if (geometry.floatingGap !== null) {
    return Math.min(340 + 28, Math.ceil(16 + 6 + title + (detail ? 10 + detail : 0) + 2 * 14));
  }
  const leading = Math.min(Math.max(Math.ceil(title + 18 + 8) + 16 + 6, 36), Math.min(240, maxWing));
  const trailing = Math.min(Math.max(Math.ceil(detail) + 16 + 6, 36), Math.min(240, maxWing));
  return geometry.cameraWidth + 2 * Math.max(leading, trailing);
}

let canvas: HTMLCanvasElement | undefined;
function measureContext(): CanvasRenderingContext2D {
  canvas ??= document.createElement("canvas");
  return canvas.getContext("2d")!;
}

/** The hover preview: where opening leads, and a chevron to open. */
export function Peek(props: { geometry: Geometry; size: Size }) {
  const selected = useIsland((s) => s.selected);
  return (
    <div
      className="n-peek"
      style={{ width: props.size.width, height: props.size.height, paddingTop: props.geometry.safeContentTop }}
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
