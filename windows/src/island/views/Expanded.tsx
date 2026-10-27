// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The open island (NotchView.expanded; spec 01-shell §1.5): a 36 pt top row,
// then the page, 28 pt in from each side and 16 pt from the bottom.

import { ChevronLeft, LayoutGrid, Pin, Search } from "lucide-react";
import { type ComponentType, type ReactNode, useEffect, useRef, useState } from "react";
import { t } from "../../i18n";
import { type Geometry, headerMetrics, contentInsets } from "../geometry";
import { type ModuleId, moduleInfo } from "../modules";
import type { Size } from "../motion";
import { useIsland } from "../store";
import { EmptyState, IconButton } from "./Controls";
import { SectionsGallery } from "./Sections";

export interface PageProps {
  width: number;
  height: number;
}

export interface PageDefinition {
  Page: ComponentType<PageProps>;
  /** Items on the left of the top row, in place of the sections button. */
  Leading?: ComponentType;
  /** Items on the right of the top row. */
  Trailing?: ComponentType;
}

/** Section pages register here as they are built. */
export const pages: Partial<Record<ModuleId, PageDefinition>> = {};

export function Expanded(props: { geometry: Geometry; size: Size; hasRow: boolean }) {
  const { geometry, size, hasRow } = props;
  // Each layer keeps the page it opened with: its content key names that
  // page, and while it fades out it must not draw a second copy of the
  // page replacing it (whose effects would then run twice).
  const [showingSections] = useState(() => useIsland.getState().showingSections);
  const [selected] = useState(() => useIsland.getState().selected);
  const pinned = useIsland((s) => s.pinned);
  const hasExploreBubble = useIsland((s) => s.settings.quickAccess.some((b) => b.action.kind === "explore"));
  const header = headerMetrics(geometry, hasRow);
  const insets = contentInsets(geometry, size);
  const contentWidth = Math.max(0, size.width - 2 * insets.horizontal);
  const contentHeight = Math.max(0, size.height - header.pageTopInset - insets.bottom);
  const page = pages[selected];

  let leading: ReactNode = null;
  if (showingSections) {
    leading = (
      <>
        <IconButton icon={ChevronLeft} label={t("island.back")} onClick={() => useIsland.getState().toggleSections()} />
        <SectionSearch />
      </>
    );
  } else if (page?.Leading) {
    leading = <page.Leading />;
  } else if (!hasExploreBubble) {
    leading = (
      <IconButton
        icon={LayoutGrid}
        label={t("island.sections")}
        onClick={() => useIsland.getState().toggleSections()}
      />
    );
  }
  const Trailing = !showingSections ? page?.Trailing : undefined;

  return (
    <div className="n-expanded" style={{ width: size.width, height: size.height }}>
      {header.headerRowHeight > 0 ? (
        <div
          className="n-row"
          style={{
            top: header.headerTopInset,
            height: header.headerRowHeight,
            left: insets.horizontal,
            right: insets.horizontal,
          }}
        >
          <div className="n-row-leading">{leading}</div>
          <div className="n-row-trailing">
            {Trailing ? <Trailing /> : null}
            {pinned ? <Pin className="n-pin-mark" size={11} /> : null}
          </div>
        </div>
      ) : null}
      <div
        className="n-page"
        style={{ top: header.pageTopInset, left: insets.horizontal, width: contentWidth, height: contentHeight }}
      >
        {showingSections ? (
          <SectionsGallery width={contentWidth + 56} height={contentHeight} />
        ) : page ? (
          <page.Page width={contentWidth} height={contentHeight} />
        ) : (
          <PlaceholderPage module={selected} />
        )}
      </div>
    </div>
  );
}

function PlaceholderPage({ module }: { module: ModuleId }) {
  return <EmptyState icon={moduleInfo(module).icon} title={t(`module.${module}`)} detail={t("island.comingSoon")} />;
}

/** A magnifier that unfolds into a field on hover or typing (ease-out 150 ms, up to 150 pt). */
function SectionSearch() {
  const query = useIsland((s) => s.sectionQuery);
  const setQuery = useIsland((s) => s.setSectionQuery);
  const [open, setOpen] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const unfolded = open || query.length > 0;

  // Typing anywhere on the gallery starts a search.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.altKey || e.metaKey || e.key.length !== 1 || document.activeElement === input.current) return;
      setOpen(true);
      input.current?.focus();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <label
      className={`n-search${unfolded ? " is-open" : ""}`}
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(query.length > 0 || document.activeElement === input.current)}
    >
      <Search size={13} strokeWidth={2} />
      <input
        ref={input}
        value={query}
        placeholder={t("island.search")}
        aria-label={t("island.search")}
        onChange={(e) => setQuery(e.target.value)}
        onBlur={() => setOpen(query.length > 0)}
      />
    </label>
  );
}
