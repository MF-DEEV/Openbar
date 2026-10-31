// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Notifications page (NotchNotificationsView.swift; spec 04 §2.1): a
// rail of 240 pt cards, newest first, each with its app, time and sender,
// Open and Dismiss; an empty state; the access row while Windows has not let
// the island listen; and the question before Clear All empties the inbox.

import { useLocale, t } from "../i18n";

import { AppWindow, Bell, Trash2, X } from "lucide-react";
import { useEffect } from "react";
import { useIsland } from "../island/store";
import { EmptyState } from "../island/views/Controls";
import { bridge } from "../shared/bridge";
import { Sheet } from "../tools/Sheet";
import { clock, NotificationIcon } from "./NotificationPreview";
import { useNotifications } from "./store";
import type { Item } from "./types";
import { notificationPresentation } from "./presentation";

const PRIVACY = () => t("windows.notifications.notificationsShowTheAppIconAppNameAnd");

export function NotificationsPage() {
  useLocale();
  const access = useNotifications((s) => s.access);
  const items = useNotifications((s) => s.items);
  const confirmingClear = useNotifications((s) => s.confirmingClear);
  useEffect(() => () => useNotifications.setState({ confirmingClear: false }), []);

  if (access !== "allowed") {
    return (
      <div className="nt-access">
        <div className="n-empty-glyph">
          <Bell size={26} strokeWidth={1.5} />
        </div>
        <div className="nt-access-text">
          <div className="n-empty-title">
            {access === "unavailable"
              ? t("windows.notifications.notificationAccessIsUnavailableInThisBuild")
              : t("windows.notifications.letTheIslandReadNewNotifications")}
          </div>
          <div className="n-empty-detail">
            {access === "unavailable"
              ? t("windows.notifications.readingNotificationsNeedsTheVorssaintPackageTheInstaller")
              : access === "denied"
                ? t("windows.notifications.windowsSaysNoAllowItInSettingsPrivacy")
                : t("windows.notifications.windowsAsksYouOnce") + PRIVACY()}
          </div>
        </div>
        {access === "notAsked" ? (
          <button
            type="button"
            className="n-pill-button is-prominent"
            onClick={() => void useNotifications.getState().requestAccess()}
          >
            {t("notchClaudeSessions.allow")}
          </button>
        ) : null}
      </div>
    );
  }
  if (items.length === 0) {
    return <EmptyState icon={Bell} title={t("notchNotifications.empty")} detail={PRIVACY()} />;
  }
  return (
    <>
      <div className="nt-rail">
        {items.map((item) => (
          <Card key={item.id} item={item} />
        ))}
      </div>
      {confirmingClear ? <ClearConfirmation /> : null}
    </>
  );
}

/** Clearing empties the inbox for good, so it asks first (confirmClearAboveIsland). */
function ClearConfirmation() {
  useLocale();
  useEffect(() => {
    useIsland.getState().setHeld(true);
    return () => {
      const island = useIsland.getState();
      island.setHeld(false);
      // A pointer that left during the question still closes a hover opening.
      if (!island.inside) island.pointerExit();
    };
  }, []);
  return (
    <Sheet
      title={t("windows.notifications.clearAllNotifications")}
      confirm="Clear All"
      cancel="Cancel"
      destructive
      onConfirm={() => void useNotifications.getState().clear()}
      onCancel={() => useNotifications.setState({ confirmingClear: false })}
    />
  );
}

function Card({ item }: { item: Item }) {
  useLocale();
  const display = notificationPresentation(item);
  return (
    <div className="nt-card">
      <div className="nt-card-head">
        <NotificationIcon src={display.icon} fallback={display.fallbackIcon} />
        <span className="nt-app" title={display.app}>
          {display.app}
        </span>
        <span className="nt-time">{clock(item.time)}</span>
        <button
          type="button"
          className="nt-mini"
          aria-label={t("notchNotifications.open")}
          title={t("notchNotifications.open")}
          onClick={() => void bridge.notifications.open(item.aumid)}
        >
          <AppWindow size={12} />
        </button>
        <button
          type="button"
          className="nt-mini"
          aria-label={t("notchNotifications.dismiss")}
          title={t("notchNotifications.dismiss")}
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
  useLocale();
  const count = useNotifications((s) => s.items.length);
  if (count === 0) return null;
  return (
    <button
      type="button"
      className="n-pill-button s-open"
      onClick={() => useNotifications.setState({ confirmingClear: true })}
    >
      <Trash2 size={11} />
      {t("notchNotifications.clearAll")}
    </button>
  );
}
