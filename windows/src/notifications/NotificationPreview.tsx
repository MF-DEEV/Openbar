// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A banner held open under the pointer (NotchNotificationPreviewView): the app,
// time and sender with its actions. Message bodies are not collected on Windows
// (docs/notifications.md), so the sender is all the text it has.

import { useLocale, t, tf, dateTimeFormat } from "../i18n";

import { AppWindow, Bell, Inbox, X } from "lucide-react";
import { contentInsets, type Geometry, notificationPreviewSize } from "../island/geometry";
import type { Size } from "../island/motion";
import { type Notice, useIsland } from "../island/store";
import { measureContext } from "../island/views/Closed";
import { bridge } from "../shared/bridge";
import { useNotifications } from "./store";

const HEADER_HEIGHT = 28;
const ACTION_HEIGHT = 28;
const SPACING = 8;
const TITLE_LINE = 18;

export function clock(time: number): string {
  return dateTimeFormat({ hour: "2-digit", minute: "2-digit" }).format(new Date(time * 1000));
}

export function NotificationIcon(props: { src?: string; fallback?: string }) {
  useLocale();
  if (!props.src) return <Bell size={14} />;
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
        } else image.style.visibility = "hidden";
      }}
    />
  );
}

/** The sender takes one line or two, measured so the island fits it (NotchNotificationPreviewLayout). */
export function previewSize(notification: NonNullable<Notice["notification"]>, geometry: Geometry): Size {
  let height = HEADER_HEIGHT + SPACING + ACTION_HEIGHT;
  if (notification.sender) {
    const { width } = notificationPreviewSize(geometry, height);
    const available = width - 2 * contentInsets(geometry, { width, height }).horizontal;
    const ctx = measureContext();
    ctx.font = "600 14px 'Inter Variable', Inter, sans-serif";
    const lines = ctx.measureText(notification.sender).width > available - 2 ? 2 : 1;
    height += SPACING + lines * TITLE_LINE;
  }
  return notificationPreviewSize(geometry, height);
}

export function NotificationPreview(props: { notice: Notice; geometry: Geometry; size: Size }) {
  useLocale();
  const { notice, geometry, size } = props;
  const notification = notice.notification!;
  const count = useNotifications((s) => s.items.length);
  const insets = contentInsets(geometry, size);

  const open = async () => {
    useIsland.getState().settleNotice(notice.id);
    const opened = await bridge.notifications.open(notification.aumid);
    if (opened === false) useIsland.getState().open("notifications");
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
      <div className="nt-preview-actions" style={{ height: ACTION_HEIGHT }}>
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
    </div>
  );
}
