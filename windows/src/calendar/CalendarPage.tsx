// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Calendar page (NotchCalendarView.swift, NotchCalendarMonthView.swift;
// spec 04 §3.1). Three layouts: a tall, wide page has the month beside the
// agenda; a short one has the week as a strip above it, and its month title
// opens the month grid over both. The title and the arrows head the page
// from the island's top row.

import {
  CalendarCheck,
  CalendarClock,
  ChevronLeft,
  ChevronRight,
  CircleDot,
  LoaderCircle,
  MapPin,
  SquareArrowOutUpRight,
  Timer,
} from "lucide-react";
import { type CSSProperties, useEffect, useRef } from "react";
import { usePageLayer } from "../island/pageLayer";
import { IconButton, PillButton } from "../island/views/Controls";
import { useIsland } from "../island/store";
import type { PageProps } from "../island/views/Expanded";
import { bridge } from "../shared/bridge";
import { useMenu } from "../shared/ui/Menu";
import {
  agendaGroups,
  dotColors,
  eventsOn,
  isChosen,
  monthDays,
  MONTH_WEEKDAY_HEIGHT,
  monthRowHeight,
  next,
  sameDay,
  sameMonth,
  startOfDay,
  startOfMonth,
  startsWeek,
  timeLine,
  weekDays,
  weekNumber,
} from "./model";
import { calendarView, requestAccess, setCountdownChoice, showMonth, useCalendar } from "./store";
import { C } from "./strings";
import type { CalendarEvent } from "./types";
import { useMinute } from "./useMinute";
import "./calendar.css";

/** Today's circle (MonthView:100). */
export const TODAY = "rgb(255, 92, 99)";
/** Three 28 pt buttons, then Open Calendar 6 pt apart. */
const NAVIGATION_WIDTH = 28 * 3;
const OPEN_WIDTH = 6 + 28;

const weekdayLetter = (day: number) => new Intl.DateTimeFormat(undefined, { weekday: "narrow" }).format(day);
const fullDate = (day: number) =>
  new Intl.DateTimeFormat(undefined, { weekday: "long", day: "numeric", month: "long", year: "numeric" }).format(day);

/** A month grid needs six rows beside an agenda; anything shorter shows the week as a strip. */
export const isWide = (width: number, height: number) => height >= 300 && width >= 420;

/** Windows asks once; the first visit to the page asks for the person. */
let asked = false;

export function CalendarPage({ width, height }: PageProps) {
  const access = useCalendar((s) => s.access);
  const received = useCalendar((s) => s.received);
  const focus = useCalendar((s) => s.focus);
  const selectedDay = useCalendar((s) => s.selectedDay);
  const showingMonth = useCalendar((s) => s.showingMonth);
  const wide = isWide(width, height);
  const now = useMinute();
  const month = startOfMonth(focus);

  const firstWeekday = useCalendar((s) => s.firstWeekday);
  // A new day moves the coming week on, so the page reads again then too.
  const today = startOfDay(now);
  useEffect(() => useCalendar.setState({ wide, width }), [wide, width]);
  useEffect(() => {
    void showMonth(month);
  }, [month, today, firstWeekday]);
  useEffect(
    () => () => {
      void showMonth(null);
      calendarView.reset();
    },
    [],
  );
  // A new day brings the page back to today unless a day is chosen.
  useEffect(() => {
    if (useCalendar.getState().selectedDay === null) useCalendar.setState({ focus: Date.now() });
  }, [today]);
  // Escape returns from the month grid to the strip before the island closes.
  usePageLayer(showingMonth && !wide, calendarView.closeMonth);
  useEffect(() => {
    if (access === "notAsked" && !asked) {
      asked = true;
      void requestAccess();
    }
  }, [access]);

  if (!received) return <Spinner />;
  if (access !== "allowed") return <PermissionCard />;
  if (wide) {
    return (
      <div className="cal-wide">
        <div className="cal-wide-month">
          <MonthView month={focus} selectedDay={selectedDay} now={now} />
        </div>
        <div className="cal-divider" aria-hidden="true" />
        <div className="cal-wide-agenda">
          <AgendaHeader />
          <Agenda now={now} width={width} />
        </div>
      </div>
    );
  }
  if (showingMonth) return <MonthGrid month={focus} selectedDay={selectedDay} now={now} height={height} />;
  return (
    <div className="cal-short">
      <WeekStrip focus={focus} selectedDay={selectedDay} now={now} />
      <Agenda now={now} width={width} />
    </div>
  );
}

