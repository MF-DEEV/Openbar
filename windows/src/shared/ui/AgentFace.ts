// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Sessions mark (OpenbarSymbol.agentFace in NotchSymbol.swift): an agent's
// face with Clawd's two tall eyes cut out of it. Filled, unlike the lucide
// icons around it, so the eyes read at 14 px. Built with lucide's own factory,
// so it goes wherever a LucideIcon does.

import { createLucideIcon } from "lucide-react";

export const AgentFace = createLucideIcon("agent-face", [
  [
    "path",
    {
      d:
        "M7.5 4.5h9a4.5 4.5 0 0 1 4.5 4.5v6a4.5 4.5 0 0 1-4.5 4.5h-9A4.5 4.5 0 0 1 3 15V9a4.5 4.5 0 0 1 4.5-4.5Z" +
        "M9.2 9a1.2 1.2 0 0 1 1.2 1.2v3.1a1.2 1.2 0 0 1-2.4 0v-3.1A1.2 1.2 0 0 1 9.2 9Z" +
        "M14.8 9a1.2 1.2 0 0 1 1.2 1.2v3.1a1.2 1.2 0 0 1-2.4 0v-3.1a1.2 1.2 0 0 1 1.2-1.2Z",
      fill: "currentColor",
      fillRule: "evenodd",
      stroke: "none",
      key: "agent-face",
    },
  ],
]);
