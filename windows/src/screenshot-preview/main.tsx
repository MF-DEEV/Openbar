// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
import "@fontsource-variable/inter";
import { createRoot } from "react-dom/client";
import { Preview } from "./Preview";
import "./standalone.css";

createRoot(document.getElementById("root")!).render(<Preview />);