// MARK: Top row

const monthFormat = (style: "long" | "short") => new Intl.DateTimeFormat(undefined, { month: style, year: "numeric" });

let measureCanvas: HTMLCanvasElement | undefined;
function titleWidth(text: string) {
  measureCanvas ??= document.createElement("canvas");
  const ctx = measureCanvas.getContext("2d")!;
  ctx.font = "600 13px 'Inter Variable', Inter, sans-serif";
  return Math.ceil(ctx.measureText(text).width) + 14;
}

/** Whether the top row holds Open Calendar after the title and the arrows. */
function rowOpensCalendar(rowWidth: number, focus: number) {
  return rowWidth - titleWidth(monthFormat("short").format(focus)) - 12 >= NAVIGATION_WIDTH + OPEN_WIDTH;
}

/**
 * The month the page shows, in full where the row has room, abbreviated
 * where it hasn't. Over the week strip it opens the month grid, and over
 * that grid it closes it.
 */
export function MonthTitle() {
  const focus = useCalendar((s) => s.focus);
  const wide = useCalendar((s) => s.wide);
  const showingMonth = useCalendar((s) => s.showingMonth);
  // The island's top row is as wide as the page.
  const row = useCalendar((s) => s.width);
  const ready = useCalendar((s) => s.received && s.access === "allowed");
  if (!ready) return null;
  const long = monthFormat("long").format(focus);
  const fits = row === 0 || titleWidth(long) + 12 + NAVIGATION_WIDTH + OPEN_WIDTH <= row;
  const title = fits ? long : monthFormat("short").format(focus);
  if (wide) return <h2 className="cal-title">{title}</h2>;
  return (
    <button
      type="button"
      className={`cal-title cal-title-button${showingMonth ? " is-open" : ""}`}
      aria-pressed={showingMonth}
      aria-label={`${title}, ${C.month}`}
      title={C.month}
      onClick={calendarView.toggleMonth}
    >
      {title}
    </button>
  );
}

/** The arrows step by week over the strip and by month over a grid, around the way back to today. */
export function Navigation() {
  const focus = useCalendar((s) => s.focus);
  const byWeek = useCalendar((s) => !s.wide && !s.showingMonth);
  const row = useCalendar((s) => s.width);
  // The band carries the month only once the page shows one (NotchCalendarView: notchBand).
  const ready = useCalendar((s) => s.received && s.access === "allowed");
  if (!ready) return null;
  return (
    <div className="cal-navigation">
      <div className="cal-arrows">
        <IconButton
          icon={ChevronLeft}
          label={byWeek ? C.previousWeek : C.previousMonth}
          onClick={() => (byWeek ? calendarView.moveWeek(-1) : calendarView.moveMonth(-1))}
        />
        <IconButton icon={CircleDot} label={C.today} onClick={calendarView.today} />
        <IconButton
          icon={ChevronRight}
          label={byWeek ? C.nextWeek : C.nextMonth}
          onClick={() => (byWeek ? calendarView.moveWeek(1) : calendarView.moveMonth(1))}
        />
      </div>
      {row === 0 || rowOpensCalendar(row, focus) ? (
        <IconButton icon={SquareArrowOutUpRight} label={C.openCalendar} onClick={() => openCalendar()} />
      ) : null}
    </div>
  );
}

function openCalendar(event?: CalendarEvent) {
  void bridge.calendar.open(event?.localId || null, event?.instanceStart ?? null);
}

// MARK: Days

interface DayProps {
  day: number;
  now: number;
  selected: boolean;
  /** Outside the month the grid shows. */
  dimmed?: boolean;
  circle: number;
  fontSize: number;
  height: number;
  /** Between the number and its dots. */
  spacing: number;
  onClick(): void;
  letter?: boolean;
}

