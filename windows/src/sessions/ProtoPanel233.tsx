// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// PROTOTYPE ONLY (#233): picks the looks in a browser. Removed before the PR.

import { createRoot } from "react-dom/client";
import { useProto233, type Look, type Switcher } from "./proto233";
import { setGrouping } from "./SessionGroups";
import { useSessions } from "./store";
import type { Grouping } from "../shared/prefs";

function Choice<T extends string>(props: { label: string; value: T; options: T[]; set(v: T): void }) {
  return (
    <div style={{ display: "flex", gap: 4, alignItems: "center" }}>
      <span style={{ width: 64, opacity: 0.6 }}>{props.label}</span>
      {props.options.map((o) => (
        <button
          key={o}
          type="button"
          onClick={() => props.set(o)}
          style={{
            padding: "3px 8px",
            borderRadius: 6,
            background: props.value === o ? "#fff" : "rgba(255,255,255,0.12)",
            color: props.value === o ? "#000" : "#fff",
          }}
        >
          {o}
        </button>
      ))}
    </div>
  );
}

function Panel() {
  const { look, switcher } = useProto233();
  const grouping = useSessions((s) => s.prefs.grouping);
  return (
    <div
      style={{
        position: "fixed",
        left: 12,
        bottom: 12,
        zIndex: 99999,
        display: "grid",
        gap: 6,
        padding: 10,
        borderRadius: 10,
        background: "rgba(30,30,34,0.95)",
        color: "#fff",
        font: "11px system-ui",
      }}
    >
      <Choice<Grouping> label="Group" value={grouping} options={["none", "project", "agent"]} set={setGrouping} />
      <Choice<Look>
        label="Look"
        value={look}
        options={["plain", "rail", "card"]}
        set={(v) => useProto233.setState({ look: v })}
      />
      <Choice<Switcher>
        label="Switch"
        value={switcher}
        options={["segment", "menu"]}
        set={(v) => useProto233.setState({ switcher: v })}
      />
    </div>
  );
}

export function mountProtoPanel() {
  const host = document.createElement("div");
  document.body.appendChild(host);
  createRoot(host).render(<Panel />);
}
