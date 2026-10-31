// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { initializeLocalization } from "../i18n/start";

// The screen selection window (capture.html). In a plain browser,
// `?color` or nothing shows the overlay on a test picture for designing.

import "@fontsource-variable/inter";
import React from "react";
import ReactDOM from "react-dom/client";
import { Overlay } from "./Overlay";
import "./capture.css";

void initializeLocalization().then(() => {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <Overlay />
    </React.StrictMode>,
  );
});