function Day(props: DayProps) {
  const events = useCalendar((s) => s.events);
  const isToday = sameDay(props.day, props.now);
  const dayEvents = eventsOn(events, props.day);
  const style = {
    "--cal-circle": `${props.circle}px`,
    "--cal-spacing": `${props.spacing}px`,
    height: props.height,
    fontSize: props.fontSize,
  } as CSSProperties;
  const label = [fullDate(props.day), isToday ? C.today : "", dayEvents.length ? C.hasEvents : ""]
    .filter(Boolean)
    .join(", ");
  return (
    <button
      type="button"
      className={`cal-day${isToday ? " is-today" : ""}${props.selected ? " is-selected" : ""}${props.dimmed ? " is-dimmed" : ""}`}
      style={style}
      aria-label={label}
      aria-pressed={props.selected}
      title={fullDate(props.day)}
      onClick={props.onClick}
    >
      {props.letter ? <span className="cal-weekday">{weekdayLetter(props.day)}</span> : null}
      <span className="cal-day-number">{new Date(props.day).getDate()}</span>
      <span className="cal-dots" aria-hidden="true">
        {dotColors(dayEvents).map((color) => (
          <span key={color} style={{ background: color }} />
        ))}
      </span>
    </button>
  );
}

/** One week across the page; a second click on the chosen day returns to the coming week. */
function WeekStrip({ focus, selectedDay, now }: { focus: number; selectedDay: number | null; now: number }) {
  const firstWeekday = useCalendar((s) => s.firstWeekday);
  return (
    <div className="cal-week" role="group" aria-label={monthFormat("long").format(focus)}>
      {weekDays(focus, firstWeekday).map((day) => {
        const selected = selectedDay !== null && sameDay(day, selectedDay);
        return (
          <Day
            key={day}
            day={day}
            now={now}
            selected={selected}
            circle={26}
            fontSize={12}
            height={52}
            spacing={2}
            letter
            onClick={() => (selected ? calendarView.week() : calendarView.select(day))}
          />
        );
      })}
    </div>
  );
}

function WeekdayHeader({ height, numbered }: { height: number; numbered: boolean }) {
  const firstWeekday = useCalendar((s) => s.firstWeekday);
  return (
    <>
      {numbered ? <span aria-hidden="true" /> : null}
      {weekDays(Date.now(), firstWeekday).map((day) => (
        <span key={day} className="cal-weekday" style={{ height }} aria-hidden="true">
          {weekdayLetter(day)}
        </span>
      ))}
    </>
  );
}

function WeekNumber({ day, height, circle }: { day: number; height: number; circle: number }) {
  const firstWeekday = useCalendar((s) => s.firstWeekday);
  const minimumDaysInFirstWeek = useCalendar((s) => s.minimumDaysInFirstWeek);
  const n = weekNumber(day, firstWeekday, minimumDaysInFirstWeek);
  return (
    <span className="cal-week-number" style={{ height }} aria-label={C.weekNumber(n)}>
      <span style={{ height: circle }}>{n}</span>
    </span>
  );
}

interface GridProps {
  month: number;
  selectedDay: number | null;
  now: number;
  rowHeight: number;
  weekdayHeight: number;
  circle: number;
  fontSize: number;
  gap: number;
  /** Between a day's number and its dots. */
  spacing: number;
  onPick(day: number): void;
}

function Grid(props: GridProps) {
  const firstWeekday = useCalendar((s) => s.firstWeekday);
  const numbered = useIsland((s) => s.settings.calendarWeekNumbers);
  return (
    <div className={`cal-grid${numbered ? " is-numbered" : ""}`} style={{ rowGap: props.gap }}>
      <WeekdayHeader height={props.weekdayHeight} numbered={numbered} />
      {monthDays(props.month, firstWeekday).map((day) => (
        <GridDay key={day} {...props} day={day} numbered={numbered && startsWeek(day, firstWeekday)} />
      ))}
    </div>
  );
}

