// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { createContext } from "react";
import type { Size } from "../motion";

/** The visible surface size, shared with compact content that follows its edges. */
export const DrawnSize = createContext<Size | null>(null);

/** Where the surface draws a shape of a given size inside its frame, so compact content moves with the outline. */
export const DrawnOffset = createContext<((size: Size) => { x: number; y: number }) | null>(null);
