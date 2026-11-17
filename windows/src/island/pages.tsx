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
import { queueNotification } from "../notifications/delivery";
import "../notifications/notifications.css";
import { loadPrefs, onPrefs, type Prefs } from "../shared/prefs";
import { DownloadsLeading, DownloadsPage, DownloadStrip, subscribeDownloads } from "../downloads/Downloads";
import { bridge, type AttentionContext } from "../shared/bridge";
import { DeferredSessionAttention, noticeLeavesWhenBusy } from "../sessions/attention";
import { inView, SessionSoundEvents, SessionSoundQueue, terminalSound } from "../sessions/waitingSound";
import { soundsMuted } from "../sessions/soundPreferences";
import { subscribeAutoAllow } from "../sessions/autoAllow";
import { awayState, startAwayWatcher } from "../sessions/awayWatcher";
import {
  REMINDER_AFTER_UNLOCK_MS,
  REMINDER_CHECK_MS,
  ReminderRunner,
  type ReminderKind,
  type ReminderWait,
} from "../sessions/reminders";
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
import "../updates/register";
import "../files/register";
import "../agents/register";
import "../watch/register";
import "../screenshot-preview/register";
import { startMascot } from "../mascot/register";
import { subscribeVoiceShortcut, watchVoice } from "../sessions/voice";

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

// Voice replies stop when the session ends or the island leaves its page,
// and the hold-to-talk shortcut speaks to the page on screen or opens one.
const showingSession = () => {
  const island = useIsland.getState();
  return island.expanded && island.selected === "sessions" ? useSessions.getState().focused : null;
};
watchVoice(
  () => showingSession() !== null,
  (fn) => useIsland.subscribe(fn),
);
subscribeVoiceShortcut({
  showing: showingSession,
  openOn: (id) => {
    useSessions.getState().focus(id);
    useIsland.getState().open("sessions");
  },
});
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
startAwayWatcher();

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

// A sound once for each new request, unless its page is already in front
// (#167) or its own terminal tab is (#210).
const waitingSound = new SessionSoundEvents();
const soundQueue = new SessionSoundQueue(
  () => {
    const s = useSessions.getState();
    const island = useIsland.getState();
    return {
      prefs: s.prefs,
      enabled: island.settings.enabled && shown("sessions"),
      inView: (id: string) => inView(island, s.focused, id),
      current: (event) => {
        const session = s.snapshot.sessions.find((row) => row.id === event.session);
        if (!session) return false;
        if (event.kind === "started") return true;
        if (event.kind === "permission" || event.kind === "question") {
          return (
            !hidesAutoApproval(session, s.autoAllow) &&
            (session.permission?.requestId === event.id || `${session.phase}:${session.phaseSince}` === event.id)
          );
        }
        const terminal = terminalSound(session);
        return terminal?.kind === event.kind && terminal.id === event.id;
      },
    };
  },
  bridge.sessions.terminalShown,
  (sound, volume) => bridge.sound.play(sound, volume),
);
const stopSounds = useSessions.subscribe((s, prev) => {
  if (s.prefs !== prev.prefs && (s.prefs.soundVolume === 0 || soundsMuted(s.prefs))) void bridge.sound.stopSession();
  if (s.snapshot === prev.snapshot && s.autoAllow === prev.autoAllow) return;
  soundQueue.enqueue(waitingSound.arrived(s.snapshot.sessions, s.autoAllow));
});
if (import.meta.hot)
  import.meta.hot.dispose(() => {
    stopSounds();
    soundQueue.dispose();
  });
subscribeSessions((event) => {
  void sessionAttention.handle(event);
});

// MARK: Reminders (#231)

const REMINDER_EVENT: Record<ReminderKind, SessionEvent["kind"]> = {
  permission: "needsPermission",
  question: "needsInput",
  finished: "finished",
};

/**
 * One reminder for every session in it, after its sound: the island on the
 * one session, or on the list of them. While another app is in use, or the
 * island is open on something else, a notice instead, as for the request
 * itself (#167). The list or the session's page already in view, or the
 * away card, needs nothing more.
 */
async function presentReminder(waits: ReminderWait[]) {
  const sessions = useSessions.getState();
  const island = useIsland.getState();
  console.info(`[openbar] reminder fired: ${waits.length} session(s) [${waits.map((w) => w.kind).join(",")}]`);
  if (sessions.away) return;
  const focused = sessions.focused;
  if (
    island.expanded &&
    island.selected === "sessions" &&
    (focused === null || (waits.length === 1 && focused === waits[0].session))
  )
    return;
  const allowed = await bridge.attention
    .current()
    .then((context) => context.canAutoPresent)
    .catch(() => false);
  const one = waits.length === 1 ? waits[0].session : null;
  if (allowed && sessions.prefs.autoExpand && !island.expanded) {
    island.open("sessions", { takeFocus: false });
    useSessions.getState().focus(one);
    return;
  }
  const rows = waits
    .map((wait) => sessions.snapshot.sessions.find((session) => session.id === wait.session))
    .filter((session) => session !== undefined);
  const first = rows[0];
  if (!first) return;
  const minutes = Math.max(1, Math.floor((Date.now() / 1000 - waits[0].since) / 60));
  island.showNotice({
    id: `reminder:${waits.map((w) => w.session).join(",")}:${Date.now()}`,
    kind: "sessions",
    title: one ? displayName(first, sessions.names[first.id]) : S.reminderTitle(rows.length),
    detail: one
      ? `${S.reminderStill} · ${S.waitingFor(minutes)}`
      : rows.map((session) => displayName(session, sessions.names[session.id])).join(", "),
    priority: 2,
    duration: 6,
    opens: "sessions",
    session: one ? { id: first.id, provider: first.provider, event: REMINDER_EVENT[waits[0].kind] } : undefined,
    onOpen: () => useSessions.getState().focus(one),
  });
}

const reminders = new ReminderRunner({
  sessions: () => {
    const s = useSessions.getState();
    return s.snapshot.sessions.filter((session) => !hidesAutoApproval(session, s.autoAllow));
  },
  prefs: () => useSessions.getState().prefs,
  enabled: () => useIsland.getState().settings.enabled && shown("sessions"),
  quiet: () => bridge.session.quiet(),
  locked: () => awayState.locked,
  terminalShown: bridge.sessions.terminalShown,
  play: (sound, volume) => bridge.sound.play(sound, volume),
  present: presentReminder,
  now: () => Date.now() / 1000,
});
const reminderTimer = setInterval(() => void reminders.check(), REMINDER_CHECK_MS);
// A reminder held through the lock comes a moment after the unlock.
void bridge.session.onUnlock(() => {
  setTimeout(() => void reminders.check(), REMINDER_AFTER_UNLOCK_MS);
});
if (import.meta.hot) import.meta.hot.dispose(() => clearInterval(reminderTimer));

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
  queueNotification(item);
});
