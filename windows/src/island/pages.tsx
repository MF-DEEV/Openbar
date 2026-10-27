// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Every section page and live activity the Windows island has so far.

import { sessionsPage } from "../sessions/SessionsPage";
import { stripWing, readingWidth, stripTextSize, SessionsStrip } from "../sessions/SessionsStrip";
import { subscribeSessions, useSessions } from "../sessions/store";
import { displayName, duration } from "../sessions/format";
import { S } from "../sessions/strings";
import type { SessionEvent } from "../sessions/types";
import { geometryOf, modulesOf, useIsland } from "./store";
import { activityViews } from "./views/Activity";
import { pages } from "./views/Expanded";
import { MusicPage, useMusicHeight } from "../music/MusicPage";
import { MUSIC_WING, MusicStrip } from "../music/MusicStrip";
import { subscribeMusic, useMusic } from "../music/store";
import "../music/music.css";
import { NotificationsPage, NotificationsTrailing } from "../notifications/NotificationsPage";
import { subscribeNotifications } from "../notifications/store";
import { notificationNotice } from "../notifications/presentation";
import "../notifications/notifications.css";
import { loadPrefs, onPrefs, type Prefs } from "../shared/prefs";
import { DownloadsPage, DownloadStrip, subscribeDownloads } from "../downloads/Downloads";
import { bridge, type AttentionContext } from "../shared/bridge";
import { DeferredSessionAttention } from "../sessions/attention";
import { MicrophoneStrip, subscribeMicrophone } from "../microphone/Microphone";
import "../timer/register";
import "../system/register";
import "../calendar/register";
import "../controls/register";
import "../tools/register";
import "../clipboard/register";
import "../files/register";
import "../agents/register";
import "../watch/register";

activityViews.microphone = MicrophoneStrip;
void subscribeMicrophone();

// MARK: Preferences

function applyPrefs(p: Prefs) {
  useIsland.setState({ settings: p.island });
  useSessions.setState({ prefs: p.sessions });
}

/** A section the person hid in Settings › Content shows nothing anywhere: no page, strip or notice. */
const shown = (id: "music" | "sessions") => modulesOf(useIsland.getState()).includes(id);
void loadPrefs().then(applyPrefs);
void onPrefs(applyPrefs);

pages.sessions = sessionsPage;
pages.music = {
  Page: function Music() {
    useMusicHeight();
    return <MusicPage />;
  },
};
activityViews.music = MusicStrip;
pages.notifications = { Page: NotificationsPage, Trailing: NotificationsTrailing };
pages.downloads = { Page: DownloadsPage };
activityViews.downloads = DownloadStrip;
void subscribeDownloads();
activityViews.sessions = SessionsStrip;

/** Sessions show beside the camera while any is working, waiting or done. */
function syncSessionsActivity() {
  const { counts } = useSessions.getState().snapshot;
  const island = useIsland.getState();
  const empty = counts.working + counts.waiting + counts.finished === 0;
  if (empty || !shown("sessions") || !useSessions.getState().prefs.live) {
    island.setActivity("sessions", null);
    return;
  }
  const geometry = geometryOf(island);
  const size = stripTextSize(geometry.stripHeight);
  island.setActivity("sessions", {
    wing: stripWing(counts, geometry.stripHeight),
    capsuleContent: 14 + 6 + readingWidth(counts, size),
    opens: "sessions",
    urgent: counts.waiting > 0,
  });
}

useSessions.subscribe((s, prev) => {
  if (s.snapshot.counts !== prev.snapshot.counts || s.prefs.live !== prev.prefs.live) syncSessionsActivity();
  if (s.snapshot !== prev.snapshot) syncPendingSessions();
});
useIsland.subscribe((s, prev) => {
  if (
    s.settings.sections !== prev.settings.sections ||
    s.settings.hiddenModules !== prev.settings.hiddenModules ||
    s.settings.showPlayingMusic !== prev.settings.showPlayingMusic
  ) {
    syncSessionsActivity();
    syncMusicActivity();
  }
});
syncSessionsActivity();

/**
 * A session that needs the person opens the island on its page, without
 * taking the keyboard and never replacing a page already open; otherwise,
 * and for finished turns, a notice (NotchService.swift:4181-4214).
 */
function presentSessionEvent(event: SessionEvent) {
  const sessions = useSessions.getState();
  const session = sessions.snapshot.sessions.find((s) => s.id === event.id);
  if (!session) return;
  const island = useIsland.getState();
  const name = displayName(session, sessions.names[session.id]);
  if (!shown("sessions")) return;
  if (event.kind !== "finished" && !island.expanded && sessions.prefs.autoExpand) {
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

const sessionAttention = new DeferredSessionAttention(
  async () => (await bridge.attention.current()).canAutoPresent,
  (event) => {
    const session = useSessions.getState().snapshot.sessions.find((s) => s.id === event.id);
    if (!session || !session.phase.startsWith("waiting")) return undefined;
    return session.permission?.requestId ?? `${session.phase}:${session.phaseSince}`;
  },
  presentSessionEvent,
);
function syncPendingSessions() {
  const events: SessionEvent[] = useSessions
    .getState()
    .snapshot.sessions.filter((session) => session.phase.startsWith("waiting"))
    .map((session) =>
      session.phase === "waitingInput"
        ? { kind: "needsInput", id: session.id }
        : { kind: "needsPermission", id: session.id, tool: session.permission?.tool ?? "" },
    );
  void sessionAttention.sync(events);
}
function applyAttention(context: AttentionContext) {
  const island = useIsland.getState();
  useIsland.setState({ autoPresentAllowed: context.canAutoPresent, attentionFullscreen: context.fullscreen });
  if (context.busy) {
    if (island.expanded && island.selected === "sessions") island.collapse();
    if (island.notice?.kind === "sessions") island.dismissNotice();
  }
  // Re-read current pending requests on both transitions. Relying only on
  // queued events misses prompts that existed before the listener started.
  syncPendingSessions();
}
void bridge.attention.onContext(applyAttention).then(() => bridge.attention.current().then(applyAttention));
subscribeSessions((event) => {
  void sessionAttention.handle(event);
});

// MARK: Music

/** Music takes the closed island while it plays (notchShowPlayingMusic). */
function syncMusicActivity() {
  const { playback } = useMusic.getState();
  const island = useIsland.getState();
  if (!playback?.isPlaying || !shown("music") || !island.settings.showPlayingMusic) {
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
    const { settings } = useIsland.getState();
    if (!settings.trackChange || !shown("music")) return;
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

// MARK: Notifications

/** Mirror native banners, including the source app and sender, without stealing focus. */
subscribeNotifications((item) => {
  const island = useIsland.getState();
  if (!modulesOf(island).includes("notifications")) return;
  if (!island.settings.enabled) return;
  island.showNotice(notificationNotice(item));
});
