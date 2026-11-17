// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Designing in a plain browser: `?tools` stands in keep awake (on) and a
// muted microphone, fills the scratchpad with two pads, and opens the
// island on `?tools=<section>` when given (tools or scratchpad).

import { useIsland } from "../island/store";
import type { ModuleId } from "../island/modules";
import { bridge } from "../shared/bridge";
import { useScratchpad } from "../scratchpad/store";
import { subscribeExternals } from "./store";

export function startToolsDemo(open: string | null) {
  const backend = (on: boolean) => ({
    toggle: async () => {},
    onState: async (handler: (on: boolean) => void) => {
      handler(on);
      return () => {};
    },
  });
  Object.assign(bridge, { keepAwake: backend(true), mic: backend(true) });
  void subscribeExternals();
  useScratchpad.setState({
    doc: {
      pads: [
        {
          id: "demo-1",
          name: "Scratchpad 1",
          text: "# Groceries\n- [x] milk\n- [ ] bread\n- eggs\n\nCall **Ana** at 5",
          modifiedAt: Date.now(),
        },
        { id: "demo-2", name: "Ideas", text: "" },
      ],
      selectedId: "demo-1",
    },
  });
  if (open === "tools" || open === "scratchpad") setTimeout(() => useIsland.getState().open(open as ModuleId), 300);
}
