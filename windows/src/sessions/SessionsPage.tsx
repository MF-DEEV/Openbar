// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Sessions section in the island: the list, or one session's page, and
// the island's top row for each (counts beside the list; back, rename,
// enlarge and open on a session's page).

import { useLocale } from "../i18n";

import { AppWindow, Check, ChevronLeft, ExternalLink, Maximize2, Minimize2, Pencil, Plus } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { PageDefinition } from "../island/views/Expanded";
import { useIsland } from "../island/store";
import { multiplexerOf } from "./Background";
import { NewSession } from "./NewSession";
import { SessionDetail } from "./SessionDetail";
import { SessionsList } from "./SessionsList";
import { GroupingSwitch } from "./SessionGroups";
import { displayName } from "./format";
import { S } from "./strings";
import { useSessions } from "./store";
import { presentedSnapshot } from "./presentation";
import { showing } from "./queue";
import "./sessions.css";
import "./progress/progress.css";
import { AwayLeading, AwayPage, AwayTrailing } from "./progress/AwayCard";

function useFocusedSession() {
  const focused = useSessions((s) => s.focused);
  const request = useSessions((s) => s.focusedRequest);
  const session = useSessions((s) =>
    focused ? presentedSnapshot(s).sessions.find((x) => x.id === focused) : undefined,
  );
  // The ask the queue opened the page on, when it waits behind another (#269).
  return useMemo(() => session && showing(session, request), [session, request]);
}

function Page() {
  useLocale();
  const enlarged = useIsland((s) => s.sessionsEnlarged);
  const session = useFocusedSession();
  const focused = useSessions((s) => s.focused);
  const composing = useSessions((s) => s.composing);
  const away = useSessions((s) => s.away !== null);
  // A session that ends while its page is open goes back to the list.
  useEffect(() => {
    if (focused && !session) useSessions.getState().focus(null);
  }, [focused, session]);
  if (away) return <AwayPage />;
  if (composing) return <NewSession />;
  return session ? <SessionDetail session={session} enlarged={enlarged} /> : <SessionsList />;
}

function Leading() {
  useLocale();
  const session = useFocusedSession();
  const custom = useSessions((s) => (session ? s.names[session.id] : undefined));
  const composing = useSessions((s) => s.composing);
  const away = useSessions((s) => s.away !== null);
  const listed = useSessions((s) => presentedSnapshot(s).sessions.length);
  if (away) return <AwayLeading />;
  if (composing) {
    return (
      <button type="button" className="s-back" onClick={() => useSessions.setState({ composing: false })}>
        <ChevronLeft size={13} strokeWidth={2.5} />
        <span>{S.newSession}</span>
      </button>
    );
  }
  // The open island shows every session, so the band leads with how they're
  // laid out rather than counting them again (#233).
  if (!session) return listed > 0 ? <GroupingSwitch /> : null;
  return (
    <button type="button" className="s-back" onClick={() => useSessions.getState().focus(null)}>
      <ChevronLeft size={13} strokeWidth={2.5} />
      <span>{displayName(session, custom)}</span>
    </button>
  );
}

function Trailing() {
  useLocale();
  const session = useFocusedSession();
  const enlarged = useIsland((s) => s.sessionsEnlarged);
  const failedUntil = useSessions((s) => s.openFailedUntil);
  const detached = useSessions((s) => s.detached);
  const renaming = useSessions((s) => s.renaming);
  const composing = useSessions((s) => s.composing);
  const canStart = useSessions((s) => s.snapshot.canStart === true || s.snapshot.canStartCodex === true);
  const away = useSessions((s) => s.away !== null);
  const [, tick] = useState(0);
  useEffect(() => {
    const wait = failedUntil * 1000 - Date.now();
    if (wait <= 0) return;
    const t = setTimeout(() => tick((n) => n + 1), wait + 20);
    return () => clearTimeout(t);
  }, [failedUntil]);
  // The Attach offer goes back to Open when it lapses.
  useEffect(() => {
    const wait = (detached?.until ?? 0) * 1000 - Date.now();
    if (wait <= 0) return;
    const t = setTimeout(() => tick((n) => n + 1), wait + 20);
    return () => clearTimeout(t);
  }, [detached]);
  if (away) return <AwayTrailing />;
  if (composing) return null;
  if (!session) {
    return canStart ? (
      <button type="button" className="n-pill-button s-open" onClick={() => useSessions.setState({ composing: true })}>
        <Plus size={11} />
        {S.newSession}
      </button>
    ) : null;
  }
  const failed = Date.now() / 1000 < failedUntil;
  const multiplexer = multiplexerOf(session);
  const offersAttach = detached !== null && detached.id === session.id && Date.now() / 1000 < detached.until;
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
        // A session Kimi Code's server holds opens in its app or page, whatever its host (#240).
        disabled={session.host === "unknown" && !session.served}
        title={
          offersAttach ? S.detachedHelp(multiplexer ?? "tmux") : session.hostName ? S.openIn(session.hostName) : S.open
        }
        onClick={() =>
          void (offersAttach
            ? useSessions.getState().attachDetached(session)
            : useSessions.getState().openWindow(session))
        }
      >
        {offersAttach ? <AppWindow size={11} /> : <ExternalLink size={11} />}
        {failed ? S.openFailed : offersAttach ? S.detachedAttach : S.open}
      </button>
    </>
  );
}

export const sessionsPage: PageDefinition = { Page, Leading, Trailing };