function GridDay(props: GridProps & { day: number; numbered: boolean }) {
  return (
    <>
      {props.numbered ? <WeekNumber day={props.day} height={props.rowHeight} circle={props.circle} /> : null}
      <Day
        day={props.day}
        now={props.now}
        selected={props.selectedDay !== null && sameDay(props.day, props.selectedDay)}
        dimmed={!sameMonth(props.day, props.month)}
        circle={props.circle}
        fontSize={props.fontSize}
        height={props.rowHeight}
        spacing={props.spacing}
        onClick={() => props.onPick(props.day)}
      />
    </>
  );
}

/** The month beside the agenda of a tall island. */
function MonthView(props: { month: number; selectedDay: number | null; now: number }) {
  return (
    <Grid
      {...props}
      rowHeight={30}
      weekdayHeight={18}
      circle={24}
      fontSize={11}
      gap={4}
      spacing={2}
      onPick={(day) => calendarView.select(day)}
    />
  );
}

/** The month over a short island's strip, rows sized to what is left; a pick returns to the strip. */
function MonthGrid(props: { month: number; selectedDay: number | null; now: number; height: number }) {
  const rowHeight = monthRowHeight(props.height);
  const circle = Math.min(24, rowHeight - 3);
  return (
    <Grid
      {...props}
      rowHeight={rowHeight}
      weekdayHeight={MONTH_WEEKDAY_HEIGHT}
      circle={circle}
      fontSize={Math.min(11, circle * 0.62)}
      gap={0}
      spacing={0}
      onPick={(day) => {
        calendarView.select(day);
        calendarView.closeMonth();
      }}
    />
  );
}

// MARK: Agenda

const dayLabel = (day: number) => new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short" }).format(day);
const weekdayShort = (day: number) => new Intl.DateTimeFormat(undefined, { weekday: "short" }).format(day);

function AgendaHeader() {
  const selectedDay = useCalendar((s) => s.selectedDay);
  return (
    <div className="cal-agenda-header">
      <h3>{selectedDay === null ? C.week : `${weekdayShort(selectedDay)} ${dayLabel(selectedDay)}`}</h3>
      {selectedDay !== null ? <IconButton icon={CalendarClock} label={C.week} onClick={calendarView.week} /> : null}
    </div>
  );
}

function Spinner() {
  return (
    <div className="cal-loading" role="status" aria-label={C.loading}>
      <LoaderCircle size={16} className="cal-spinner" />
    </div>
  );
}

function Agenda({ now, width }: { now: number; width: number }) {
  const events = useCalendar((s) => s.events);
  const loading = useCalendar((s) => s.loading);
  const revealing = useCalendar((s) => s.revealing);
  const selectedDay = useCalendar((s) => s.selectedDay);
  const calendars = useCalendar((s) => s.calendars);
  const focus = useCalendar((s) => s.focus);
  const everyStart = useIsland((s) => s.settings.calendarCountdown);
  const chosen = useIsland((s) => s.settings.calendarChosenCountdowns);
  const list = useRef<HTMLDivElement>(null);
  const menu = useMenu();
  const groups = agendaGroups(events, now, selectedDay);
  const upNext = next(events, now);

  // Scrolls once to the countdown's event opened from the closed island.
  // A tick later: the page's own read starts in its parent's effect, after
  // this one, and the scroll waits for that read to arrive.
  useEffect(() => {
    if (!revealing || loading) return;
    const timer = setTimeout(() => {
      const state = useCalendar.getState();
      if (state.loading || state.revealing !== revealing) return;
      const target = list.current?.querySelector(`[data-event="${CSS.escape(revealing)}"]`);
      target?.scrollIntoView({ block: "center", behavior: "instant" });
      // Once: the event is scrolled to, or isn't on the page.
      useCalendar.setState({ revealing: null });
    });
    return () => clearTimeout(timer);
  }, [revealing, loading, events]);

  if (loading) return <Spinner />;
  if (groups.length === 0) {
    return (
      <div className="cal-empty">
        <CalendarCheck size={26} strokeWidth={1.25} aria-hidden="true" />
        <div>
          <p>{selectedDay === null ? C.empty : C.emptyDay}</p>
          {calendars.length === 0 ? <p className="cal-empty-hint">{C.noCalendars}</p> : null}
          {/* With no card to open it from and no room for it in the row. */}
          {!rowOpensCalendar(width, focus) ? (
            <PillButton prominent onClick={() => openCalendar()}>
              {C.openCalendar}
            </PillButton>
          ) : null}
        </div>
      </div>
    );
  }
  return (
    <div className="cal-agenda" ref={list} key={selectedDay ?? "week"}>
      {groups.map((group) => (
        <section key={group.day} className="cal-group">
          {selectedDay === null ? <DayLabel day={group.day} now={now} /> : null}
          {group.events.map((event) => {
            // A timed event yet to start offers its own countdown while the one for every event is off.
            const choice =
              !everyStart && !event.allDay && event.start > now && event.countdownKey
                ? isChosen(event, new Set(Object.keys(chosen)))
                : null;
            return (
              <EventCard
                key={event.id}
                event={event}
                day={group.day}
                now={now}
                isNext={event.id === upNext?.id}
                chosen={choice === true}
                onMenu={
                  choice === null
                    ? undefined
                    : (x, y) =>
                        menu.open({ x, y }, [
                          {
                            label: choice ? C.removeCountdown : C.addCountdown,
                            icon: Timer,
                            onSelect: () => setCountdownChoice(event, !choice),
                          },
                        ])
                }
              />
            );
          })}
        </section>
      ))}
      {menu.menu}
    </div>
  );
}

