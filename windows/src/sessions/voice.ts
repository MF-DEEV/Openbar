// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Voice replies (VoiceReplyRecorder.swift on the Mac). The Rust side listens
// and transcribes on this PC (src-tauri/src/voice); here is when to stop and
// what the words do: they fill the reply field as they come, Escape gives the
// field back, quiet after speech or the 60 s cap ends it, and "Send when I
// stop talking" sends them by the typed-reply path.

import { create } from "zustand";
import { bridge, type VoiceProblem } from "../shared/bridge";
import { isWaiting, type ReplyFailure, type Session } from "./types";
import { useSessions } from "./store";
import { getLanguage } from "../i18n";
import { voiceHeard } from "../shared/platform";

export type VoiceEnding = "stopped" | "silence" | "cap" | "cancelled" | "sessionGone" | "islandClosed";
export type VoicePhase =
  { kind: "idle" } | { kind: "preparing" } | { kind: "recording" } | { kind: "finishing"; ending: VoiceEnding };

export const SILENCE_AFTER_SPEECH = 1.5;
export const CAP = 60;
/** Levels are 0…1, -60…-10 dBFS (voice/mod.rs level): above is a voice, below a quiet room. */
export const SPEECH_LEVEL = 0.35;
export const QUIET_LEVEL = 0.2;

/** Whether voice replies are on: chosen in Settings, and heard on this PC. */
export const voiceWanted = (prefs: { voiceReplies: boolean }) => voiceHeard() && prefs.voiceReplies;

/** Whether "Send when I stop talking" may send what was heard. */
export const sends = (ending: VoiceEnding) => ending === "stopped" || ending === "silence";
/** Whether the words heard stay in the field. */
export const keepsWords = (ending: VoiceEnding) => ending !== "cancelled" && ending !== "sessionGone";

export interface Machine {
  phase: VoicePhase;
  sessionId: string | null;
  /** The field before recording, which Escape gives back. */
  snapshot: string;
  transcript: string;
  heardSpeech: boolean;
  startedAt: number;
  quietSince: number | null;
  sendOnStop: boolean;
}

/** What has to happen after an event. */
export interface Effects {
  draft?: string;
  /** Close the microphone; the last words follow. */
  finish?: boolean;
  /** Drop the recording without its last words. */
  abort?: boolean;
  send?: string;
}

export const IDLE: Machine = {
  phase: { kind: "idle" },
  sessionId: null,
  snapshot: "",
  transcript: "",
  heardSpeech: false,
  startedAt: 0,
  quietSince: null,
  sendOnStop: false,
};

const reset = (m: Machine): Machine => ({ ...IDLE, sendOnStop: m.sendOnStop });
export const isActive = (m: Machine) => m.phase.kind !== "idle";

/** What was in the field, then the words heard, with one space between. */
export function compose(before: string, words: string): string {
  const said = words.trim();
  if (!said) return before;
  if (!before) return said;
  return /\s$/.test(before) ? before + said : `${before} ${said}`;
}

export function start(m: Machine, sessionId: string, draft: string, sendOnStop: boolean): Machine | null {
  if (isActive(m)) return null;
  return { ...IDLE, phase: { kind: "preparing" }, sessionId, snapshot: draft, sendOnStop };
}

export function began(m: Machine, at: number): Machine {
  return m.phase.kind === "preparing" ? { ...m, phase: { kind: "recording" }, startedAt: at } : m;
}

export function partial(m: Machine, text: string): [Machine, Effects] {
  if (m.phase.kind !== "recording" && m.phase.kind !== "finishing") return [m, {}];
  const next = text && text !== m.transcript ? { ...m, heardSpeech: true, quietSince: null } : { ...m };
  next.transcript = text;
  return [next, { draft: compose(m.snapshot, text) }];
}

export function end(m: Machine, ending: VoiceEnding): [Machine, Effects] {
  const drop = (): [Machine, Effects] => [
    reset(m),
    { abort: true, ...(ending === "cancelled" ? { draft: m.snapshot } : {}) },
  ];
  switch (m.phase.kind) {
    case "idle":
      return [m, {}];
    case "preparing":
      // A permission prompt can take the island's focus; the press waits for its answer.
      return ending === "islandClosed" ? [m, {}] : drop();
    case "finishing":
      return !keepsWords(ending) && keepsWords(m.phase.ending) ? drop() : [m, {}];
    case "recording":
      if (!keepsWords(ending)) return drop();
      return [{ ...m, phase: { kind: "finishing", ending } }, { finish: true }];
  }
}

