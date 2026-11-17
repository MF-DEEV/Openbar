// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { initializeLocalization } from "../i18n/start";
import "@fontsource-variable/inter";
import { createRoot } from "react-dom/client";
import { Preview } from "./Preview";
import "./standalone.css";

void initializeLocalization().then(() => {
  createRoot(document.getElementById("root")!).render(<Preview />);
});
