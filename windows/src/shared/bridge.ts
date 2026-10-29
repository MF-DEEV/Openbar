// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The one place the pages talk to the Rust side. Outside Tauri (the page
// opened in a plain browser while designing) every call is a no-op, so the
// island can be developed and screenshotted without the app around it.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { register, unregister, type ShortcutEvent } from "@tauri-apps/plugin-global-shortcut";
import type { Decision, ReplyAttachment, ReplyFailure, SessionEvent, Snapshot } from "../sessions/types";
import type { MusicCommand, NowPlaying } from "../music/types";
import type { Access, Inbox, Item } from "../notifications/types";
import type { DownloadSnapshot } from "../downloads/types";
import type { MicrophoneSnapshot } from "../microphone/model";
import type { MetricKind, SpeedTestState, SystemSnapshot } from "../system/types";
import type { CalendarAccess, CalendarSnapshot } from "../calendar/types";
import type { AudioState, BrightnessDisplay, KeepAwakeState } from "../audio/types";
import type { CaptureOutcome, CapturePurpose, ToggleState } from "../tools/types";
import type { CaptureSession, Pick } from "../capture/types";
import type { RecentCapture } from "../captures/types";
import type { ScratchpadDocument } from "../scratchpad/types";
import type { ClipboardSnapshot, PasteResult } from "../clipboard/types";
import type { DragPhase, FilesSnapshot, OpenWithApp, ZipStatus } from "../files/types";
import type { AgentEvent, AgentSnapshot, CodexProfile, RedeemOutcome } from "../agents/types";
import type { WatchCondition, WatchFinished, WatchSnapshot } from "../watch/types";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
export interface AttentionContext {
  canAutoPresent: boolean;
  fullscreen: boolean;
  busy: boolean;
}

/** A rect in window CSS pixels; radii round its top and bottom corners. */
export interface HitRect {
  x: number;
  y: number;
  width: number;
  height: number;
  topRadius: number;
  bottomRadius: number;
}

