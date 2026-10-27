// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Notifications page (NotchNotificationsView.swift; spec 04 §2.1): a
// rail of 240 pt cards, newest first, each with its app, time and sender,
// Open and Dismiss; an empty state; and the access row while
// Windows has not let the island listen.

import { AppWindow, Bell, Trash2, X } from "lucide-react";
import { EmptyState } from "../island/views/Controls";
import { bridge } from "../shared/bridge";
import { useNotifications } from "./store";
import type { Item } from "./types";
import { notificationPresentation } from "./presentation";

const PRIVACY =
  "Notifications show the app icon, app name and sender only. Message text is not collected or displayed. Discord’s Do Not Disturb and muted conversations follow its desktop notification settings. Notification details are cleared when you lock this PC or turn this off.";

function clock(time: number): string {
  return new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit" }).format(new Date(time * 1000));
}

export function NotificationsPage() {
  const access = useNotifications((s) => s.access);
  const items = useNotifications((s) => s.items);

  if (access !== "allowed") {
    return (
      <div className="nt-access">
        <div className="n-empty-glyph">
          <Bell size={26} strokeWidth={1.5} />
        </div>
        <div className="nt-access-text">
          <div className="n-empty-title">
            {access === "unavailable"
              ? "Notification access is unavailable in this build"
              : "Let the island read new notifications"}
          </div>
          <div className="n-empty-detail">
            {access === "unavailable"
              ? "Reading notifications needs the Vorssaint package the installer registers. Reinstall Vorssaint to add it, then open Notifications again."
              : access === "denied"
                ? "Windows says no. Allow it in Settings › Privacy & security › Notifications."
                : "Windows asks you once. " + PRIVACY}
          </div>
        </div>
        {access === "notAsked" ? (
          <button
            type="button"
            className="n-pill-button is-prominent"
            onClick={() => void useNotifications.getState().requestAccess()}
          >
            Allow
          </button>
        ) : null}
      </div>
    );
  }
  if (items.length === 0) {
    return <EmptyState icon={Bell} title="New notifications will appear here" detail={PRIVACY} />;
  }
  return (
    <div className="nt-rail">
      {items.map((item) => (
        <Card key={item.id} item={item} />
      ))}
    </div>
  );
}

function Card({ item }: { item: Item }) {
  const display = notificationPresentation(item);
  return (
    <div className="nt-card">
      <div className="nt-card-head">
        {display.icon ? (
          <img
            key={display.icon}
            src={display.icon}
            alt=""
            className="nt-icon"
            onError={(event) => {
              const image = event.currentTarget;
              if (display.fallbackIcon && image.getAttribute("src") !== display.fallbackIcon) {
                image.src = display.fallbackIcon;
              } else image.style.visibility = "hidden";
            }}
          />
        ) : (
          <Bell size={14} />
        )}
        <span className="nt-app" title={display.app}>
          {display.app}
        </span>
        <span className="nt-time">{clock(item.time)}</span>
        <button
          type="button"
          className="nt-mini"
          aria-label="Open"
          title="Open"
          onClick={() => void bridge.notifications.open(item.aumid)}
        >
          <AppWindow size={12} />
        </button>
        <button
          type="button"
          className="nt-mini"
          aria-label="Dismiss"
          title="Dismiss"
          onClick={() => void useNotifications.getState().dismiss(item.id)}
        >
          <X size={12} />
        </button>
      </div>
      <div className="nt-title" title={display.sender}>
        {display.sender || display.app}
      </div>
    </div>
  );
}

/** "Clear All" on the right of the top row while the inbox holds anything. */
export function NotificationsTrailing() {
  const count = useNotifications((s) => s.items.length);
  if (count === 0) return null;
  return (
    <button type="button" className="n-pill-button s-open" onClick={() => void useNotifications.getState().clear()}>
      <Trash2 size={11} />
      Clear All
    </button>
  );
}
