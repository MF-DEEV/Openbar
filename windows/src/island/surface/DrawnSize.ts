// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { createContext } from "react";
import type { Size } from "../motion";

/** The visible surface size, shared with compact content that follows its edges. */
export const DrawnSize = createContext<Size | null>(null);