export function tick(m: Machine, level: number, at: number): [Machine, Effects] {
  if (m.phase.kind !== "recording") return [m, {}];
  if (at - m.startedAt >= CAP) return end(m, "cap");
  if (level >= SPEECH_LEVEL) return [{ ...m, heardSpeech: true, quietSince: null }, {}];
  if (level < QUIET_LEVEL && m.heardSpeech) {
    const since = m.quietSince ?? at;
    const quiet = { ...m, quietSince: since };
    return at - since >= SILENCE_AFTER_SPEECH ? end(quiet, "silence") : [quiet, {}];
  }
  return [m, {}];
}

export function finished(m: Machine, text: string | null): [Machine, Effects] {
  if (m.phase.kind !== "finishing") return [m, {}];
  const transcript = text ? text : m.transcript;
  const draft = compose(m.snapshot, transcript);
  const send = m.sendOnStop && sends(m.phase.ending) && transcript.trim() ? draft : undefined;
  return [reset(m), { draft, ...(send ? { send } : {}) }];
}

export function failed(m: Machine): [Machine, Effects] {
  const keep = m.phase.kind === "recording" || m.phase.kind === "finishing";
  return [reset(m), { abort: true, ...(keep ? { draft: compose(m.snapshot, m.transcript) } : {}) }];
}

/** The same rule as the reply field: replies reach it, and it waits on nothing else. */
export function canSpeak(session: Session): boolean {
  return session.acceptsReplies && !isWaiting(session.phase) && !session.waitingFor && !session.background?.ended;
}

/** The microphone shows where replies reach a session at all. */
export function showsMicrophone(session: Session, enabled: boolean): boolean {
  return enabled && session.acceptsReplies && !session.background?.ended;
}

/** The session a held shortcut speaks to: the page on screen, else the one last at work. */
export function shortcutTarget(sessions: Session[], showing: string | null): Session | null {
  const shown = showing ? sessions.find((s) => s.id === showing) : undefined;
  if (shown) return canSpeak(shown) ? shown : null;
  let best: Session | null = null;
  for (const session of sessions) {
    if (!canSpeak(session)) continue;
    if (!best || activity(session) > activity(best)) best = session;
  }
  return best;
}

const activity = (s: Session) => s.lastActive ?? s.phaseSince ?? 0;

// MARK: The store

export type VoiceIssue = VoiceProblem | { sendFailed: ReplyFailure };

interface VoiceState {
  machine: Machine;
  level: number;
  /** Why the last try failed, for that session's field. */
  problem: { sessionId: string; issue: VoiceIssue } | null;
  toggle(session: Session): void;
  start(session: Session): void;
  end(ending: VoiceEnding): void;
  /** Escape; false when nothing was recording. */
  cancel(): boolean;
  clearProblem(): void;
}

const seconds = () => performance.now() / 1000;
let recording: number | null = null;
let ticker: number | null = null;
let finishTimer: number | null = null;
/** Bumped on every start and end, so late answers can't reach the next recording. */
let generation = 0;
let latestLevel = 0;

