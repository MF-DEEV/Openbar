// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { useLocale, t } from "../i18n";

import { create } from "zustand";
import { HeadphoneOff, Mic, MicOff } from "lucide-react";
import { bridge } from "../shared/bridge";
import { useIsland } from "../island/store";
import { reactMascot } from "../mascot/service";
import { edgeInset } from "../island/geometry";
import type { ActivityViewProps } from "../island/views/Activity";
import { audioState, EMPTY_MICROPHONE, type MicrophoneSnapshot } from "./model";
import "./microphone.css";

const useMicrophone = create<MicrophoneSnapshot>(() => EMPTY_MICROPHONE);

export function AudioIcons({ muted, deafened }: { muted: boolean; deafened: boolean }) {
  useLocale();
  return (
    <span className={`mic-icons${muted || deafened ? " is-muted" : ""}`}>
      {deafened ? <HeadphoneOff size={16} aria-hidden="true" /> : null}
      {muted ? <MicOff size={16} aria-hidden="true" /> : <Mic size={16} aria-hidden="true" />}
    </span>
  );
}

function label(muted: boolean, deafened: boolean) {
  return deafened ? t("notchVoice.deafened") : muted ? t("s.micMutedHUD") : t("notchVoice.unmuted");
}

function syncActivity() {
  const island = useIsland.getState();
  const state = audioState(useMicrophone.getState());
  const enabled = island.settings.enabled && island.settings.microphoneStatus;
  island.setActivity(
    "microphone",
    enabled && state.active ? { wing: 54, capsuleContent: 84, opens: "sessions" } : null,
  );
  if (!enabled && island.notice?.kind === "microphone") island.dismissNotice();
}

export async function subscribeMicrophone() {
  let version = 0;
  const receive = (snapshot: MicrophoneSnapshot, initial = false) => {
    const previous = useMicrophone.getState();
    const before = audioState(previous);
    const next = audioState(snapshot);
    useMicrophone.setState(snapshot);
    syncActivity();
    const island = useIsland.getState();
    // Losing access to Discord is unknown, not an unmute event.
    const sourceLost = previous.discord !== null && snapshot.discord === null;
    if (
      !initial &&
      !sourceLost &&
      (snapshot.recording || snapshot.discord !== null) &&
      island.settings.microphoneStatus &&
      island.settings.enabled &&
      (before.muted !== next.muted || before.deafened !== next.deafened)
    ) {
      island.showNotice({
        id: `microphone:${Date.now()}`,
        kind: "microphone",
        title: t("windows.microphone.title"),
        detail: label(next.muted, next.deafened),
        priority: 1,
        duration: 3,
        audio: next,
        mascot: next.muted ? "hush" : "perk",
      });
      reactMascot(next.muted ? "hush" : "perk");
    }
  };
  await bridge.microphone.onSnapshot((snapshot) => {
    version++;
    receive(snapshot);
  });
  const at = version;
  const first = await bridge.microphone.snapshot();
  if (first && at === version) receive(first, true);
}

useIsland.subscribe((state, previous) => {
  if (state.settings !== previous.settings) syncActivity();
});

export function MicrophoneStrip({ geometry, size }: ActivityViewProps) {
  useLocale();
  const state = audioState(useMicrophone());
  const text = label(state.muted, state.deafened);
  const padding = geometry.floatingGap === null ? edgeInset(size, 16, 3, 5, geometry.input) : 14;
  return (
    <button
      className={`mic-strip${state.muted ? " is-muted" : ""}`}
      style={{ width: size.width, height: size.height, paddingInline: padding }}
      aria-label={text}
      title={text}
      onClick={() => useIsland.getState().clickClosed()}
    >
      <AudioIcons muted={state.muted} deafened={state.deafened} />
      <span className="mic-muted-dot" aria-hidden="true" />
    </button>
  );
}
