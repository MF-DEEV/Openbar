// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Notifications page (NotchNotificationsView.swift; spec 04 §2.1): a
// rail of 240 pt cards, newest first, each with its app, time and sender,
// Open and Dismiss; an empty state; the access row while Windows has not let
// the island listen; and the question before Clear All empties the inbox.

import { useLocale, t, dateTimeFormat } from "../i18n";

import { AppWindow, Bell, Reply, Search, Send, Trash2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useIsland } from "../island/store";
import { EmptyState } from "../island/views/Controls";
import { bridge } from "../shared/bridge";
import { Sheet } from "../tools/Sheet";
import { clock, NotificationIcon } from "./NotificationPreview";
import { useNotifications } from "./store";
import type { Item } from "./types";
import { useDiscordReply } from "./reply";
import { canReplyInDiscord, notificationPresentation } from "./presentation";

const PRIVACY = () => t("windows.notifications.notificationsShowTheAppIconAppNameAnd");

export function NotificationsPage() {
  useLocale();
  const access = useNotifications((s) => s.access);
  const items = useNotifications((s) => s.items);
  const error = useNotifications((s) => s.error);
  const requesting = useNotifications((s) => s.requesting);
  const [query, setQuery] = useState("");
  const matches = items.filter((item) =>
    `${item.app}\n${item.title}\n${item.body ?? ""}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()),
  );
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
              ? t("windows.notifications.readingNotificationsNeedsTheOpenbarPackageTheInstaller")
              : access === "denied"
                ? t("windows.notifications.windowsSaysNoAllowItInSettingsPrivacy")
                : t("windows.notifications.windowsAsksYouOnce") + PRIVACY()}
          </div>
        </div>
        {access === "notAsked" ? (
          <button
            type="button"
            className="n-pill-button is-prominent"
            disabled={requesting}
            onClick={() => void useNotifications.getState().requestAccess()}
          >
            {requesting ? t("windows.calendar.loading") : t("notchClaudeSessions.allow")}
          </button>
        ) : null}
        {access === "denied" ? (
          <button
            type="button"
            className="n-pill-button is-prominent"
            onClick={() => void bridge.notifications.openPrivacySettings()}
          >
            {t("windows.notifications.openSettings")}
          </button>
        ) : null}
      </div>
    );
  }
  if (items.length === 0) {
    return (
      <>
        <EmptyState icon={Bell} title={t("notchNotifications.empty")} detail={PRIVACY()} />
        {error ? (
          <p className="nt-error" role="alert">
            {error}
          </p>
        ) : null}
      </>
    );
  }
  return (
    <div className="nt-history">
      <div className="nt-history-toolbar">
        <label className="nt-search">
          <Search size={13} aria-hidden="true" />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("windows.notifications.searchHistory")}
            aria-label={t("windows.notifications.searchHistory")}
          />
        </label>
        <span className="nt-history-count">{items.length}</span>
      </div>
      {error ? (
        <p className="nt-error" role="alert">
          {error}
        </p>
      ) : null}
      <div className="nt-rail">
        {matches.map((item, index) => {
          const day = new Date(item.time * 1000).toDateString();
          const previous = matches[index - 1];
          return (
            <div className="nt-entry" key={item.id}>
              {!previous || new Date(previous.time * 1000).toDateString() !== day ? (
                <div className="nt-day-label">
                  {dateTimeFormat({ weekday: "short", day: "numeric", month: "long" }).format(item.time * 1000)}
                </div>
              ) : null}
              <Card item={item} />
            </div>
          );
        })}
        {matches.length === 0 ? <EmptyState icon={Search} title={t("windows.notifications.noMatches")} /> : null}
      </div>
      {confirmingClear ? <ClearConfirmation /> : null}
    </div>
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
      confirm={t("notchNotifications.clearAll")}
      cancel={t("windows.calendar.cancel")}
      destructive
      onConfirm={() => void useNotifications.getState().clear()}
      onCancel={() => useNotifications.setState({ confirmingClear: false })}
    />
  );
}

function Card({ item }: { item: Item }) {
  useLocale();
  const display = notificationPresentation(item);
  const discord = canReplyInDiscord(item);
  const reply = useDiscordReply(item.id);
  const { replying } = reply;
  const field = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    if (replying) {
      field.current?.scrollIntoView({ block: "nearest" });
      field.current?.focus();
    }
  }, [replying]);
  return (
    <div className="nt-card">
      <div className="nt-card-head">
        <NotificationIcon src={display.icon} fallback={display.fallbackIcon} />
        <span className="nt-app" title={display.app}>
          {display.app}
        </span>
        <span className="nt-time">{clock(item.time)}</span>
        {discord ? (
          <button
            type="button"
            className="nt-mini"
            aria-label={t("windows.notifications.replyInDiscord")}
            aria-expanded={replying}
            title={t("windows.notifications.replyInDiscord")}
            onClick={reply.toggle}
          >
            <Reply size={12} />
          </button>
        ) : null}
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
      {display.message ? <div className="nt-body">{display.message}</div> : null}
      {display.image ? (
        <img className="nt-card-image" src={display.image} alt={t("windows.notifications.discordImageAlt")} />
      ) : null}
      {discord && replying ? (
        <div className="nt-reply">
          <div className="nt-reply-compose">
            <textarea
              ref={field}
              value={reply.draft}
              onChange={(event) => reply.setDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
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
              className="nt-reply-submit"
              disabled={!reply.draft.trim() || reply.busy}
              onClick={() => void reply.send()}
              title={t("windows.notifications.sendReplyLabel")}
              aria-label={t("windows.notifications.sendReplyLabel")}
            >
              <Send size={14} />
              <span>{t("windows.notifications.sendReply")}</span>
            </button>
          </div>
          <span className="nt-reply-hint" role="status">
            {reply.status}
          </span>
        </div>
      ) : null}
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