export const useVoice = create<VoiceState>()((set, get) => {
  const apply = (next: Machine, effects: Effects, sessionId: string | null) => {
    set({ machine: next });
    const sessions = useSessions.getState();
    const live = sessionId ? sessions.snapshot.sessions.find((s) => s.id === sessionId) : undefined;
    if (effects.draft !== undefined && live) sessions.setDraft(live.id, effects.draft);
    if (effects.finish) {
      stopTicker();
      if (recording !== null) void bridge.voice.stop(recording);
      const token = generation;
      // The last words, or the last partial if the recognizer says nothing.
      finishTimer = window.setTimeout(() => {
        if (token !== generation) return;
        const [after, done] = finished(get().machine, null);
        apply(after, done, sessionId);
      }, 15000);
    }
    if (effects.abort && recording !== null) void bridge.voice.cancel(recording);
    if (!isActive(next)) tearDown();
    if (effects.send && live) {
      const text = effects.send;
      void sessions.reply(live, text, []).then((failure) => {
        if (failure) set({ problem: { sessionId: live.id, issue: { sendFailed: failure } } });
      });
    }
  };

  const stopTicker = () => {
    if (ticker !== null) window.clearInterval(ticker);
    ticker = null;
  };

  const tearDown = () => {
    stopTicker();
    if (finishTimer !== null) window.clearTimeout(finishTimer);
    finishTimer = null;
    recording = null;
    generation += 1;
    latestLevel = 0;
    set({ level: 0 });
  };

  const fail = (sessionId: string, issue: VoiceIssue) => {
    const [next, effects] = failed(get().machine);
    apply(next, effects, sessionId);
    set({ problem: { sessionId, issue } });
  };

  void bridge.voice.onLevel(({ id, level }) => {
    if (id === recording) latestLevel = Math.max(latestLevel, level);
  });
  void bridge.voice.onPartial(({ id, text }) => {
    if (id !== recording) return;
    const m = get().machine;
    const [next, effects] = partial(m, text);
    apply(next, effects, m.sessionId);
  });
  void bridge.voice.onFinal(({ id, text }) => {
    if (id !== recording) return;
    const m = get().machine;
    const [next, effects] = finished(m, text);
    apply(next, effects, m.sessionId);
  });
  void bridge.voice.onError(({ id, problem }) => {
    if (id !== recording) return;
    const m = get().machine;
    if (m.sessionId) fail(m.sessionId, problem);
  });

  return {
    machine: IDLE,
    level: 0,
    problem: null,
    toggle(session) {
      const m = get().machine;
      if (m.sessionId === session.id && isActive(m)) get().end("stopped");
      else get().start(session);
    },
    start(session) {
      const sessions = useSessions.getState();
      if (!voiceWanted(sessions.prefs) || !canSpeak(session)) return;
      const next = start(get().machine, session.id, sessions.drafts[session.id] ?? "", sessions.prefs.voiceSendOnStop);
      if (!next) return;
      generation += 1;
      const token = generation;
      set({ machine: next, problem: null });
      const language = sessions.prefs.voiceLanguage || getLanguage();
      bridge.voice
        .start(language)
        .then((id) => {
          if (token !== generation) {
            if (id !== undefined) void bridge.voice.cancel(id);
            return;
          }
          if (id === undefined) return fail(session.id, "noMicrophone");
          recording = id;
          set({ machine: began(get().machine, seconds()) });
          ticker = window.setInterval(() => {
            const level = latestLevel;
            latestLevel = 0;
            set({ level });
            const m = get().machine;
            const [after, effects] = tick(m, level, seconds());
            if (after !== m || Object.keys(effects).length) apply(after, effects, m.sessionId);
          }, 100);
        })
        .catch((problem: unknown) => {
          if (token === generation)
            fail(session.id, typeof problem === "string" ? (problem as VoiceProblem) : "failed");
        });
    },
    end(ending) {
      const m = get().machine;
      if (!isActive(m)) return;
      const [next, effects] = end(m, ending);
      apply(next, effects, m.sessionId);
    },
    cancel() {
      if (!isActive(get().machine)) return false;
      get().end("cancelled");
      return true;
    },
    clearProblem: () => set({ problem: null }),
  };
});

/** The session ending, another page or a closed island stops a recording. */
export function watchVoice(isOpenOnSessions: () => boolean, subscribe: (fn: () => void) => () => void): () => void {
  const check = () => {
    const voice = useVoice.getState();
    const id = voice.machine.sessionId;
    if (!id || !isActive(voice.machine)) return;
    const sessions = useSessions.getState();
    if (!sessions.snapshot.sessions.some((s) => s.id === id)) voice.end("sessionGone");
    else if (sessions.focused !== id || !isOpenOnSessions()) voice.end("islandClosed");
  };
  const off = [useSessions.subscribe(check), subscribe(check)];
  return () => off.forEach((f) => f());
}

// MARK: Hold to talk

let heldByShortcut = false;

/** The shortcut went down: the page on screen, or the session last at work, which the island opens on. */
export function voiceKeyPressed(island: { showing: () => string | null; openOn: (id: string) => void }) {
  const voice = useVoice.getState();
  const sessions = useSessions.getState();
  if (!voiceWanted(sessions.prefs) || isActive(voice.machine)) return;
  const showing = island.showing();
  const target = shortcutTarget(sessions.snapshot.sessions, showing);
  if (!target) return;
  if (showing !== target.id) island.openOn(target.id);
  heldByShortcut = true;
  voice.start(target);
}

export function voiceKeyReleased() {
  if (!heldByShortcut) return;
  heldByShortcut = false;
  useVoice.getState().end("stopped");
}

/** Holds the shortcut's keys while voice replies and the shortcut are on. */
export function subscribeVoiceShortcut(island: {
  showing: () => string | null;
  openOn: (id: string) => void;
}): () => void {
  let registered: string | null = null;
  let queue = Promise.resolve();
  const wanted = () => {
    const p = useSessions.getState().prefs;
    return voiceWanted(p) && p.voiceShortcutEnabled && p.voiceShortcut ? p.voiceShortcut : null;
  };
  const sync = () => {
    const next = wanted();
    queue = queue
      .then(async () => {
        if (next === registered) return;
        if (registered) await bridge.voice.unregisterKey(registered);
        registered = null;
        if (!next) return;
        await bridge.voice.registerKey(next, (event) => {
          if (event.state === "Pressed") voiceKeyPressed(island);
          else voiceKeyReleased();
        });
        registered = next;
      })
      .catch((error) => console.warn("Could not hold the voice reply shortcut", error));
  };
  const off = useSessions.subscribe((s, before) => {
    if (s.prefs !== before.prefs) sync();
  });
  sync();
  return () => {
    off();
    const held = registered;
    registered = null;
    if (held) void bridge.voice.unregisterKey(held);
  };
}
