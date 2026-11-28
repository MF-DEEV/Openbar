// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A banner held open under the pointer (NotchNotificationPreviewView): the app,
// time, sender and message with its actions.

import { useLocale, t, tf, dateTimeFormat } from "../i18n";

import { AppWindow, Bell, Inbox, Reply, Send, X } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { contentInsets, type Geometry, notificationPreviewSize } from "../island/geometry";
import type { Size } from "../island/motion";
import { type Notice, useIsland } from "../island/store";
import { measureContext } from "../island/views/Closed";
import { bridge } from "../shared/bridge";
import { useNotifications } from "./store";
import { canReplyInDiscord } from "./presentation";
import { useDiscordReply } from "./reply";

const HEADER_HEIGHT = 28;
const ACTION_HEIGHT = 28;
const SPACING = 8;
const TITLE_LINE = 18;
const IMAGE_HEIGHT = 96;
const REPLY_HEIGHT = 100;

function messageLines(message: string, width: number) {
  const ctx = measureContext();
  ctx.font = "400 12px 'Inter Variable', Inter, sans-serif";
  return Math.min(
    3,
    Math.max(
      1,
      message
        .split(/\r?\n/)
        .reduce(
          (lines, line) => lines + Math.max(1, Math.ceil(ctx.measureText(line).width / Math.max(1, width - 16))),
          0,
        ),
    ),
  );
}

export function clock(time: number): string {
  return dateTimeFormat({ hour: "2-digit", minute: "2-digit" }).format(new Date(time * 1000));
}

export function NotificationIcon(props: { src?: string; fallback?: string }) {
  useLocale();
  const [failed, setFailed] = useState(false);
  useEffect(() => setFailed(false), [props.src]);
  if (!props.src || failed) return <Bell size={14} />;
  return (
    <img
      key={props.src}
      src={props.src}
      alt=""
      className="nt-icon"
      onError={(event) => {
        const image = event.currentTarget;
        if (props.fallback && image.getAttribute("src") !== props.fallback) {
          image.src = props.fallback;
        } else setFailed(true);
      }}
    />
  );
}

/** The sender takes one line or two, measured so the island fits it (NotchNotificationPreviewLayout). */
export function previewSize(
  notification: NonNullable<Notice["notification"]>,
  geometry: Geometry,
  replying = false,
): Size {
  let height = HEADER_HEIGHT + SPACING + ACTION_HEIGHT;
  if (notification.sender) {
    const { width } = notificationPreviewSize(geometry, height);
    const available = width - 2 * contentInsets(geometry, { width, height }).horizontal;
    const ctx = measureContext();
    ctx.font = "600 14px 'Inter Variable', Inter, sans-serif";
    const lines = ctx.measureText(notification.sender).width > available - 2 ? 2 : 1;
    height += SPACING + lines * TITLE_LINE;
  }
  if (notification.body) {
    const { width } = notificationPreviewSize(geometry, height);
    const available = width - 2 * contentInsets(geometry, { width, height }).horizontal;
    height += SPACING + messageLines(notification.body, available) * TITLE_LINE;
  }
  if (notification.image) height += SPACING + IMAGE_HEIGHT;
  if (replying) height += SPACING + REPLY_HEIGHT;
  return notificationPreviewSize(geometry, height);
}

