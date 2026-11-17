// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The closed island while a drag is over it (NotchView.swift:106-128): a
// dashed 16-radius box, 52 tall under the safe top, saying where to drop.

import { useLocale } from "../i18n";

import { Inbox } from "lucide-react";
import { useContext } from "react";
import { contentInsets, type Geometry } from "../island/geometry";
import type { Size } from "../island/motion";
import { DrawnSize } from "../island/surface/DrawnSize";
import { S } from "./strings";
import { MascotFace } from "../mascot/MascotView";
import { mascotEnabled, mascotLook } from "../mascot/service";
import { useIsland } from "../island/store";
import "./files.css";

export function DropHint(props: { geometry: Geometry; size: Size }) {
  useLocale();
  const size = useContext(DrawnSize) ?? props.size;
  const settings = useIsland((s) => s.settings);
  return (
    <div
      className="fl-drop-hint"
      style={{
        width: size.width,
        height: size.height,
        paddingTop: props.geometry.safeContentTop,
        // 18 in, or clear of the outline's curved ends, like the peek row.
        paddingInline: Math.max(18, contentInsets(props.geometry, size).horizontal),
      }}
    >
      <div className="fl-drop-box">
        {/* The companion stands by the hint and waits for the file (NotchView.swift:108). */}
        {mascotEnabled(settings) ? (
          <MascotFace look={mascotLook(settings)} size={20} />
        ) : (
          <Inbox size={15} strokeWidth={2} aria-hidden="true" />
        )}
        <span>{S.dropHint}</span>
      </div>
    </div>
  );
}
