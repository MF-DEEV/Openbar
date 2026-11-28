// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The sections gallery (NotchSectionsView.swift; spec 01-shell §5): tiles
// 92 × 86 at least, 8 apart, radius 16, three rows in view, whole-row paging
// with a dot column on the right.

import { useLocale, t } from "../../i18n";

import { Check } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { moduleInfo, type ModuleId } from "../modules";
import { galleryColumns, modulesOf, useIsland } from "../store";

const ROW = 86;
const GAP = 8;

export function SectionsGallery(props: { width: number; height: number }) {
  const language = useLocale();
  const settings = useIsland((s) => s.settings);
  const selected = useIsland((s) => s.selected);
  const query = useIsland((s) => s.sectionQuery);
  const select = useIsland((s) => s.select);
  const modules = useMemo(() => modulesOf({ settings }), [settings]);
  const shown = useMemo(() => {
    const q = query.trim().toLocaleLowerCase(language);
    return q ? modules.filter((id) => t(`module.${id}`).toLocaleLowerCase(language).includes(q)) : modules;
  }, [modules, query, language]);
  const columns = galleryColumns(props.width);
  const visibleRows = Math.max(1, Math.min(3, Math.floor((props.height + GAP) / (ROW + GAP))));
  const rows = Math.ceil(shown.length / columns);
  const [row, setRow] = useState(0);
  const [highlight, setHighlight] = useState(0);
  const pages = Math.max(1, rows - visibleRows + 1);

  useEffect(() => {
    setHighlight(0);
    setRow(0);
  }, [query]);

  useEffect(() => setRow((current) => Math.min(current, pages - 1)), [pages]);

  // Arrows move the highlight in grid order, Return opens it (spec §2.6).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.altKey || e.metaKey) return;
      let next = highlight;
      if (e.key === "ArrowRight" || (e.key === "Tab" && !e.shiftKey)) next += 1;
      else if (e.key === "ArrowLeft" || (e.key === "Tab" && e.shiftKey)) next -= 1;
      else if (e.key === "ArrowDown") next += columns;
      else if (e.key === "ArrowUp") next -= columns;
      else if (e.key === "Enter") {
        const id = shown[highlight];
        if (id) select(id);
        e.preventDefault();
        return;
      } else return;
      e.preventDefault();
      next = Math.max(0, Math.min(shown.length - 1, next));
      setHighlight(next);
      const r = Math.floor(next / columns);
      setRow((current) => (r < current ? r : r >= current + visibleRows ? r - visibleRows + 1 : current));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [highlight, columns, shown, select, visibleRows]);

  // A wheel notch steps one whole row.
  const onWheel = (e: React.WheelEvent) => {
    if (Math.abs(e.deltaY) < 4) return;
    setRow((r) => Math.max(0, Math.min(pages - 1, r + (e.deltaY > 0 ? 1 : -1))));
  };

  return (
    <div className="n-gallery" onWheel={onWheel}>
      <div
        className="n-gallery-viewport"
        style={{ height: Math.min(props.height, Math.max(0, Math.min(rows, visibleRows) * (ROW + GAP) - GAP)) }}
      >
        <div
          className="n-gallery-grid"
          style={{
            gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
            transform: `translateY(${-row * (ROW + GAP)}px)`,
          }}
        >
          {shown.map((id, index) => (
            <Tile key={id} id={id} current={id === selected} highlighted={index === highlight} onSelect={select} />
          ))}
        </div>
      </div>
      {pages > 1 ? (
        <div className="n-gallery-dots">
          {Array.from({ length: pages }, (_, i) => (
            <button
              key={i}
              type="button"
              className={`n-gallery-dot${i === row ? " is-current" : ""}`}
              aria-label={`${i + 1}`}
              onClick={() => setRow(i)}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}

function Tile(props: { id: ModuleId; current: boolean; highlighted: boolean; onSelect: (id: ModuleId) => void }) {
  useLocale();
  const info = moduleInfo(props.id);
  const Icon = info.icon;
  return (
    <button
      type="button"
      className={`n-tile${props.highlighted ? " is-highlighted" : ""}`}
      onClick={() => props.onSelect(props.id)}
    >
      {props.current ? <Check className="n-tile-check" size={10} strokeWidth={3} /> : null}
      <Icon size={22} strokeWidth={1.75} style={{ color: info.tint ?? "rgba(255,255,255,0.85)" }} />
      <span className="n-tile-title">{t(`module.${props.id}`)}</span>
      <span className="n-tile-shortcut">
        {t("windows.island.ctrlAlt")}
        {info.shortcutKey.toUpperCase()}
      </span>
    </button>
  );
}