export function NotificationPreview(props: { notice: Notice; geometry: Geometry; size: Size }) {
  useLocale();
  const { notice, geometry, size } = props;
  const notification = notice.notification!;
  const count = useNotifications((s) => s.items.length);
  const insets = contentInsets(geometry, size);
  const discord = canReplyInDiscord(notification);
  const reduceMotion = useReducedMotion();
  const reply = useDiscordReply(notification.id);
  const { replying } = reply;
  const field = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (!replying) return;
    void bridge.setFocusable(true, true);
    const timer = setTimeout(() => field.current?.focus({ preventScroll: true }), reduceMotion ? 0 : 300);
    return () => {
      clearTimeout(timer);
      if (!useIsland.getState().expanded) void bridge.setFocusable(false, false);
    };
  }, [replying, reduceMotion]);

  const open = async () => {
    try {
      const opened = await bridge.notifications.open(notification.aumid);
      if (opened) useIsland.getState().settleNotice(notice.id);
      else useIsland.getState().open("notifications");
    } catch {
      useIsland.getState().open("notifications");
    }
  };
  const dismiss = () => {
    useIsland.getState().settleNotice(notice.id);
    void useNotifications.getState().dismiss(notification.id);
  };

  return (
    <div
      className="nt-preview"
      role="group"
      aria-label={notice.title}
      style={{
        width: size.width,
        height: size.height,
        paddingTop: geometry.safeContentTop,
        paddingInline: insets.horizontal,
        gap: SPACING,
      }}
    >
      <div className="nt-preview-head" style={{ height: HEADER_HEIGHT }}>
        <NotificationIcon src={notice.image} fallback={notice.imageFallback} />
        <span className="nt-preview-app" title={notification.app}>
          {notification.app}
        </span>
        <span className="nt-time">{clock(notification.time)}</span>
        <button
          type="button"
          className="nt-mini"
          aria-label={t("notchNotifications.dismiss")}
          title={t("notchNotifications.dismiss")}
          onClick={dismiss}
        >
          <X size={12} />
        </button>
      </div>
      {notification.sender ? (
        <div className="nt-preview-title" title={notification.sender} style={{ lineHeight: `${TITLE_LINE}px` }}>
          {notification.sender}
        </div>
      ) : null}
      {notification.body ? (
        <div
          className="nt-preview-body"
          style={{ WebkitLineClamp: messageLines(notification.body, size.width - 2 * insets.horizontal) }}
        >
          {notification.body}
        </div>
      ) : null}
      {notification.image ? (
        <img className="nt-preview-image" src={notification.image} alt={t("windows.notifications.discordImageAlt")} />
      ) : null}
      <div className="nt-preview-actions" style={{ height: ACTION_HEIGHT }}>
        {discord ? (
          <button type="button" className="n-pill-button is-prominent" aria-expanded={replying} onClick={reply.toggle}>
            <Reply size={12} />
            {replying ? t("windows.notifications.cancelReply") : t("windows.notifications.reply")}
          </button>
        ) : null}
        <button type="button" className="n-pill-button is-prominent" onClick={() => void open()}>
          <AppWindow size={12} />
          {t("notchNotifications.open")}
        </button>
        {count > 1 ? (
          <button
            type="button"
            className="n-pill-button is-prominent"
            aria-label={tf("windows.notifications.notificationsValue", count)}
            title={t("notchNotifications.title")}
            onClick={() => useIsland.getState().open("notifications")}
          >
            <Inbox size={12} />
            {count}
          </button>
        ) : null}
      </div>
      <AnimatePresence initial={false}>
        {discord && replying ? (
          <motion.div
            className="nt-preview-reply"
            key="reply"
            initial={{ height: 0, opacity: 0, y: -8 }}
            animate={{ height: REPLY_HEIGHT, opacity: 1, y: 0 }}
            exit={{ height: 0, opacity: 0, y: -8 }}
            transition={reduceMotion ? { duration: 0 } : { duration: 0.32, ease: [0.22, 1, 0.36, 1] }}
          >
            <span className="nt-preview-reply-label">
              {tf("windows.notifications.replyTo", notification.sender || notification.app)}
            </span>
            <div className="nt-preview-reply-field">
              <textarea
                ref={field}
                value={reply.draft}
                onChange={(event) => reply.setDraft(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Escape") {
                    event.stopPropagation();
                    reply.close();
                  } else if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
                    event.preventDefault();
                    void reply.send();
                  }
                }}
                placeholder={t("windows.notifications.writeReply")}
                aria-label={t("windows.notifications.writeReplyLabel")}
                rows={1}
              />
              <button
                type="button"
                disabled={!reply.draft.trim() || reply.busy}
                onClick={() => void reply.send()}
                aria-label={t("windows.notifications.sendReplyLabel")}
                title={t("windows.notifications.sendReplyLabel")}
              >
                <Send size={14} />
                <span>{t("windows.notifications.sendReply")}</span>
              </button>
            </div>
            <span className="nt-preview-reply-status" role="status">
              {reply.status}
            </span>
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  );
}
