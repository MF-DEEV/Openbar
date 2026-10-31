// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale } from "../i18n/index";

// Only when the island page is opened in a plain browser with ?desktop: a
// stand-in Windows desktop and menu bar behind the island, for designing
// and screenshots without the app around it.

import { Bar } from "../bar/Bar";
import "../bar/bar.css";

export function DevDesktop() {
  useLocale();
  return (
    <div className="dev-desktop">
      <div className="dev-bar">
        <Bar appName="Windows Terminal" />
      </div>
      <div className="dev-window">
        <div className="dev-window-title">PowerShell</div>
        <pre>PS C:\Users\vors&gt; claude</pre>
      </div>
      <div className="dev-taskbar" />
    </div>
  );
}
