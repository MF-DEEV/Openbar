// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Wires the Tools section into the island: its page, the keep-awake and
// mic state its tiles show, and the screenshot the Controls page asks for
// with a `vorssaint:screenshot` window event. The Scratchpad section, which
// a Tools tile opens, registers alongside.

import { pages } from "../island/views/Expanded";
import { runCapture } from "./capture";
import { subscribeExternals } from "./store";
import { ToolsLeading, ToolsPage, ToolsTrailing } from "./ToolsPage";
import "./tools.css";
import "../scratchpad/register";

pages.tools = { Page: ToolsPage, Leading: ToolsLeading, Trailing: ToolsTrailing };

void subscribeExternals();

window.addEventListener("vorssaint:screenshot", () => void runCapture("screenshot"));
