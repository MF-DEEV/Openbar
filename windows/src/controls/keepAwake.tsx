// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Keep Awake: the home page's tile and, for those who turn it on, the
// closed island's strip (NotchKeepAwakeSupport.swift, NotchCapsuleViews.swift
// 674-732; spec 04 §7): a yellow cup and the time left, or ∞.

import { Coffee, Infinity as Endless } from "lucide-react";
import { useEffect, useState } from "react";
import { create } from "zustand";
import { edgeInset } from "../island/geometry";
import { useIsland } from "../island/store";
import type { ActivityViewProps } from "../island/views/Activity";
import { bridge } from "../shared/bridge";
import type { KeepAwakeState } from "../audio/types";
import { S } from "./strings";

export const useKeepAwake = create<{ state: KeepAwakeState }>(() => ({ state: { active: false, endsAt: null } }));

/** Whole minutes left, rounded up, at least 1 (NotchKeepAwakeSupport.minutesLeft). */
export function minutesLeft(endsAt: number, now: number): number {
  const minutes = Math.ceil((endsAt - now) / 60_000);
  return Number.isFinite(minutes) ? Math.max(1, minutes) : 1;
}

/** "45m", then the timer strip's "1h35" from an hour, past three hours too. */
export function keepAwakeText(endsAt: number, now: number): string {
  const minutes = minutesLeft(endsAt, now);
  if (minutes < 60) return `${minutes}m`;
  return `${Math.floor(minutes / 60)}h${String(minutes % 60).padStart(2, "0")}`;
}

/** Milliseconds until the reading next changes: the remaining time's next minute boundary. */
export function nextTick(endsAt: number, now: number): number {
  const left = endsAt - now;
  const into = left % 60_000;
  return into > 0 ? into : 60_000;
}

export async function toggleKeepAwake(): Promise<void> {
  const next = await bridge.keepAwake.toggle();
  if (next) useKeepAwake.setState({ state: next });
}

/** The wider side: the cup, or the time left at 13 pt with the strip's insets. */
const WING = 48;

function syncActivity() {
  const island = useIsland.getState();
  const { state } = useKeepAwake.getState();
  const shown = island.settings.enabled && island.settings.keepAwakeActivity && state.active;
  island.setActivity(
    "keepAwake",
    shown ? { wing: WING, capsuleContent: 14 + 6 + (state.endsAt === null ? 16 : 34), opens: "controls" } : null,
  );
}

export async function subscribeKeepAwake(): Promise<void> {
  let version = 0;
  await bridge.keepAwake.onState((state) => {
    version++;
    useKeepAwake.setState({ state });
  });
  const at = version;
  const first = await bridge.keepAwake.state();
  if (first && version === at) useKeepAwake.setState({ state: first });
}

useKeepAwake.subscribe(syncActivity);
useIsland.subscribe((s, prev) => {
  if (s.settings !== prev.settings) syncActivity();
});

/** The time left, re-read once a minute on the reading's own boundary. */
function useTimeLeft(endsAt: number | null): string | null {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (endsAt === null) return;
    const t = setTimeout(() => setNow(Date.now()), nextTick(endsAt, Date.now()) + 20);
    return () => clearTimeout(t);
  }, [endsAt, now]);
  return endsAt === null ? null : keepAwakeText(endsAt, now);
}

export function KeepAwakeStrip({ geometry, size }: ActivityViewProps) {
  const state = useKeepAwake((s) => s.state);
  const text = useTimeLeft(state.endsAt);
  const capsule = geometry.floatingGap !== null;
  const status = text ? `${S.keepAwakeEndsIn} ${text}` : S.keepAwakeUntilOff;
  const inset = capsule ? 12 : edgeInset(size, 14, 4, 5, geometry.input);
  const wing = capsule ? undefined : (size.width - geometry.cameraWidth) / 2;
  const reading = text ? (
    <span className="ct-strip-reading">{text}</span>
  ) : (
    <Endless size={15} strokeWidth={2.25} aria-hidden="true" />
  );
  return (
    <button
      type="button"
      className={`ct-strip${capsule ? " is-capsule" : ""}`}
      style={{ width: size.width, height: size.height, paddingInline: capsule ? inset : 0 }}
      aria-label={`${S.keepAwake}: ${status}`}
      title={status}
      onClick={() => useIsland.getState().open("controls")}
    >
      {capsule ? (
        <>
          <Coffee size={14} strokeWidth={2.25} aria-hidden="true" />
          {reading}
        </>
      ) : (
        <>
          <span className="ct-strip-wing" style={{ width: wing, paddingLeft: inset }}>
            <Coffee size={14} strokeWidth={2.25} aria-hidden="true" />
          </span>
          <span style={{ width: geometry.cameraWidth, flex: "none" }} />
          <span className="ct-strip-wing is-end" style={{ width: wing, paddingRight: inset }}>
            {reading}
          </span>
        </>
      )}
    </button>
  );
}
