// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A session's task list (#215): a quiet reading on its row, the list itself
// on its page. Sessions without a list draw nothing here.

import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { cloneElement, type CSSProperties, isValidElement, type ReactNode } from "react";

/** The mark's size inside the strip's ring: even, like the ring's 18 px. */
const STRIP_RING_MARK = 10;
import { presentedSnapshot } from "../presentation";
import { S } from "../strings";
import { useSessions } from "../store";
import { providerTint } from "../format";
import type { Session, TaskItem, Tasks } from "../types";

export const hasTasks = (s: Session): s is Session & { tasks: Tasks } => !!s.tasks && s.tasks.total > 0;
export const tasksDone = (t: Tasks) => t.total > 0 && t.done >= t.total;

/** The list as items, from the items when the agent gave them, else made up
 *  from the counts so the dots still read right. */
export function itemsOf(t: Tasks): TaskItem[] {
  if (t.items?.length) return t.items;
  return Array.from({ length: t.total }, (_, i) => ({
    title: i === t.done ? (t.current ?? "") : "",
    state: i < t.done ? "done" : i === t.done ? "current" : "pending",
  }));
}

/** Where the agent's badge sits on the ring, clockwise from 3 o'clock, and
 *  the gap the ring leaves round it, wide enough to read as apart at a glance. */
export const BADGE_ANGLE = 39.6;
export const BADGE_GAP = 100;

/** A ring drawn round the row's Clawd, open where the agent's badge sits and
 *  filled to the share done, from the end of the gap on. */
export function AvatarRing({ session }: { session: Session }) {
  const look = useSessions((s) => s.prefs.progressStyle);
  if (look !== "ring" || !hasTasks(session)) return null;
  const t = session.tasks;
  const fraction = Math.min(1, t.done / t.total);
  const done = tasksDone(t);
  const span = (360 - BADGE_GAP) / 360;
  const start = BADGE_ANGLE + BADGE_GAP / 2;
  return (
    <svg
      className={`p-avatar-ring${done ? " is-done" : ""}`}
      viewBox="0 0 44 44"
      aria-hidden
      style={{ "--p-tint": providerTint(session.provider) } as CSSProperties}
    >
      <circle
        cx="22"
        cy="22"
        r="20"
        className="p-ring-track"
        pathLength="1"
        strokeDasharray={`${span} 1`}
        strokeLinecap="round"
        transform={`rotate(${start} 22 22)`}
      />
      <circle
        cx="22"
        cy="22"
        r="20"
        className="p-ring-fill"
        pathLength="1"
        strokeDasharray={`${span * fraction} 1`}
        transform={`rotate(${start} 22 22)`}
      />
    </svg>
  );
}

/** "4/9" on the row's first line, the number rolling as it moves on. */
export function TaskCount({ session }: { session: Session }) {
  const look = useSessions((s) => s.prefs.progressStyle);
  const reduce = useReducedMotion();
  if (!hasTasks(session)) return null;
  const t = session.tasks;
  const done = tasksDone(t);
  if (look === "dots") return <TaskDots session={session} />;
  return (
    <span className={`p-count${done ? " is-done" : ""}`} aria-label={S.tasks(t.done, t.total)}>
      {done ? (
        <CheckMark />
      ) : (
        <span className="p-count-roll">
          <AnimatePresence initial={false} mode="popLayout">
            <motion.span
              key={t.done}
              initial={reduce ? false : { y: 8, opacity: 0 }}
              animate={{ y: 0, opacity: 1 }}
              exit={reduce ? undefined : { y: -8, opacity: 0 }}
              transition={{ type: "spring", stiffness: 420, damping: 30 }}
            >
              {t.done}
            </motion.span>
          </AnimatePresence>
        </span>
      )}
      {done ? null : <span className="p-count-total">/{t.total}</span>}
    </span>
  );
}

/** One dot per task: done ones filled, the current one breathing. */
function TaskDots({ session }: { session: Session }) {
  const t = session.tasks!;
  const items = itemsOf(t);
  const shown = items.slice(0, 12);
  return (
    <span
      className="p-dots"
      style={{ "--p-tint": providerTint(session.provider) } as CSSProperties}
      aria-label={S.tasks(t.done, t.total)}
    >
      {shown.map((item, i) => (
        <span key={i} className={`p-dot is-${item.state}`} />
      ))}
      {items.length > shown.length ? <span className="p-dots-more">+{items.length - shown.length}</span> : null}
    </span>
  );
}

