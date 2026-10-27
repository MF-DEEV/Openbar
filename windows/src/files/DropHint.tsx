// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The closed island while a drag is over it (NotchView.swift:106-128): a
// dashed 16-radius box, 52 tall under the safe top, saying where to drop.

import { Inbox } from "lucide-react";
import { useContext } from "react";
import type { Geometry } from "../island/geometry";
import type { Size } from "../island/motion";
import { DrawnSize } from "../island/surface/DrawnSize";
import { S } from "./strings";
import "./files.css";

export function DropHint(props: { geometry: Geometry; size: Size }) {
  const size = useContext(DrawnSize) ?? props.size;
  return (
    <div
      className="fl-drop-hint"
      style={{ width: size.width, height: size.height, paddingTop: props.geometry.safeContentTop }}
    >
      <div className="fl-drop-box">
        <Inbox size={15} strokeWidth={2} aria-hidden="true" />
        <span>{S.dropHint}</span>
      </div>
    </div>
  );
}
