// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The settings window: Windows 11's Settings layout, with the Mac's island
// tabs (Layout, Content, Activity, Behavior) as pages beside General.

import "@fontsource-variable/inter";
import { Activity, LayoutTemplate, type LucideIcon, Settings2, SlidersHorizontal, Wand2 } from "lucide-react";
import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { ActivitySettings } from "./ActivitySettings";
import { BehaviorSettings } from "./BehaviorSettings";
import { ContentSettings } from "./ContentSettings";
import { GeneralSettings } from "./GeneralSettings";
import { LayoutSettings } from "./LayoutSettings";
import { startSettings, useSettings } from "./state";
import { T } from "./strings";
import "./settings.css";

type PageId = "general" | "layout" | "content" | "activity" | "behavior";

const PAGES: { id: PageId; icon: LucideIcon; label: string; Page: () => React.JSX.Element; island?: boolean }[] = [
  { id: "general", icon: Settings2, label: T.nav.general, Page: GeneralSettings },
  { id: "layout", icon: LayoutTemplate, label: T.nav.layout, Page: LayoutSettings, island: true },
  { id: "content", icon: SlidersHorizontal, label: T.nav.content, Page: ContentSettings, island: true },
  { id: "activity", icon: Activity, label: T.nav.activity, Page: ActivitySettings, island: true },
  { id: "behavior", icon: Wand2, label: T.nav.behavior, Page: BehaviorSettings, island: true },
];

const PAGE_KEY = "vorssaint.settings.page";

function initialPage(): PageId {
  const hash = location.hash.slice(1) as PageId;
  if (PAGES.some((p) => p.id === hash)) return hash;
  try {
    const saved = localStorage.getItem(PAGE_KEY) as PageId | null;
    if (saved && PAGES.some((p) => p.id === saved)) return saved;
  } catch {
    // No storage: start on General.
  }
  return "general";
}

function App() {
  const [page, setPage] = useState<PageId>(initialPage);
  const error = useSettings((s) => s.error);
  useEffect(() => {
    try {
      localStorage.setItem(PAGE_KEY, page);
    } catch {
      // Remembering the page is only a convenience.
    }
    history.replaceState(null, "", `#${page}`);
    document.querySelector(".st-main")?.scrollTo({ top: 0 });
  }, [page]);

  const current = PAGES.find((p) => p.id === page)!;
  const pick = (id: PageId) => setPage(id);

  return (
    <div className="st-app">
      <nav className="st-nav" aria-label="Settings">
        <div className="st-brand">
          <span className="st-brand-mark" aria-hidden />
          <span className="st-brand-text">
            <div className="st-brand-name">Vorssaint</div>
            <div className="st-brand-sub">{T.brand}</div>
          </span>
        </div>
        {PAGES.map((p, i) => (
          <React.Fragment key={p.id}>
            {p.island && !PAGES[i - 1]?.island ? <div className="st-nav-heading">{T.nav.island}</div> : null}
            <button
              type="button"
              className="st-nav-item"
              aria-current={p.id === page ? "page" : undefined}
              title={p.label}
              onClick={() => pick(p.id)}
            >
              <p.icon size={18} strokeWidth={1.75} aria-hidden />
              <span className="st-nav-label">{p.label}</span>
            </button>
          </React.Fragment>
        ))}
      </nav>
      <main className="st-main">
        {error ? (
          <div className="st-banner" role="alert">
            {T.saveFailed} {error}
          </div>
        ) : null}
        <current.Page key={current.id} />
      </main>
    </div>
  );
}

startSettings();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