export interface PointerEvent {
  inside: boolean;
  x: number;
  y: number;
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T | undefined> {
  if (!inTauri) return undefined;
  return invoke<T>(command, args);
}

export const bridge = {
  microphone: {
    snapshot: () => call<MicrophoneSnapshot>("microphone_snapshot"),
    onSnapshot(handler: (snapshot: MicrophoneSnapshot) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<MicrophoneSnapshot>("microphone://snapshot", (event) => handler(event.payload));
    },
  },
  attention: {
    current: async (): Promise<AttentionContext> =>
      (await call<AttentionContext>("attention_context")) ?? {
        canAutoPresent: !inTauri,
        fullscreen: false,
        busy: inTauri,
      },
    compactSessions: (on: boolean) => call("attention_compact_sessions", { on }),
    onContext(handler: (context: AttentionContext) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<AttentionContext>("island://attention-context", (event) => handler(event.payload));
    },
  },
  setHitRects: (rects: HitRect[]) => call("island_set_hit_rects", { rects }),
  setCapture: (capture: boolean) => call("island_set_capture", { capture }),
  openSettings: () => call("open_settings"),
  /** Lets the window take the keyboard (and takes it now when `take`), or gives it back. */
  setFocusable: (focusable: boolean, take: boolean) => call("island_set_focusable", { focusable, take }),
  sessions: {
    registerPermissionKey: async (key: "y" | "n" | "always", handler: (event: ShortcutEvent) => void) => {
      if (inTauri) await register(key === "always" ? "Control+T" : `Control+${key.toUpperCase()}`, handler);
    },
    unregisterPermissionKey: async (key: "y" | "n" | "always") => {
      if (inTauri) await unregister(key === "always" ? "Control+T" : `Control+${key.toUpperCase()}`);
    },
    onSnapshot(handler: (s: Snapshot) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<Snapshot>("sessions://snapshot", (e) => handler(e.payload));
    },
    onEvent(handler: (e: SessionEvent) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<SessionEvent>("sessions://event", (e) => handler(e.payload));
    },
    acknowledge: (sessionId: string) => call("sessions_acknowledge", { sessionId }),
    /** The open session page (null for the list), so Codex keeps that thread joined. */
    focus: (sessionId: string | null) => call("sessions_focus", { sessionId }),
    decide: (sessionId: string, requestId: string, decision: Decision) =>
      call<boolean>("sessions_decide", { sessionId, requestId, decision }),
    answer: (sessionId: string, requestId: string, answers: Record<string, string | string[]>) =>
      call<boolean>("sessions_answer", { sessionId, requestId, answers }),
    /** null when delivered. */
    reply: (sessionId: string, text: string, images: string[]) =>
      call<ReplyFailure | null>("sessions_reply", { sessionId, text, images }),
    attachImage: (dataUrl: string) => call<ReplyAttachment>("sessions_attach_image", { dataUrl }),
    clipboardImages: () => call<ReplyAttachment[]>("sessions_clipboard_images"),
    open: (sessionId: string) => call<boolean>("sessions_open", { sessionId }),
  },
  music: {
    selectSource: (source: string | null) => call<void>("music_select_source", { source }),
    players: () => call<import("../music/types").Source[]>("music_players"),
    now: () => call<NowPlaying | null>("music_now"),
    onNowPlaying(handler: (n: NowPlaying) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<NowPlaying>("music://now-playing", (e) => handler(e.payload));
    },
    command: (command: MusicCommand, source?: string) => call<boolean>("music_command", { command, source }),
    seek: (seconds: number, source?: string) => call<boolean>("music_seek", { seconds, source }),
    shuffle: (on: boolean, source?: string) => call<boolean>("music_shuffle", { on, source }),
    volume: (source: string, level?: number) => call<number>("music_volume", { source, level: level ?? null }),
    openPlayer: (source: string) => call<boolean>("music_open_player", { source }),
  },
  notifications: {
    inbox: () => call<Inbox>("notifications_inbox"),
    onInbox(handler: (i: Inbox) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<Inbox>("notifications://inbox", (e) => handler(e.payload));
    },
    onNew(handler: (i: Item) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<Item>("notifications://new", (e) => handler(e.payload));
    },
    requestAccess: () => call<Access>("notifications_request_access"),
    dismiss: (id: number) => call<Inbox>("notifications_dismiss", { id }),
    clear: () => call<Inbox>("notifications_clear"),
    open: (aumid: string) => call<boolean>("notifications_open", { aumid }),
  },
  downloads: {
    snapshot: () => call<DownloadSnapshot>("downloads_snapshot"),
    onSnapshot(handler: (snapshot: DownloadSnapshot) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<DownloadSnapshot>("downloads://snapshot", (event) => handler(event.payload));
    },
  },
  system: {
    /** Null until the monitor has probed what the PC can read. */
    snapshot: () => call<SystemSnapshot | null>("system_monitor_snapshot"),
    /** Samples run only while the page is open; `detail` adds its process list. */
    watch: (visible: boolean, detail: MetricKind | null) => call("system_monitor_watch", { visible, detail }),
    onSnapshot(handler: (snapshot: SystemSnapshot) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<SystemSnapshot>("system://snapshot", (event) => handler(event.payload));
    },
    /** Asks Windows once for the per-app network list (system_monitor.rs Approval). */
    approveNetwork: () => call<"approved" | "declined" | "busy">("system_network_approve"),
    speedTest: () => call<SpeedTestState>("speed_test_state"),
    /** Starts the network detail's speed test unless one is running. */
    startSpeedTest: () => call<SpeedTestState>("speed_test_start"),
    onSpeedTest(handler: (state: SpeedTestState) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<SpeedTestState>("speedtest://state", (event) => handler(event.payload));
    },
  },
  calendar: {
    snapshot: () => call<CalendarSnapshot>("calendar_snapshot"),
    onSnapshot(handler: (snapshot: CalendarSnapshot) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<CalendarSnapshot>("calendar://events", (event) => handler(event.payload));
    },
    /** The month grid's days to read besides the coming week, or null for the week alone; answers once read. */
    showRange: (range: { start: number; end: number } | null) =>
      call<CalendarSnapshot>("calendar_show_range", { start: range?.start ?? null, end: range?.end ?? null }),
    /** Windows' consent prompt, asked off the UI thread. */
    requestAccess: () => call<CalendarAccess>("calendar_request_access"),
    /** The appointment in the default calendar app, or the app itself without one. */
    open: (localId: string | null, instanceStart: number | null) =>
      call<boolean>("calendar_open", { localId, instanceStart }),
    openPrivacySettings: () => call<boolean>("calendar_open_privacy_settings"),
  },
  files: {
    snapshot: () => call<FilesSnapshot>("files_snapshot"),
    onShelf(handler: (snapshot: FilesSnapshot) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<FilesSnapshot>("files://shelf", (e) => handler(e.payload));
    },
    onZip(handler: (zip: ZipStatus) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<ZipStatus>("files://zip", (e) => handler(e.payload));
    },
    /** A drag over the island: entering, leaving, or dropped (and whether it landed). */
    onDrag(handler: (drag: DragPhase) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<DragPhase>("files://drag", (e) => handler(e.payload));
    },
    /** Every file an action reached for is gone. */
    onMissing(handler: () => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen("files://missing", () => handler());
    },
    remove: (ids: string[]) => call("files_remove", { ids }),
    clear: () => call("files_clear"),
    togglePin: (id: string) => call("files_toggle_pin", { id }),
    open: (ids: string[]) => call("files_open", { ids }),
    openWithApps: (ids: string[]) => call<OpenWithApp[]>("files_open_with_apps", { ids }),
    openWith: (ids: string[], handler: string) => call("files_open_with", { ids, handler }),
    reveal: (ids: string[]) => call("files_reveal", { ids }),
    revealOutputs: () => call("files_reveal_outputs"),
    share: (ids: string[]) => call<boolean>("files_share", { ids }),
    /** A native drag of these tiles; resolves once dropped, true when a destination took them. */
    dragOut: (ids: string[]) => call<boolean>("files_drag_out", { ids }),
    /** Asks where to save, then starts the job; true when it started. */
    zip: (ids: string[], title: string) => call<boolean>("files_zip", { ids, title }),
    cancelZip: () => call("files_zip_cancel"),
    kind: (path: string) => call<string | null>("files_kind", { path }),
  },
  agents: {
    snapshot: () => call<AgentSnapshot>("agents_snapshot"),
    onSnapshot(handler: (snapshot: AgentSnapshot) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<AgentSnapshot>("agents://snapshot", (event) => handler(event.payload));
    },
    onEvent(handler: (event: AgentEvent) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<AgentEvent>("agents://event", (event) => handler(event.payload));
    },
    /** The AI page is on screen: Claude's limits are checked every minute instead of five. */
    pageVisible: (visible: boolean) => call("agents_page_visible", { visible }),
    /** Codex's banked resets, read again when stale (or now when `force`). */
    refreshResets: (force: boolean) => call("agents_resets_refresh", { force }),
    /** Spends one banked reset. Only ever behind the Resets card's confirmation. */
    redeemReset: () => call<RedeemOutcome>("agents_resets_redeem"),
    /** Checks an added Codex home now, or every one. */
    refreshCodexAccount: (id: string | null) => call("agents_codex_refresh", { id }),
    /** The usual Codex home and the `.codex-<name>` homes beside it, with new ids. */
    discoverCodexHomes: (defaultName: string) => call<CodexProfile[]>("agents_codex_discover", { defaultName }),
    /** A folder the person picks; null when they cancel. */
    chooseCodexHome: (title: string, defaultName: string) =>
      call<{ name: string; directory: string } | { invalid: true } | null>("agents_codex_choose_folder", {
        title,
        defaultName,
      }),
  },
  shell: {
    /** A PNG data URL: the file's icon, or its thumbnail when `thumbnail`. */
    icon: (path: string, size: number, thumbnail = false) =>
      call<string | null>("shell_icon", { path, size, thumbnail }),
    /** Task Manager, or the running one brought forward; false when it couldn't be started. */
    openTaskManager: () => call<boolean>("shell_open_task_manager"),
  },
  sound: {
    play: () => call("sound_play"),
    /** The chime every 2 s for up to 5 min, until turned off (timer, watch). */
    alarm: (key: string, on: boolean) => call("sound_alarm", { key, on }),
  },
  timer: {
    /** Windows' interrupt time in ms: monotonic, and it counts sleep. */
    now: () => call<number>("timer_now_ms"),
    /** Raises `timer://due` after `delayMs` of interrupt time (null cancels), however the webview throttles. */
    wake: (delayMs: number | null) => call("timer_wake", { delayMs }),
    onDue(handler: () => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen("timer://due", () => handler());
    },
  },
  audio: {
    state: () => call<AudioState>("audio_state"),
    onState(handler: (state: AudioState) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<AudioState>("audio://state", (e) => handler(e.payload));
    },
    /** The output level moved without the island (keys, the system flyout, another app). */
    onVolumeChanged(handler: (level: { volume: number; muted: boolean }) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<{ volume: number; muted: boolean }>("audio://volume-changed", (e) => handler(e.payload));
    },
    setOutput: (volume?: number, muted?: boolean) =>
      call("audio_set_output", { volume: volume ?? null, muted: muted ?? null }),
    setInput: (volume?: number, muted?: boolean) =>
      call("audio_set_input", { volume: volume ?? null, muted: muted ?? null }),
    /** Makes the device the default for every role. */
    setDefault: (id: string) => call("audio_set_default", { id }),
    setApp: (key: string, volume?: number, muted?: boolean) =>
      call("audio_set_app", { key, volume: volume ?? null, muted: muted ?? null }),
    /** The mixer page shows: apps and their peaks are read four times a second. */
    watchApps: (on: boolean) => call("audio_watch_apps", { on }),
    /** Mic Mute on every microphone; undefined toggles. Resolves to the new state. */
    micMute: (muted?: boolean) => call<boolean>("mic_mute_set", { muted: muted ?? null }),
    /** Audio priority: a list's automatic switching, or its new order (outputs, else microphones). */
    setPriority: (output: boolean, change: { enabled?: boolean; ids?: string[] }) =>
      call("audio_priority_set", { output, enabled: change.enabled ?? null, ids: change.ids ?? null }),
    /** A shortcut field records: the output switcher lets go of its shortcut meanwhile. */
    switcherRecording: (on: boolean) => call("audio_switcher_recording", { on }),
  },
  keepAwake: {
    state: () => call<KeepAwakeState>("keep_awake_state"),
    onState(handler: (state: KeepAwakeState) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<KeepAwakeState>("keepawake://state", (e) => handler(e.payload));
    },
    /** On for the duration picked in Settings, or off. */
    toggle: () => call<KeepAwakeState>("keep_awake_toggle"),
    /** Minutes starts a session (0 = until turned off); null ends it. */
    set: (minutes: number | null) => call<KeepAwakeState>("keep_awake_set", { minutes }),
  },
  power: {
    /** Battery Saver (Energy saver), the Mac's Low Power Mode. */
    saver: () => call<boolean>("power_saver_on"),
    onSaver(handler: (on: boolean) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<boolean>("power://saver", (e) => handler(e.payload));
    },
  },
  brightness: {
    displays: () => call<BrightnessDisplay[]>("brightness_displays"),
    set: (id: string, value: number) => call("brightness_set", { id, value }),
  },
  capture: {
    list: () => call<RecentCapture[]>("captures_list"),
    open: (id: string) => call<void>("captures_open", { id }),
    remove: (id: string) => call<void>("captures_remove", { id }),
    clear: () => call<void>("captures_clear"),
    onChanged: (fn: () => void) => (inTauri ? listen("captures://changed", fn) : Promise.resolve(() => {})),
    /** Collapse the island first: the frozen picture is taken right away. */
    run: (purpose: CapturePurpose, name: string) => call<CaptureOutcome>("capture_run", { purpose, name }),
    // The overlay window's side (capture.html).
    session: () => call<CaptureSession | null>("capture_session"),
    pixels: (id: number) => call<ArrayBuffer>("capture_pixels", { id }),
    ready: (id: number) => call("capture_ready", { id }),
    commit: (id: number, pick: Pick | null) => call("capture_commit", { id, pick }),
  },
  tools: {
    copyText: (text: string) => call("tools_copy_text", { text }),
  },
  toggles: {
    state: () => call<ToggleState>("toggles_state"),
    run: (action: string) => call<void>("toggles_run", { action }),
  },
  scratchpad: {
    load: () => call<ScratchpadDocument | null>("scratchpad_load"),
    save: (document: ScratchpadDocument) => call("scratchpad_save", { document }),
    /** The saved path, or null when the person cancels. */
    export: (name: string, text: string) => call<string | null>("scratchpad_export", { name, text }),
  },
  clipboard: {
    snapshot: () => call<ClipboardSnapshot>("clipboard_snapshot"),
    onHistory(handler: (snapshot: ClipboardSnapshot) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<ClipboardSnapshot>("clipboard://history", (event) => handler(event.payload));
    },
    /** Puts the entry back on the clipboard; false when Windows wouldn't take it. */
    copy: (id: string) => call<boolean>("clipboard_copy", { id }),
    paste: (id: string) => call<PasteResult>("clipboard_paste", { id }),
    togglePin: (id: string) => call("clipboard_toggle_pin", { id }),
    remove: (id: string) => call("clipboard_remove", { id }),
    move: (id: string, up: boolean) => call("clipboard_move", { id, up }),
    /** Each unpinned entry the confirmation counted, with its copy time. */
    clearRecent: (confirmed: Record<string, number>) => call("clipboard_clear_recent", { confirmed }),
    copyText: (text: string) => call<boolean>("clipboard_copy_text", { text }),
    copyImage: (dataUrl: string) => call<boolean>("clipboard_copy_image", { dataUrl }),
    beep: () => call("clipboard_beep"),
  },
  watch: {
    snapshot: () => call<WatchSnapshot>("watch_snapshot"),
    onState(handler: (snapshot: WatchSnapshot) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<WatchSnapshot>("watch://state", (event) => handler(event.payload));
    },
    onFinished(handler: (event: WatchFinished) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<WatchFinished>("watch://finished", (event) => handler(event.payload));
    },
    /** Collapse the island first: the overlay takes the screen at once. True when a watch started. */
    choose: () => call<boolean>("watch_choose"),
    stop: () => call("watch_stop"),
    again: () => call<boolean>("watch_again"),
    /** Only what is given changes; text and numbers as typed, read by Rust in the person's number format. */
    setRule: (rule: { condition?: WatchCondition; text?: string; number?: string }) =>
      call("watch_set_rule", {
        condition: rule.condition ?? null,
        text: rule.text ?? null,
        number: rule.number ?? null,
      }),
    pinLine: (line: number | null) => call("watch_pin_line", { line }),
    /** The page is open: the area is read every second instead of two. */
    pageVisible: (visible: boolean) => call("watch_page_visible", { visible }),
    /** A Windows notification; false when the app has no identity to post it. */
    notify: (title: string, body: string) => call<boolean>("watch_notify", { title, body }),
  },
  onForegroundApp(handler: (name: string) => void): Promise<UnlistenFn> {
    if (!inTauri) return Promise.resolve(() => {});
    return listen<string>("bar://foreground-app", (e) => handler(e.payload));
  },
  /** The global shortcut (Ctrl+Alt+Space). */
  onToggle(handler: () => void): Promise<UnlistenFn> {
    if (!inTauri) return Promise.resolve(() => {});
    return listen("island://toggle", () => handler());
  },
  onOutsideClick(handler: () => void): Promise<UnlistenFn> {
    if (!inTauri) return Promise.resolve(() => {});
    return listen("island://outside-click", () => handler());
  },
  onFullscreen(handler: (hidden: boolean) => void): Promise<UnlistenFn> {
    if (!inTauri) return Promise.resolve(() => {});
    return listen<boolean>("island://fullscreen", (event) => handler(event.payload));
  },
  onPointer(handler: (event: PointerEvent) => void): Promise<UnlistenFn> {
    if (!inTauri) return Promise.resolve(() => {});
    return listen<PointerEvent>("island://pointer", (e) => handler(e.payload));
  },
};