function DayLabel({ day, now }: { day: number; now: number }) {
  const today = sameDay(day, now);
  return (
    <div className="cal-day-label">
      {today ? <span className="is-today">{C.today}</span> : <span>{weekdayShort(day)}</span>}
      <span>{dayLabel(day)}</span>
    </div>
  );
}

function EventCard(props: {
  event: CalendarEvent;
  day: number;
  now: number;
  isNext: boolean;
  chosen: boolean;
  onMenu?: (x: number, y: number) => void;
}) {
  const { event, now } = props;
  const ongoing = !event.allDay && event.start <= now && event.end > now;
  const ended = event.end <= now;
  return (
    <button
      type="button"
      data-event={event.id}
      className={`cal-card${ongoing ? " is-ongoing" : ""}${ended ? " is-ended" : ""}`}
      style={{ "--cal-color": event.color } as CSSProperties}
      title={C.openCalendar}
      onClick={() => openCalendar(event)}
      onContextMenu={(e) => {
        e.preventDefault();
        props.onMenu?.(e.clientX, e.clientY);
      }}
    >
      {ongoing || props.isNext ? (
        <span className={`cal-kicker${ongoing ? " is-ongoing" : ""}`}>{ongoing ? C.ongoing : C.next}</span>
      ) : null}
      <span className="cal-card-title">{event.title.trim() || C.untitled}</span>
      <span className="cal-card-time">
        {timeLine(event, props.day, C.allDay)}
        {props.chosen ? <Timer size={11} aria-label={C.countdown} /> : null}
      </span>
      <span className="cal-card-calendar">{event.calendar}</span>
      {event.location ? (
        <span className="cal-card-location">
          <MapPin size={10} aria-hidden="true" />
          {event.location}
        </span>
      ) : null}
    </button>
  );
}

// MARK: Access

function PermissionCard() {
  const access = useCalendar((s) => s.access);
  const requesting = useCalendar((s) => s.requesting);
  const failed = useCalendar((s) => s.requestFailed);
  return (
    <div className="cal-permission">
      <CalendarClock size={28} strokeWidth={1.5} aria-hidden="true" />
      {access === "unavailable" ? <p className="cal-permission-title">{C.unavailableTitle}</p> : null}
      <p>{access === "unavailable" ? C.unavailable : access === "denied" ? C.denied : C.permission}</p>
      {access === "unavailable" ? null : (
        <div className="cal-permission-actions">
          {access === "denied" || failed ? (
            <PillButton prominent onClick={() => void bridge.calendar.openPrivacySettings()}>
              {C.settings}
            </PillButton>
          ) : (
            <PillButton prominent onClick={() => void (requesting || requestAccess())}>
              {C.allow}
            </PillButton>
          )}
          {requesting ? <LoaderCircle size={14} className="cal-spinner" aria-label={C.loading} /> : null}
        </div>
      )}
      {failed ? <p className="cal-permission-failed">{C.requestFailed}</p> : null}
    </div>
  );
}
