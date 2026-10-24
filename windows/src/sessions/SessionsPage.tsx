// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Sessions section in the island: the list, or one session's page, and
// the island's top row for each (counts beside the list; back, rename,
// enlarge and open on a session's page).

import { Check, ChevronLeft, ExternalLink, Maximize2, Minimize2, Pencil } from "lucide-react";
import { useEffect, useState } from "react";
import type { PageDefinition } from "../island/views/Expanded";
import { useIsland } from "../island/store";
import { SessionDetail } from "./SessionDetail";
import { SessionsList } from "./SessionsList";
import { displayName } from "./format";
import { countItems } from "./SessionsStrip";
import { S } from "./strings";
import { useSessions } from "./store";
import "./sessions.css";

function useFocusedSession() {
  const focused = useSessions((s) => s.focused);
  return useSessions((s) => (focused ? s.snapshot.sessions.find((x) => x.id === focused) : undefined));
}

function Page() {
  const session = useFocusedSession();
  const focused = useSessions((s) => s.focused);
  // A session that ends while its page is open goes back to the list.
  useEffect(() => {
    if (focused && !session) useSessions.getState().focus(null);
  }, [focused, session]);
  return session ? <SessionDetail session={session} /> : <SessionsList />;
}

/** Counts with 9 pt symbols and 11 pt numbers, or the total when nothing is going on. */
function Counts() {
  const counts = useSessions((s) => s.snapshot.counts);
  const total = useSessions((s) => s.snapshot.sessions.length);
  const items = countItems(counts);
  if (items.length === 0) return total > 0 ? <span className="s-band-total">{total}</span> : null;
  return (
    <span className="s-band-counts">
      {items.map(({ n, Icon, tint }, i) => (
        <span key={i} style={{ color: tint }}>
          <Icon size={9} strokeWidth={3} />
          {n}
        </span>
      ))}
    </span>
  );
}

function Leading() {
  const session = useFocusedSession();
  const custom = useSessions((s) => (session ? s.names[session.id] : undefined));
  if (!session) return <Counts />;
  return (
    <button type="button" className="s-back" onClick={() => useSessions.getState().focus(null)}>
      <ChevronLeft size={13} strokeWidth={2.5} />
      <span>{displayName(session, custom)}</span>
    </button>
  );
}

function Trailing() {
  const session = useFocusedSession();
  const enlarged = useIsland((s) => s.sessionsEnlarged);
  const failedUntil = useSessions((s) => s.openFailedUntil);
  const renaming = useSessions((s) => s.renaming);
  const [, tick] = useState(0);
  useEffect(() => {
    const wait = failedUntil * 1000 - Date.now();
    if (wait <= 0) return;
    const t = setTimeout(() => tick((n) => n + 1), wait + 20);
    return () => clearTimeout(t);
  }, [failedUntil]);
  if (!session) return null;
  const failed = Date.now() / 1000 < failedUntil;
  return (
    <>
      <button
        type="button"
        className={`n-icon-button${renaming ? " is-selected" : ""}`}
        aria-label={S.rename}
        title={S.rename}
        onClick={() => useSessions.setState({ renaming: !renaming })}
      >
        {renaming ? <Check size={12} /> : <Pencil size={12} />}
      </button>
      <button
        type="button"
        className="n-icon-button"
        aria-label={enlarged ? S.shrink : S.enlarge}
        title={enlarged ? S.shrink : S.enlarge}
        onClick={() => useIsland.setState({ sessionsEnlarged: !enlarged })}
      >
        {enlarged ? <Minimize2 size={12} /> : <Maximize2 size={12} />}
      </button>
      <button
        type="button"
        className="n-pill-button is-prominent s-open"
        disabled={session.host === "unknown"}
        onClick={() => void useSessions.getState().openWindow(session)}
      >
        <ExternalLink size={11} />
        {failed ? S.openFailed : S.open}
      </button>
    </>
  );
}

export const sessionsPage: PageDefinition = { Page, Leading, Trailing };
