// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Every section page and live activity the Windows island has so far.

import { sessionsPage } from "../sessions/SessionsPage";
import { stripWing, readingWidth, stripTextSize, SessionsStrip } from "../sessions/SessionsStrip";
import { subscribeSessions, useSessions } from "../sessions/store";
import { displayName, duration } from "../sessions/format";
import { S } from "../sessions/strings";
import type { SessionEvent } from "../sessions/types";
import { geometryOf, useIsland } from "./store";
import { activityViews } from "./views/Activity";
import { pages } from "./views/Expanded";
import { MusicPage, useMusicHeight } from "../music/MusicPage";
import { MUSIC_WING, MusicStrip } from "../music/MusicStrip";
import { subscribeMusic, useMusic } from "../music/store";
import "../music/music.css";

pages.sessions = sessionsPage;
pages.music = {
  Page: function Music() {
    useMusicHeight();
    return <MusicPage />;
  },
};
activityViews.music = MusicStrip;
activityViews.sessions = SessionsStrip;

/** Sessions show beside the camera while any is working, waiting or done. */
function syncSessionsActivity() {
  const { counts } = useSessions.getState().snapshot;
  const island = useIsland.getState();
  const empty = counts.working + counts.waiting + counts.finished === 0;
  if (empty || island.settings.sections.sessions === false) {
    island.setActivity("sessions", null);
    return;
  }
  const geometry = geometryOf(island);
  const size = stripTextSize(geometry.stripHeight);
  island.setActivity("sessions", {
    wing: stripWing(counts, geometry.stripHeight),
    capsuleContent: 14 + 6 + readingWidth(counts, size),
    opens: "sessions",
  });
}

useSessions.subscribe((s, prev) => {
  if (s.snapshot.counts !== prev.snapshot.counts) syncSessionsActivity();
});
syncSessionsActivity();

/**
 * A session that needs the person opens the island on its page, without
 * taking the keyboard and never replacing a page already open; otherwise,
 * and for finished turns, a notice (NotchService.swift:4181-4214).
 */
function onSessionEvent(event: SessionEvent) {
  const sessions = useSessions.getState();
  const session = sessions.snapshot.sessions.find((s) => s.id === event.id);
  if (!session) return;
  const island = useIsland.getState();
  const name = displayName(session, sessions.names[session.id]);
  if (event.kind !== "finished" && !island.expanded) {
    island.open("sessions", { takeFocus: false });
    sessions.focus(session.id);
    return;
  }
  const detail =
    event.kind === "needsPermission"
      ? `${S.permissionNotice} · ${session.permission?.summary ?? event.tool}`
      : event.kind === "needsInput"
        ? `${S.inputNotice} · ${session.permission?.summary ?? ""}`
        : event.duration
          ? S.finishedAfter(duration(event.duration))
          : S.finishedNotice;
  island.showNotice({
    id: `${event.kind}:${event.id}:${Date.now()}`,
    kind: "sessions",
    title: name,
    detail: detail.replace(/ · $/, ""),
    priority: 2,
    duration: 6,
    opens: "sessions",
  });
}

subscribeSessions(onSessionEvent);

// MARK: Music

/** Music takes the closed island while it plays (notchShowPlayingMusic). */
function syncMusicActivity() {
  const { playback } = useMusic.getState();
  const island = useIsland.getState();
  if (!playback?.isPlaying || island.settings.sections.music === false) {
    island.setActivity("music", null);
    return;
  }
  island.setActivity("music", { wing: MUSIC_WING, capsuleContent: 26 + 8 + 22, opens: "music" });
}

/** A new playing song raises the track notice after 0.5 s (NotchService.scheduleTrackNotice). */
let trackTimer: ReturnType<typeof setTimeout> | undefined;
let lastTrack = "";
function noticeTrack() {
  const p = useMusic.getState().playback;
  if (!p?.isPlaying) return;
  const key = `${p.source}|${p.title}|${p.artist}`;
  if (key === lastTrack) return;
  const first = lastTrack === "";
  lastTrack = key;
  if (first) return;
  clearTimeout(trackTimer);
  trackTimer = setTimeout(() => {
    const now = useMusic.getState().playback;
    if (!now || `${now.source}|${now.title}|${now.artist}` !== key) return;
    useIsland.getState().showNotice({
      id: `track:${key}`,
      kind: "track",
      title: now.title,
      detail: now.artist,
      priority: 0,
      duration: 3,
      opens: "music",
      image: now.artwork,
    });
  }, 500);
}

useMusic.subscribe((s, prev) => {
  if (s.playback !== prev.playback) {
    syncMusicActivity();
    noticeTrack();
  }
});
subscribeMusic();