/** A 2 px bar along the row's bottom edge. */
export function RowBar({ session }: { session: Session }) {
  const look = useSessions((s) => s.prefs.progressStyle);
  if (look !== "bar" || !hasTasks(session)) return null;
  const t = session.tasks;
  return (
    <span className="p-row-bar" style={{ "--p-tint": providerTint(session.provider) } as CSSProperties} aria-hidden>
      <span style={{ width: `${Math.min(1, t.done / t.total) * 100}%` }} />
    </span>
  );
}

function CheckMark() {
  return (
    <svg className="p-check" viewBox="0 0 12 12" width="11" height="11" aria-hidden>
      <path d="M2.5 6.4 5 8.8 9.6 3.4" pathLength="1" />
    </svg>
  );
}

/** The list on the session's page: done items ticked, the current one lit,
 *  the rest waiting in gray. */
export function Checklist({ session }: { session: Session }) {
  const reduce = useReducedMotion();
  if (!hasTasks(session)) return null;
  const t = session.tasks;
  const items = itemsOf(t);
  const done = tasksDone(t);
  const tint = providerTint(session.provider);
  // A long list shows where it is: the last one done, the current one and
  // the next few; the rest fold into a count at either end.
  const at = Math.min(t.done, items.length - 1);
  const from = items.length > 4 ? Math.max(0, Math.min(at - 1, items.length - 4)) : 0;
  const to = items.length > 4 ? from + 4 : items.length;
  const shown = items.slice(from, to).map((item, k) => ({ item, i: from + k }));
  return (
    <div className={`p-list${done ? " is-done" : ""}`} style={{ "--p-tint": tint } as CSSProperties}>
      <div className="p-list-head">
        <span className="p-segments" aria-hidden>
          {items.map((item, i) => (
            <span key={i} className={`p-segment is-${item.state}`} />
          ))}
        </span>
        <span className="p-list-count">{done ? <CheckMark /> : `${t.done}/${t.total}`}</span>
      </div>
      <ul className="p-items">
        {shown.map(({ item, i }) => (
          <motion.li
            key={i}
            layout={!reduce}
            className={`p-item is-${item.state}`}
            initial={false}
            transition={{ type: "spring", stiffness: 380, damping: 32 }}
          >
            <span className="p-item-mark" aria-hidden>
              {item.state === "done" ? <CheckMark /> : item.state === "current" ? <span className="p-spin" /> : null}
            </span>
            <span className="p-item-title">{item.title}</span>
          </motion.li>
        ))}
      </ul>
    </div>
  );
}

/** The closed strip's mark, ringed by how far the working sessions' lists are. */
export function StripTasksRing({ children, off }: { children: ReactNode; off?: boolean }) {
  // One string, so the store hands back the same value while nothing moves.
  const reading = useSessions((s) => {
    const lists = presentedSnapshot(s).sessions.filter(
      (x) => hasTasks(x) && !tasksDone(x.tasks!) && x.phase !== "finished",
    );
    if (lists.length === 0) return "";
    const done = lists.reduce((n, x) => n + x.tasks!.done, 0);
    const total = lists.reduce((n, x) => n + x.tasks!.total, 0);
    return `${done}/${total}/${lists[0]!.provider}`;
  });
  if (off || !reading) return <>{children}</>;
  const [d, t, provider] = reading.split("/");
  const done = Number(d);
  const total = Number(t);
  const tint = providerTint(provider as Session["provider"]);
  return (
    <span className="p-strip-ring" style={{ "--p-tint": tint } as CSSProperties}>
      <span className="p-strip-mark">
        {isValidElement<{ size?: number }>(children) ? cloneElement(children, { size: STRIP_RING_MARK }) : children}
      </span>
      <svg className="p-strip-arc" viewBox="0 0 24 24" aria-hidden>
        <circle cx="12" cy="12" r="10.5" className="p-ring-track" style={{ strokeWidth: 2.2 }} />
        <circle
          cx="12"
          cy="12"
          r="10.5"
          className="p-ring-fill"
          style={{ strokeWidth: 2.2 }}
          pathLength="1"
          strokeDasharray={`${done / total} 1`}
          transform="rotate(-90 12 12)"
        />
      </svg>
    </span>
  );
}
