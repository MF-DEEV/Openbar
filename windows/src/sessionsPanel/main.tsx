// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// The window of the sessions' tray panel (#331; sessions_panel.rs).

import { initializeLocalization } from "../i18n/start";
import "@fontsource-variable/inter";
import React from "react";
import ReactDOM from "react-dom/client";
import { inTauri } from "../shared/bridge";
import { Panel, startPanel } from "./Panel";
import "../island/island.css";
import "../shared/controls.css";
import "../shared/ui/ui.css";
import "./panel.css";

startPanel();

// Designing in a plain browser: ?sessions fills the panel with samples.
if (!inTauri && new URLSearchParams(location.search).has("sessions")) {
  void Promise.all([import("../sessions/demo"), import("../sessions/store")]).then(([demo, store]) => {
    store.useSessions.getState().receive(demo.demoSnapshot());
  });
}

void initializeLocalization().then(() => {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <Panel />
    </React.StrictMode>,
  );
});
