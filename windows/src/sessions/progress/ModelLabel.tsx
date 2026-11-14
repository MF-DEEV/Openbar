// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Which model a session runs on, and how hard it thinks (#215): a quiet chip
// whose name rolls over when the model is switched.

import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { modelName } from "../../agents/format";
import type { Session } from "../types";

/** "Opus" from "claude-opus-5-5", "GPT-5.5" from "gpt-5.5-codex": the family only. */
export function shortModel(session: Pick<Session, "model" | "modelName">): string | null {
  const raw = session.model;
  if (!raw || raw.startsWith("<") || raw === "inherit") return null;
  const full = session.modelName && session.modelName !== raw ? session.modelName : modelName(raw);
  if (!full) return null;
  if (raw.startsWith("claude-")) return full.split(" ")[0] ?? full;
  return full.replace(/[ -]codex$/i, "");
}

const LEVELS: Record<string, number> = { minimal: 1, low: 1, medium: 2, high: 3, xhigh: 4, max: 4 };
/** 1 to 4 bars for the level, none when the agent names none. */
export const effortLevel = (effort?: string) => (effort ? (LEVELS[effort.toLowerCase()] ?? 0) : 0);

export function ModelLabel({ session }: { session: Session }) {
  const reduce = useReducedMotion();
  const name = shortModel(session);
  if (!name) return null;
  const level = effortLevel(session.effort);
  const label = session.effort ? `${name} · ${session.effort}` : name;
  return (
    <span className="p-model" aria-label={label} title={label}>
      <span className="p-model-roll">
        <AnimatePresence initial={false} mode="popLayout">
          <motion.span
            key={name}
            initial={reduce ? false : { y: 9, opacity: 0, filter: "blur(2px)" }}
            animate={{ y: 0, opacity: 1, filter: "blur(0px)" }}
            exit={reduce ? undefined : { y: -9, opacity: 0, filter: "blur(2px)" }}
            transition={{ type: "spring", stiffness: 380, damping: 28 }}
          >
            {name}
          </motion.span>
        </AnimatePresence>
      </span>
      {level > 0 ? <EffortBars level={level} /> : null}
    </span>
  );
}

/** One to four rising bars, like a signal reading. */
export function EffortBars({ level }: { level: number }) {
  return (
    <svg className="p-effort" viewBox="0 0 11 8" width="11" height="8" aria-hidden>
      {[0, 1, 2, 3].map((i) => (
        <rect
          key={i}
          x={i * 3}
          y={6 - i * 2}
          width="2"
          height={2 + i * 2}
          rx="0.6"
          className={i < level ? "is-on" : undefined}
        />
      ))}
    </svg>
  );
}
