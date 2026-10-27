// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Calendar section's strings (NotchCalendarStrings.swift, English), with
// Mac-only wording swapped for Windows.

export const C = {
  title: "Calendar",
  permission: "Read your calendars to show upcoming appointments. Events stay on this PC.",
  allow: "Allow Calendar Access",
  denied: "Allow calendar access in Windows Settings to see your appointments.",
  settings: "Open Windows Settings",
  unavailableTitle: "Calendar access is unavailable in this build",
  unavailable:
    "Reading your calendars needs the Vorssaint package the installer registers. Reinstall Vorssaint to add it, then open Calendar again.",
  empty: "No upcoming appointments",
  emptyDay: "No appointments on this day",
  noCalendars:
    "No calendars found. Windows shares the calendars of its Calendar app and classic Outlook; the new Outlook keeps its own.",
  next: "Up next",
  ongoing: "Happening now",
  allDay: "All day",
  untitled: "Untitled event",
  loading: "Loading",
  openCalendar: "Open Calendar",
  week: "Next 7 days",
  today: "Today",
  requestFailed: "Could not request calendar access. Please try again.",
  previousMonth: "Previous month",
  nextMonth: "Next month",
  previousWeek: "Previous week",
  nextWeek: "Next week",
  month: "Month",
  hasEvents: "Has appointments",
  countdown: "Event countdown",
  addCountdown: "Add Countdown",
  removeCountdown: "Remove Countdown",
  weekNumber: (n: number) => `Week ${n}`,
  /** The closed strip's label: "Up next: Standup". */
  stripLabel: (ongoing: boolean, title: string) => `${ongoing ? "Happening now" : "Up next"}: ${title}`,
  /** Read out for the strip's clock. */
  remaining: (seconds: number) => {
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return [m ? `${m} minute${m === 1 ? "" : "s"}` : "", `${s} second${s === 1 ? "" : "s"}`].filter(Boolean).join(", ");
  },
};
