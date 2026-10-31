// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Wires the Tools section into the island: its page, the keep-awake and
// mic state its tiles show, and the screenshot the Controls page asks for
// with a `vorssaint:screenshot` window event. The Scratchpad section, which
// a Tools tile opens, registers alongside.

import { pages } from "../island/views/Expanded";
import { useIsland } from "../island/store";
import { activityViews } from "../island/views/Activity";
import { bridge } from "../shared/bridge";
import { runCapture } from "./capture";
import { RecordingStrip } from "./RecordingStrip";
import { endedNotice, toggleRecording, useRecording } from "./recording";
import { subscribeExternals } from "./store";
import { ToolsLeading, ToolsPage, ToolsTrailing } from "./ToolsPage";
import "./tools.css";
import "../scratchpad/register";

pages.tools = { Page: ToolsPage, Leading: ToolsLeading, Trailing: ToolsTrailing };

void subscribeExternals();

window.addEventListener("vorssaint:screenshot", () => void runCapture("screenshot"));
window.addEventListener("vorssaint:recording", () => toggleRecording());

activityViews.recording = RecordingStrip;
function setRecording(startedAt: number | null) {
  useRecording.setState({ startedAt });
  useIsland.getState().setActivity("recording", startedAt === null ? null : { wing: 52, opens: "tools" });
}
void bridge.capture.recorder.onStarted(() => setRecording(Date.now()));
void bridge.capture.recorder.onEnded((ended) => {
  setRecording(null);
  useIsland.getState().showNotice(endedNotice(ended));
});
void bridge.capture.recorder.running().then((running) => {
  if (running) setRecording(Date.now());
});
