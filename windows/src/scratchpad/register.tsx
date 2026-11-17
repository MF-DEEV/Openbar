// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Wires the Scratchpad section into the island.

import { pages } from "../island/views/Expanded";
import { ScratchpadLeading, ScratchpadPage, ScratchpadTrailing } from "./ScratchpadPage";
import "./scratchpad.css";

pages.scratchpad = { Page: ScratchpadPage, Leading: ScratchpadLeading, Trailing: ScratchpadTrailing };
