// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The closed island's live activity strip: each section registers the view
// that draws its strip (see pages.ts).

import { type ComponentType, useContext } from "react";
import type { Geometry } from "../geometry";
import type { Size } from "../motion";
import type { ActivityId } from "../store";
import { DrawnSize } from "../surface/DrawnSize";

export interface ActivityViewProps {
  geometry: Geometry;
  size: Size;
}

export const activityViews: Partial<Record<ActivityId, ComponentType<ActivityViewProps>>> = {};

export function ActivityStrip({ id, geometry, size }: { id: ActivityId } & ActivityViewProps) {
  const drawn = useContext(DrawnSize) ?? size;
  const View = activityViews[id];
  return View ? <View geometry={geometry} size={drawn} /> : null;
}
