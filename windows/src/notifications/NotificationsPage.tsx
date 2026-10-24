// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Notifications page (NotchNotificationsView.swift; spec 04 §2.1): a
// rail of 240 pt cards, newest first, each with its app, time, title and
// message, Open and Dismiss; an empty state; and the access row while
// Windows has not let the island listen.

import { AppWindow, Bell, Trash2, X } from "lucide-react";
import { EmptyState } from "../island/views/Controls";
import { bridge } from "../shared/bridge";
import { useNotifications } from "./store";
import type { Item } from "./types";

const PRIVACY =
  "Shows new banners only. Messages stay in memory and are cleared when you lock this PC or turn this off.";

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
              ? "Notifications need the Vorssaint package"
              : "Let the island read new notifications"}
          </div>
          <div className="n-empty-detail">
            {access === "unavailable"
              ? "Install Vorssaint with its installer so Windows can let it read notifications."
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
  return (
    <div className="nt-card">
      <div className="nt-card-head">
        {item.icon ? <img src={item.icon} alt="" className="nt-icon" /> : <Bell size={14} />}
        <span className="nt-app">{item.app}</span>
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
      <div className="nt-title">{item.title}</div>
      {item.body ? <div className="nt-body">{item.body}</div> : null}
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
