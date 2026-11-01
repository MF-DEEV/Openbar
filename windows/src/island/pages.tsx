// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Every section page and live activity the Windows island has so far.

import { useLocale } from "../i18n";

import { sessionsPage } from "../sessions/SessionsPage";
import { marksWidth, stripWing, readingWidth, stripTextSize, SessionsStrip } from "../sessions/SessionsStrip";
import { subscribeSessions, useSessions } from "../sessions/store";
import { currentStripLimit } from "../sessions/stripLimit";
import { useAgents } from "../agents/store";
import { displayName, duration } from "../sessions/format";
import { S } from "../sessions/strings";
import type { SessionEvent } from "../sessions/types";
import { geometryOf, modulesOf, useIsland } from "./store";
import { configuredIdleContent } from "./idle";
import { subscribeBattery } from "./battery";
import { followTrayBattery } from "./trayBattery";
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
import { DownloadsLeading, DownloadsPage, DownloadStrip, subscribeDownloads } from "../downloads/Downloads";
import { bridge, type AttentionContext } from "../shared/bridge";
import { DeferredSessionAttention, noticeLeavesWhenBusy } from "../sessions/attention";
import { subscribeAutoAllow } from "../sessions/autoAllow";
import { hidesAutoApproval, presentedSnapshot } from "../sessions/presentation";
import { MicrophoneStrip, subscribeMicrophone } from "../microphone/Microphone";
import "../timer/register";
import "../system/register";
import "../calendar/register";
import "../controls/register";
import "../tools/register";
import "../clipboard/register";
import "../captures/register";
import "../camera/register";
import "../files/register";
import "../agents/register";
import "../watch/register";
import "../screenshot-preview/register";
import { startMascot } from "../mascot/register";

activityViews.microphone = MicrophoneStrip;
void subscribeMicrophone();
void subscribeBattery();
followTrayBattery();

// MARK: Preferences

function applyPrefs(p: Prefs) {
  useIsland.setState({ settings: p.island });
  useSessions.setState({ prefs: p.sessions });
}

/** A section the person hid in Settings › Content shows nothing anywhere: no page, strip or notice. */
const shown = (id: "music" | "sessions") => modulesOf(useIsland.getState()).includes(id);
void loadPrefs().then((prefs) => {
  applyPrefs(prefs);
  startMascot();
});
void onPrefs(applyPrefs);

pages.sessions = sessionsPage;
pages.music = {
  Page: function Music() {
    useLocale();
    useMusicHeight();
    return <MusicPage />;
  },
};
activityViews.music = MusicStrip;
pages.notifications = { Page: NotificationsPage, Trailing: NotificationsTrailing };
pages.downloads = { Page: DownloadsPage, Leading: DownloadsLeading };
activityViews.downloads = DownloadStrip;
void subscribeDownloads();
activityViews.sessions = SessionsStrip;
subscribeAutoAllow();

/** The share the strip was last measured with. */
let lastLimitShare: string | null = null;

/** Sessions show beside the camera while any is working, waiting, stopped by a limit or done. */
function syncSessionsActivity() {
  const { counts } = presentedSnapshot(useSessions.getState());
  const island = useIsland.getState();
  const empty = counts.working + counts.waiting + (counts.limited ?? 0) + counts.finished === 0;
  if (empty || !shown("sessions") || !useSessions.getState().prefs.live) {
    lastLimitShare = null;
    island.setActivity("sessions", null);
    return;
  }
  const geometry = geometryOf(island);
  const size = stripTextSize(geometry.stripHeight);
  // A limit past the warning threshold widens the counts' wing.
  const limit = currentStripLimit()?.share ?? null;
  lastLimitShare = limit;
  island.setActivity("sessions", {
    wing: stripWing(counts, geometry.stripHeight, limit),
    capsuleContent: marksWidth(counts) + 6 + readingWidth(counts, size, limit),
    opens: "sessions",
    urgent: counts.waiting > 0,
  });
}

// The limits reading or its settings moving the strip's share remeasures it.
useAgents.subscribe((s, prev) => {
  if (s.snapshot !== prev.snapshot && (currentStripLimit()?.share ?? null) !== lastLimitShare) syncSessionsActivity();
});

useSessions.subscribe((s, prev) => {
  if (
    presentedSnapshot(s).counts !== presentedSnapshot(prev).counts ||
    s.prefs.live !== prev.prefs.live ||
    s.prefs.limits !== prev.prefs.limits
  )
    syncSessionsActivity();
  if (s.snapshot !== prev.snapshot || s.autoAllow !== prev.autoAllow) syncPendingSessions();
});
useIsland.subscribe((s, prev) => {
  if (
    s.settings.sections !== prev.settings.sections ||
    s.settings.hiddenModules !== prev.settings.hiddenModules ||
    s.settings.showPlayingMusic !== prev.settings.showPlayingMusic ||
    s.settings.idleContent !== prev.settings.idleContent ||
    s.settings.hiddenSystemMetrics !== prev.settings.hiddenSystemMetrics ||
    s.settings.agentsLimitFocus !== prev.settings.agentsLimitFocus ||
    s.settings.agentsLimitDisplay !== prev.settings.agentsLimitDisplay ||
    s.settings.agentsLimitAlert !== prev.settings.agentsLimitAlert ||
    s.settings.agentsLimitThreshold !== prev.settings.agentsLimitThreshold
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
  if (hidesAutoApproval(session, sessions.autoAllow)) return;
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
    session: { id: session.id, provider: session.provider, event: event.kind },
    // A tap lands on this session's page (NotchService.swift:2523).
    onOpen: () => useSessions.getState().focus(session.id),
  });
}

const sessionAttention = new DeferredSessionAttention(
  async () => (await bridge.attention.current()).canAutoPresent,
  (event) => {
    const session = useSessions.getState().snapshot.sessions.find((s) => s.id === event.id);
    if (!session || !session.phase.startsWith("waiting")) return undefined;
    if (hidesAutoApproval(session, useSessions.getState().autoAllow)) return undefined;
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
    const notice = island.notice;
    if (
      notice?.kind === "sessions" &&
      noticeLeavesWhenBusy(notice.session?.id, useSessions.getState().snapshot.sessions)
    )
      island.dismissNotice();
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
  if (
    !playback?.isPlaying ||
    !shown("music") ||
    !island.settings.showPlayingMusic ||
    configuredIdleContent(island.settings) === "none"
  ) {
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
