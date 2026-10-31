// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { t, tf, formatUnit } from "../i18n/index";

// The Calendar section's strings (NotchCalendarStrings.swift, English), with
// Mac-only wording swapped for Windows.

export const C = {
  get title() {
    return t("notchCalendar.title");
  },
  get permission() {
    return t("windows.calendar.permission");
  },
  get allow() {
    return t("notchCalendar.allow");
  },
  get denied() {
    return t("windows.calendar.denied");
  },
  get settings() {
    return t("windows.calendar.settings");
  },
  get unavailableTitle() {
    return t("windows.calendar.unavailableTitle");
  },
  get unavailable() {
    return t("windows.calendar.unavailable");
  },
  get empty() {
    return t("notchCalendar.empty");
  },
  get emptyDay() {
    return t("notchCalendar.emptyDay");
  },
  get noCalendars() {
    return t("windows.calendar.noCalendars");
  },
  get next() {
    return t("notchCalendar.next");
  },
  get ongoing() {
    return t("notchCalendar.ongoing");
  },
  get allDay() {
    return t("notchCalendar.allDay");
  },
  get untitled() {
    return t("notchCalendar.untitled");
  },
  get loading() {
    return t("windows.calendar.loading");
  },
  get openCalendar() {
    return t("notchCalendar.openCalendar");
  },
  get week() {
    return t("notchCalendar.week");
  },
  get today() {
    return t("notchCalendar.today");
  },
  get requestFailed() {
    return t("notchCalendar.requestFailed");
  },
  get previousMonth() {
    return t("notchCalendar.previousMonth");
  },
  get nextMonth() {
    return t("notchCalendar.nextMonth");
  },
  get previousWeek() {
    return t("notchCalendar.previousWeek");
  },
  get nextWeek() {
    return t("notchCalendar.nextWeek");
  },
  get month() {
    return t("notchCalendar.month");
  },
  get hasEvents() {
    return t("notchCalendar.hasEvents");
  },
  get countdown() {
    return t("notchCalendar.countdown");
  },
  get addCountdown() {
    return t("notchCalendar.addCountdown");
  },
  get removeCountdown() {
    return t("notchCalendar.removeCountdown");
  },
  weekNumber: (n: number) => tf("notchCalendar.weekNumber", n),
  /** The closed strip's label: "Up next: Standup". */
  stripLabel: (ongoing: boolean, title: string) =>
    tf("windows.calendar.stripLabel", ongoing ? t("notchCalendar.ongoing") : t("notchCalendar.next"), title),
  /** Read out for the strip's clock. */
  remaining: (seconds: number) => {
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return [m ? formatUnit(m, "minute") : "", formatUnit(s, "second")].filter(Boolean).join(", ");
  },
};
