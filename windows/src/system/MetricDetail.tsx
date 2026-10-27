// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A metric's detail in the island (MetricDetailView.swift, notch
// presentation): a back button where the Mac's band has it, the summary with
// its history graph, the readings, and for CPU and memory the processes using
// the most. Escape steps back to the cards.

import { Bluetooth, ChevronLeft, Usb } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { usePageLayer } from "../island/pageLayer";
import { useIsland } from "../island/store";
import { bridge } from "../shared/bridge";
import { detailRows, processValue, summary, TITLES } from "./detail";
import { Sparkline } from "./Sparkline";
import { useSystem } from "./store";
import { GLYPHS } from "./glyphs";
import { S } from "./strings";
import type { MetricKind, ProcessRow } from "./types";

/** The back row's height and the gap under it. */
export const DETAIL_BAR = 36 + 8;

/** `onMeasure` gets the readings' height, so a short detail keeps a short island (as the Mac's fan detail). */
export function MetricDetail({ kind, onMeasure }: { kind: MetricKind; onMeasure(height: number): void }) {
  const snapshot = useSystem((s) => s.snapshot);
  const history = useSystem((s) => s.history);
  const close = useSystem((s) => s.closeDetail);
  const inBits = useIsland((s) => s.settings.networkSpeedUnit === "bits");
  usePageLayer(true, close);
  const content = useRef<HTMLDivElement>(null);
  const measure = useRef(onMeasure);
  measure.current = onMeasure;
  useLayoutEffect(() => {
    const el = content.current;
    if (!el) return;
    measure.current(el.offsetHeight);
    const observer = new ResizeObserver(() => measure.current(el.offsetHeight));
    observer.observe(el);
    return () => observer.disconnect();
  }, [snapshot === null]);
  if (!snapshot) return null;
  const title = TITLES[kind];
  const Glyph = GLYPHS[kind];
  const head = summary(kind, snapshot, history, inBits);
  const rows = detailRows(kind, snapshot, inBits);
  const lists = kind === "cpu" || kind === "memory";
  return (
    <div className="sy-detail">
      <div className="sy-detail-bar">
        <button type="button" className="sy-back" aria-label={`${S.back}: ${title}`} title={S.back} onClick={close}>
          <ChevronLeft size={14} strokeWidth={2.5} aria-hidden />
          <span>{title}</span>
        </button>
      </div>
      <div className="sy-detail-body">
        <div ref={content} className="sy-detail-content">
          <section className="sy-panel sy-summary">
            <div className="sy-summary-head">
              <Glyph size={16} strokeWidth={2.2} style={{ color: head.color }} aria-hidden />
              <div>
                <div className="sy-summary-value">{head.primary}</div>
                <div className="sy-summary-line">{head.secondary}</div>
              </div>
            </div>
            {head.graph ? (
              <Sparkline lines={head.graph.lines} max={head.graph.max} ceilingLabel={head.graph.label} />
            ) : null}
          </section>
          <section className="sy-panel sy-rows">
            {rows.map((row, i) => (
              <div key={`${row.title}-${i}`} className="sy-row-line">
                {row.bluetooth === undefined ? null : row.bluetooth ? (
                  <Bluetooth size={12} aria-hidden />
                ) : (
                  <Usb size={12} aria-hidden />
                )}
                <span className="sy-row-title">{row.title}</span>
                <span className="sy-row-value">{row.value}</span>
              </div>
            ))}
          </section>
          {lists ? (
            <section className="sy-panel sy-rows">
              <div className="sy-panel-title">{S.usage}</div>
              {snapshot.processesFor !== kind || snapshot.processes.length === 0 ? (
                <div className="sy-panel-empty">{S.measuring}</div>
              ) : (
                snapshot.processes.map((row, i) => <Process key={`${i}-${row.name}`} kind={kind} row={row} />)
              )}
            </section>
          ) : null}
        </div>
      </div>
    </div>
  );
}

const icons = new Map<string, Promise<string | null>>();

/** The exe's icon, asked of the shell once per path. */
function useIcon(path: string | null): string | null {
  const [icon, setIcon] = useState<string | null>(null);
  useEffect(() => {
    if (!path) return;
    let live = true;
    const size = Math.round(16 * window.devicePixelRatio);
    const key = `${size}|${path}`;
    if (!icons.has(key))
      icons.set(
        key,
        bridge.shell
          .icon(path, size)
          .then((url) => url ?? null)
          .catch(() => null),
      );
    void icons.get(key)!.then((url) => live && setIcon(url));
    return () => {
      live = false;
    };
  }, [path]);
  return icon;
}

function Process({ kind, row }: { kind: MetricKind; row: ProcessRow }) {
  const icon = useIcon(row.path);
  return (
    <div className="sy-row-line sy-process" title={row.name}>
      {icon ? <img src={icon} alt="" className="sy-process-icon" /> : <span className="sy-process-icon" />}
      <span className="sy-row-title">{row.name}</span>
      <span className="sy-row-value">{processValue(kind, row.value)}</span>
    </div>
  );
}
