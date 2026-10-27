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
    now: () => call<NowPlaying | null>("music_now"),
    onNowPlaying(handler: (n: NowPlaying) => void): Promise<UnlistenFn> {
      if (!inTauri) return Promise.resolve(() => {});
      return listen<NowPlaying>("music://now-playing", (e) => handler(e.payload));
    },
    command: (command: MusicCommand) => call<boolean>("music_command", { command }),
    seek: (seconds: number) => call<boolean>("music_seek", { seconds }),
    shuffle: (on: boolean) => call<boolean>("music_shuffle", { on }),
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
