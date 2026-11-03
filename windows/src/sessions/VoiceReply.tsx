// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The microphone beside a reply field (NotchVoiceReplyViews.swift on the
// Mac): pressed, it listens and grows into a level meter with a stop button.

import { Loader2, Mic, MicOff, Square } from "lucide-react";
import { useReducedMotion } from "motion/react";
import { t } from "../i18n";
import { bridge } from "../shared/bridge";
import { S } from "./strings";
import type { Session } from "./types";
import { isActive, useVoice, type VoiceIssue } from "./voice";

const BARS = [0.45, 0.75, 1, 0.75, 0.45];

export function VoiceMeter({ level, height }: { level: number; height: number }) {
  const still = useReducedMotion() ?? false;
  return (
    <span className={`s-voice-meter${still ? " is-still" : ""}`} style={{ height }} aria-hidden>
      {BARS.map((weight, index) => (
        <span
          key={index}
          style={
            still
              ? { height: height * 0.7, opacity: level * 5 >= index + 0.5 ? 1 : 0.3 }
              : { height: Math.max(2, height * weight * Math.max(0.18, Math.min(1, level * 1.4))) }
          }
        />
      ))}
    </span>
  );
}

export function VoiceButton({ session, disabled, large }: { session: Session; disabled: boolean; large?: boolean }) {
  const machine = useVoice((s) => s.machine);
  const level = useVoice((s) => s.level);
  const mine = machine.sessionId === session.id && isActive(machine);
  const listening = mine && machine.phase.kind === "recording";
  const label = listening ? t("windows.sessions.voiceStop") : t("windows.sessions.voiceStart");
  const icon = large ? 15 : 12;
  return (
    <button
      type="button"
      className={`s-mic${mine ? " is-active" : ""}${listening ? " is-listening" : ""}`}
      aria-label={label}
      aria-pressed={mine}
      title={listening ? `${label} · ${t("windows.sessions.voiceEscCancels")}` : label}
      disabled={disabled && !mine}
      onClick={() => useVoice.getState().toggle(session)}
    >
      {listening ? <VoiceMeter level={level} height={large ? 14 : 11} /> : null}
      {mine && !listening ? (
        <Loader2 size={icon} className="s-mic-spin" />
      ) : listening ? (
        <Square size={icon - 3} fill="currentColor" strokeWidth={0} />
      ) : (
        <Mic size={icon} strokeWidth={2.4} />
      )}
    </button>
  );
}

function message(issue: VoiceIssue): string {
  if (typeof issue === "object") return issue.sendFailed === "waiting" ? S.sendWhileWaiting : S.sendFailed;
  switch (issue) {
    case "noMicrophone":
      return t("windows.sessions.voiceNoMicrophone");
    case "microphoneDenied":
      return t("windows.sessions.voiceMicrophoneDenied");
    case "modelMissing":
      return t("windows.sessions.voiceModelMissing");
    case "busy":
      return t("windows.sessions.voiceBusy");
    default:
      return t("windows.sessions.voiceFailed");
  }
}

function fix(issue: VoiceIssue): (() => void) | null {
  if (issue === "microphoneDenied") return () => void bridge.voice.openWindowsSettings("microphone");
  if (issue === "noMicrophone") return () => void bridge.voice.openWindowsSettings("sound");
  if (issue === "modelMissing") return () => void bridge.openSettings();
  return null;
}

/** Why a voice reply could not happen, under the field, with the page that fixes it. */
export function VoiceProblem({ session }: { session: Session }) {
  const problem = useVoice((s) => (s.problem?.sessionId === session.id ? s.problem.issue : null));
  if (!problem) return null;
  const action = fix(problem);
  return (
    <div className="s-error s-voice-problem" role="status">
      <MicOff size={10} />
      <span>{message(problem)}</span>
      {action ? (
        <button
          type="button"
          className="s-voice-fix"
          onClick={() => {
            useVoice.getState().clearProblem();
            action();
          }}
        >
          {t("windows.sessions.voiceOpenSettings")}
        </button>
      ) : null}
    </div>
  );
}
