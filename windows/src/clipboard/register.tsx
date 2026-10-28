// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Clipboard section in the island: its page with the magnifier and the
// history's actions in the top row, and the history's live updates (which
// also raise the "Copied" notice). It has no closed-island strip.

import { pages } from "../island/views/Expanded";
import { ClipboardLeading, ClipboardPage, ClipboardTrailing } from "./ClipboardPage";
import { subscribeClipboard } from "./store";

pages.clipboard = { Page: ClipboardPage, Leading: ClipboardLeading, Trailing: ClipboardTrailing };
void subscribeClipboard();
