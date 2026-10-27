// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The AI Agents section's island wiring: its page and top row, the closed
// island's strip while an agent works (priority #5), and the notices
// agent_usage raises (NotchService.showAgentEvent, spec 03 §9).

import { CircleDollarSign, RotateCw, TriangleAlert } from "lucide-react";
import type { ReactElement } from "react";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { geometryOf, modulesOf, useIsland } from "../island/store";
import { activityViews } from "../island/views/Activity";
import { pages } from "../island/views/Expanded";
import { AgentsLeading, AgentsPage } from "./AgentsPage";
import { AgentsStrip, capsuleContent, currentReading, stripWing } from "./AgentsStrip";
import { cost, duration, PROVIDER_NAMES, PROVIDER_TINTS } from "./format";
import { advancesWithClock, limitShare, readingShape, windowLabel, workingProviders } from "./model";
import { Mark } from "./Parts";
import { subscribeAgents, useAgents } from "./store";
import { A } from "./strings";
import type { AgentEvent, Provider } from "./types";
import "./agents.css";

pages.agents = { Page: AgentsPage, Leading: AgentsLeading };
activityViews.agents = AgentsStrip;

/** A section hidden in Settings › Content shows nothing anywhere: no page, strip or notice. */
const shown = () => {
  const island = useIsland.getState();
  return island.settings.enabled && modulesOf(island).includes("agents");
};

// MARK: Strip

let lastShape = "";
let lastSpec = "";
/** The strip shows while an agent works and the person wants it there; its wings follow the reading's shape. */
function syncActivity() {
  const island = useIsland.getState();
  const { snapshot } = useAgents.getState();
  if (!shown() || !island.settings.agentsLiveActivity || snapshot.live.length === 0) {
    lastShape = lastSpec = "";
    island.setActivity("agents", null);
    return;
  }
  const reading = currentReading(snapshot, Date.now());
  const count = workingProviders(snapshot).length;
  const strip = geometryOf(island).stripHeight;
  const spec = {
    wing: stripWing(reading, count, strip),
    capsuleContent: capsuleContent(reading, count, strip),
    opens: "agents" as const,
  };
  const key = `${spec.wing}:${spec.capsuleContent}`;
  lastShape = readingShape(reading);
  if (key === lastSpec && island.activities.agents) return;
  lastSpec = key;
  island.setActivity("agents", spec);
}

// A clock reading that gains a digit, as when an hour passes, widens the wings.
setInterval(() => {
  const { snapshot } = useAgents.getState();
  if (!lastSpec || snapshot.live.length === 0) return;
  if (!advancesWithClock(useIsland.getState().settings.agentsReadout)) return;
  if (readingShape(currentReading(snapshot, Date.now())) !== lastShape) syncActivity();
}, 1000);

useAgents.subscribe((s, prev) => {
  if (s.snapshot !== prev.snapshot) syncActivity();
});
useIsland.subscribe((s, prev) => {
  if (s.settings !== prev.settings || s.screen !== prev.screen) syncActivity();
});

// MARK: Notices

/** A mark or symbol as a picture for the notice's image slot. */
function picture(element: ReactElement): string | undefined {
  try {
    const host = document.createElement("div");
    const root = createRoot(host);
    flushSync(() => root.render(element));
    let svg = host.innerHTML;
    root.unmount();
    if (!svg.startsWith("<svg")) return undefined;
    if (!svg.includes("xmlns=")) svg = svg.replace("<svg", '<svg xmlns="http://www.w3.org/2000/svg"');
    return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  } catch {
    return undefined;
  }
}

const markPicture = (provider: Provider) => picture(<Mark provider={provider} size={16} />);

export function agentNotice(event: AgentEvent, display: "remaining" | "used") {
  switch (event.kind) {
    case "finished":
      return {
        title: A.finished(PROVIDER_NAMES[event.provider]),
        detail: [duration(event.duration), event.cost > 0 ? cost(event.cost) : ""].filter(Boolean).join(" · "),
        image: () => markPicture(event.provider),
      };
    case "limitWarning":
      return {
        title: `${PROVIDER_NAMES[event.provider]} · ${windowLabel(event.window)}`,
        detail: limitShare(event.window, display),
        image: () => picture(<TriangleAlert size={18} color={PROVIDER_TINTS[event.provider]} strokeWidth={2.25} />),
      };
    case "limitReset":
      return {
        title: `${PROVIDER_NAMES[event.provider]} · ${windowLabel(event.window)}`,
        detail: A.limitRenewed,
        image: () => picture(<RotateCw size={18} color={PROVIDER_TINTS[event.provider]} strokeWidth={2.25} />),
      };
    case "budgetReached":
      return {
        title: A.budgetTitle,
        detail: cost(event.spent),
        image: () => picture(<CircleDollarSign size={18} color="#ffffff" strokeWidth={2.25} />),
      };
  }
}

function present(event: AgentEvent) {
  if (!shown()) return;
  const island = useIsland.getState();
  const notice = agentNotice(event, island.settings.agentsLimitDisplay);
  island.showNotice({
    id: `agents:${event.kind}:${Date.now()}`,
    kind: "agents",
    title: notice.title,
    detail: notice.detail,
    priority: 1,
    duration: 5,
    opens: "agents",
    image: notice.image(),
  });
}

void subscribeAgents(present);
syncActivity();
