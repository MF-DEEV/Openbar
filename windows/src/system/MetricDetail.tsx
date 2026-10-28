// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A metric's detail in the island (MetricDetailView.swift, notch
// presentation): a back button where the Mac's band has it, the summary with
// its history graph, the readings, for the network the speed test, and the
// supported process lists, with Task Manager a click away (the Mac's Activity
// Monitor button). Escape steps back to the cards.

import { Bluetooth, ChevronLeft, LoaderCircle, ShieldCheck, SquareArrowOutUpRight, Usb } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { usePageLayer } from "../island/pageLayer";
import { useIsland } from "../island/store";
import { bridge } from "../shared/bridge";
import { detailRows, processEmptyText, processRowValue, summary, TITLES } from "./detail";
import { Sparkline } from "./Sparkline";
import { SpeedTestCard } from "./SpeedTestCard";
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
  const lists = ["cpu", "memory", "gpu", "network", "power"].includes(kind);
  const emptyText = processEmptyText(kind, snapshot);
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
          {kind === "network" ? <SpeedTestCard /> : null}
          {lists ? (
            <section className="sy-panel sy-rows">
              <div className="sy-panel-head">
                <div className="sy-panel-title">
                  {kind === "network" ? S.networkApps : kind === "power" ? S.energyApps : S.usage}
                </div>
                <button
                  type="button"
                  className="sy-open-app"
                  aria-label={S.openTaskManager}
                  title={S.openTaskManager}
                  onClick={() => void bridge.shell.openTaskManager()}
                >
                  <SquareArrowOutUpRight size={10} strokeWidth={2.2} aria-hidden />
                </button>
              </div>
              {emptyText !== null ? (
                <div className="sy-panel-empty">
                  {emptyText}
                  {snapshot.processesFor === kind && snapshot.processStatus === "needsApproval" ? (
                    <NetworkApproval />
                  ) : null}
                </div>
              ) : (
                snapshot.processes.map((row, i) => (
                  <Process key={`${i}-${row.name}`} kind={kind} row={row} inBits={inBits} />
                ))
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
    setIcon(null);
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

/** Windows' prompt for the helper that reads per-app traffic (network_helper.rs). */
function NetworkApproval() {
  const [asking, setAsking] = useState(false);
  const allow = () => {
    setAsking(true);
    void bridge.system.approveNetwork().finally(() => setAsking(false));
  };
  return asking ? (
    <span className="sy-speed-testing sy-approve" role="status">
      <LoaderCircle className="sy-spin" size={13} aria-hidden />
      {S.networkApproving}
    </span>
  ) : (
    <button type="button" className="sy-speed-button sy-approve" onClick={allow}>
      <ShieldCheck size={12} strokeWidth={2.2} aria-hidden />
      {S.networkApprove}
    </button>
  );
}

function Process({ kind, row, inBits }: { kind: MetricKind; row: ProcessRow; inBits: boolean }) {
  const icon = useIcon(row.path);
  return (
    <div className="sy-row-line sy-process" title={row.name}>
      {icon ? <img src={icon} alt="" className="sy-process-icon" /> : <span className="sy-process-icon" />}
      <span className="sy-row-title">{row.name}</span>
      <span className="sy-row-value">{processRowValue(kind, row, inBits)}</span>
    </div>
  );
}
