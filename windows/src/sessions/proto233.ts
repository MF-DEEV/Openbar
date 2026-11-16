// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// PROTOTYPE ONLY (#233): the looks to pick from in a browser, `island.html?sessions&groups&proto233`.
// Removed before the PR.

import { create } from "zustand";

export type Look = "plain" | "rail" | "card";
export type Switcher = "segment" | "menu";

export const useProto233 = create<{ look: Look; switcher: Switcher }>(() => ({ look: "plain", switcher: "segment" }));
