// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The AI page's pure logic: which cards show and how they pair into rows
// (NotchAgentSupport.swift:253-331), how a limit window reads now
// (AgentLimitSupport in AgentUsageSummary.swift:406-469), and what the closed
// island shows beside the camera (NotchAgentSupport.stripReading).

import { clock, cost, duration, percent, reportsLimits, tokens } from "./format";
import { A } from "./strings";
import {
  AGENT_CARDS,
  type AgentCard,
  type AgentReadout,
  type AgentSnapshot,
  type LimitDisplay,
  type LimitFocus,
  type LimitWindow,
  type Limits,
  type PeriodUsage,
  PROVIDERS,
  type Provider,
  type Totals,
} from "./types";

// MARK: Cards and rows

export const SPACING = 10;
export const CARD_HEIGHT = 96;
export const CHART_HEIGHT = 118;
/** An account card also says when it was checked, and the Resets card beside account cards names its account. */
export const ACCOUNT_CARD_HEIGHT = 116;
/** An account card's reading, checked every five minutes, has missed two. */
export const ACCOUNT_STALENESS = 10 * 60_000;
/** Below this width every card takes a row of its own. */
export const PAIR_WIDTH = 390;
/** The page's height while it says it is empty (NotchLayout.emptyHeight). */
export const EMPTY_HEIGHT = 140;

/** Charts need the island's width; everything else pairs up. */
export const fullWidth = (card: AgentCard) => card === "trend" || card === "activity";
/** Cards that cover the period chosen in the island's top row. */
export const followsPeriod = (card: AgentCard) =>
  card === "spend" || card === "trend" || card === "models" || card === "projects";

export interface Tile {
  card: AgentCard;
  /** The agent a limits card belongs to. */
  provider: Provider | null;
  /** The added Codex home a limits card belongs to. */
  account?: string;
  /** The Resets card beside added Codex homes, which says whose resets they are. */
  namesDefaultAccount?: boolean;
  id: string;
}

/** Every card in the saved order; cards added later join at the end. */
export function orderedCards(order: readonly AgentCard[]): AgentCard[] {
  return [...new Set([...order.filter((c) => AGENT_CARDS.includes(c)), ...AGENT_CARDS])];
}

export function shownCards(order: readonly AgentCard[], hidden: readonly AgentCard[]): AgentCard[] {
  return orderedCards(order).filter((c) => !hidden.includes(c));
}

/**
 * One limits card per agent that reports limits; Codex homes the person
 * added take the place of Codex's one card. Resets only while Codex is read:
 * banked resets belong to the default account, which the card names once
 * others have cards.
 */
export function tiles(
  cards: readonly AgentCard[],
  providers: readonly Provider[],
  accounts: readonly string[] = [],
): Tile[] {
  return cards.flatMap((card): Tile[] => {
    if (card === "limits")
      return providers
        .filter(reportsLimits)
        .flatMap((provider): Tile[] =>
          provider === "codex" && accounts.length > 0
            ? accounts.map((account) => ({ card, provider, account, id: `limits.codex.${account}` }))
            : [{ card, provider, id: `limits.${provider}` }],
        );
    if (card === "resets")
      return providers.includes("codex")
        ? [{ card, provider: "codex", namesDefaultAccount: accounts.length > 0, id: "resets.codex" }]
        : [];
    return [{ card, provider: null, id: card }];
  });
}

/** Cards pair up in reading order; a chart, or a card left without a partner, takes the whole row. */
export function rows(list: readonly Tile[], width: number): Tile[][] {
  const pairs = width >= PAIR_WIDTH;
  const out: Tile[][] = [];
  let waiting: Tile | null = null;
  for (const tile of list) {
    if (fullWidth(tile.card) || !pairs) {
      if (waiting) out.push([waiting]);
      waiting = null;
      out.push([tile]);
    } else if (waiting) {
      out.push([waiting, tile]);
      waiting = null;
    } else waiting = tile;
  }
  if (waiting) out.push([waiting]);
  return out;
}

export function rowHeight(row: readonly Tile[]): number {
  if (row.some((t) => fullWidth(t.card))) return CHART_HEIGHT;
  return row.some((t) => t.account !== undefined || t.namesDefaultAccount) ? ACCOUNT_CARD_HEIGHT : CARD_HEIGHT;
}

export function contentHeight(list: readonly Tile[][]): number {
  if (list.length === 0) return 0;
  return list.reduce((sum, row) => sum + rowHeight(row), 0) + SPACING * (list.length - 1);
}

/** Agents switched on in Settings that left something on this PC, or whose accounts were added: only they get cards. */
export function pageProviders(
  snapshot: AgentSnapshot,
  enabled: Record<Provider, boolean>,
  hasAccounts = false,
): Provider[] {
  return PROVIDERS.filter((p) => enabled[p] && (snapshot.seen.includes(p) || (p === "codex" && hasAccounts)));
}

/** Added Codex homes have cards only in the Limits card's place (NotchAgentSupport.showsCodexAccounts). */
export const showsCodexAccounts = (codexOn: boolean, hidden: readonly AgentCard[]) =>
  codexOn && !hidden.includes("limits");

/**
 * An account card's line in place of its windows while it has none, or
 * under them: a failure takes one of the two, never both.
 */
export function accountNotes(
  hasWindows: boolean,
  checking: boolean,
  failure: string | null,
  updated: string | null,
): { placeholder: string | null; caption: string | null } {
  if (!hasWindows) return { placeholder: checking ? A.loading : (failure ?? A.accountRefreshHint), caption: null };
  return { placeholder: null, caption: failure ?? updated };
}

// MARK: Limits

/** A window that renewed since it was read has nothing spent on it yet. */
export function current(window: LimitWindow, now: number): LimitWindow {
  if (window.resetsAt === null || window.resetsAt > now) return window;
  return { ...window, usedPercent: 0, resetsAt: null, estimatedResetsAt: null };
}

/** Once an estimate passed, the countdown is unknown until another reading places it. */
export function displayedReset(window: LimitWindow, now: number): number | null {
  const reset = window.estimatedResetsAt ?? window.resetsAt;
  return reset !== null && reset > now ? reset : null;
}

/** How much of the window's time has passed, 0…1. */
export function pace(window: LimitWindow, now: number): number | null {
  const resets = displayedReset(window, now);
  if (!window.minutes || window.minutes <= 0 || resets === null) return null;
  const length = window.minutes * 60_000;
  const elapsed = now - (resets - length);
  return elapsed > 0 ? Math.min(1, elapsed / length) : null;
}

export const usedFraction = (w: LimitWindow) => Math.min(1, Math.max(0, w.usedPercent / 100));

/** The window that stops work first: the most spent, and on a tie the one renewing last. */
export function binding(limits: Limits | undefined, now: number): LimitWindow | null {
  let best: LimitWindow | null = null;
  for (const raw of limits?.windows ?? []) {
    const w = current(raw, now);
    if (
      !best ||
      w.usedPercent > best.usedPercent ||
      (w.usedPercent === best.usedPercent && (w.resetsAt ?? 0) > (best.resetsAt ?? 0))
    )
      best = w;
  }
  return best;
}

/** The window the person chose, or the one closest to running out; never a model's own allowance. */
export function focusedLimit(limits: Limits | undefined, focus: LimitFocus, now: number): LimitWindow | null {
  if (focus !== "mostUsed") {
    const kind = focus === "session" ? "session" : "weekly";
    const chosen = limits?.windows.find((w) => w.kind === kind && w.scope === null);
    if (chosen) return current(chosen, now);
  }
  return binding(limits, now);
}

/** Most spent chosen window across accounts; binding windows if none reports that kind. */
export function restingLimit(snapshot: AgentSnapshot, focus: LimitFocus, now: number) {
  const limits = [
    ...Object.values(snapshot.limits),
    ...snapshot.codexAccounts.flatMap((account) => (account.limits ? [account.limits] : [])),
  ];
  const mostSpent = (chosen: boolean) =>
    limits
      .flatMap((limits) => {
        const raw =
          chosen && focus !== "mostUsed"
            ? limits.windows.find((window) => window.kind === focus && window.scope === null)
            : binding(limits, now);
        return raw ? [{ provider: limits.provider, window: current(raw, now) }] : [];
      })
      .sort((a, b) => b.window.usedPercent - a.window.usedPercent || a.provider.localeCompare(b.provider))[0] ?? null;
  return mostSpent(true) ?? mostSpent(false);
}

/** Two rows fit: the session and whichever longer window binds first. */
export function cardWindows(limits: Limits | undefined, now: number): LimitWindow[] {
  const all = (limits?.windows ?? []).map((w) => current(w, now));
  const session = all.find((w) => w.kind === "session");
  let longer: LimitWindow | undefined;
  for (const w of all) if (w.kind !== "session" && (!longer || w.usedPercent > longer.usedPercent)) longer = w;
  return [session, longer].filter((w): w is LimitWindow => w !== undefined);
}

/** "Session", "Week", "Week · Opus", or the window's length. */
export function windowLabel(window: LimitWindow): string {
  if (window.kind === "session") return A.session;
  if (window.kind === "weekly") return window.scope ? `${A.weekly} · ${window.scope}` : A.weekly;
  if (window.minutes === null) return window.scope ?? A.readoutLimit;
  return duration(window.minutes * 60, 1);
}

/**
 * A reading old enough to have missed use elsewhere shows quieter: an
 * account card's after missed checks, and among the others only Claude's,
 * which the Claude app or Claude Code took a while ago.
 */
export const CLAUDE_FRESHNESS = 30 * 60_000;
export function limitsAreStale(
  limits: Limits | null | undefined,
  provider: Provider,
  now: number,
  account = false,
): boolean {
  if (!limits) return false;
  const age = now - limits.observedAt;
  return account ? age >= ACCOUNT_STALENESS : provider === "claude" && age >= CLAUDE_FRESHNESS;
}

/** "20% left" or "80% used". */
export function limitShare(window: LimitWindow, display: LimitDisplay): string {
  const used = usedFraction(window);
  return display === "used" ? A.used(percent(used)) : A.left(percent(1 - used));
}

// MARK: Totals

/** Cost when every response had a price, tokens otherwise. */
export function weight(totals: Totals | undefined, byCost: boolean): number {
  if (!totals) return 0;
  return byCost
    ? totals.cost
    : totals.tokens.input + totals.tokens.cacheWrite + totals.tokens.cacheRead + totals.tokens.output;
}

export const fullyPriced = (usage: PeriodUsage) => usage.total.unpriced === 0;

export function sumTotals(list: readonly (Totals | undefined)[]): Totals {
  const out: Totals = {
    tokens: { input: 0, cacheWrite: 0, cacheRead: 0, output: 0, reasoning: 0 },
    cost: 0,
    savings: 0,
    requests: 0,
    unpriced: 0,
  };
  for (const t of list) {
    if (!t) continue;
    out.tokens.input += t.tokens.input;
    out.tokens.cacheWrite += t.tokens.cacheWrite;
    out.tokens.cacheRead += t.tokens.cacheRead;
    out.tokens.output += t.tokens.output;
    out.tokens.reasoning += t.tokens.reasoning;
    out.cost += t.cost;
    out.savings += t.savings;
    out.requests += t.requests;
    out.unpriced += t.unpriced;
  }
  return out;
}

// MARK: Strip

/** Agents at work now, in provider order. */
export function workingProviders(snapshot: AgentSnapshot): Provider[] {
  return PROVIDERS.filter((p) => snapshot.live.some((s) => s.provider === p));
}

/** The reading beside the camera: the chosen one, or the time elapsed while that one is unknown. */
export function stripReading(
  snapshot: AgentSnapshot,
  readout: AgentReadout,
  display: LimitDisplay,
  focus: LimitFocus,
  now: number,
): string {
  const live = snapshot.live;
  const elapsed = () => clock((now - Math.min(now, ...live.map((s) => s.started))) / 1000);
  switch (readout) {
    case "elapsed":
      return elapsed();
    case "tokens":
      return tokens(live.reduce((sum, s) => sum + s.tokens.output, 0));
    case "cost":
      return cost(live.reduce((sum, s) => sum + s.cost, 0));
    case "limit": {
      const provider = workingProviders(snapshot)[0];
      const window = provider ? focusedLimit(snapshot.limits[provider], focus, now) : null;
      if (!window) return elapsed();
      const used = usedFraction(window);
      return percent(display === "used" ? used : 1 - used);
    }
  }
}

/** Every digit the same width, so the shape, not the value, sets the strip's width. */
export const readingShape = (reading: string) => reading.replace(/[0-9]/g, "0");

/** Tokens and cost change only with a snapshot; time and limits need the clock. */
export const advancesWithClock = (readout: AgentReadout) => readout === "elapsed" || readout === "limit";

/** Days in a row with any use, counting today only once it has some. */
export function streak(snapshot: AgentSnapshot): number {
  let count = 0;
  const days = snapshot.days;
  for (let i = days.length - 1; i >= 0; i--) {
    const requests = sumTotals(Object.values(days[i]!.byProvider)).requests;
    if (requests > 0) count++;
    else if (count > 0 || i !== days.length - 1) break;
  }
  return count;
}

/** Quartiles of the active days, so one heavy day does not wash out the rest. */
export function heatLevels(values: readonly number[]): number[] {
  const active = values.filter((v) => v > 0).sort((a, b) => a - b);
  if (active.length === 0) return [];
  return [0.25, 0.5, 0.75].map((q) => active[Math.min(active.length - 1, Math.floor(active.length * q))]!);
}

export function heatOpacity(value: number, levels: readonly number[]): number {
  if (value <= 0) return 0.07;
  const level = levels.filter((l) => value > l).length;
  return [0.28, 0.45, 0.65, 0.9][Math.min(3, level)]!;
}

/** Days laid into week columns that start on the reader's first weekday (0 = Sunday). */
export function weekColumns<T extends { start: number }>(days: readonly T[], firstWeekday: number): (T | null)[][] {
  const weeks: (T | null)[][] = [];
  let currentWeek: (T | null)[] = Array(7).fill(null);
  for (const day of days) {
    const weekday = (new Date(day.start).getDay() - firstWeekday + 7) % 7;
    if (weekday === 0 && currentWeek.some((d) => d !== null)) {
      weeks.push(currentWeek);
      currentWeek = Array(7).fill(null);
    }
    currentWeek[weekday] = day;
  }
  if (currentWeek.some((d) => d !== null)) weeks.push(currentWeek);
  return weeks;
}

/** The locale's first weekday (0 = Sunday), Monday where Intl cannot say. */
export function firstWeekday(locale = navigator.language): number {
  try {
    const info =
      (
        new Intl.Locale(locale) as Intl.Locale & {
          getWeekInfo?: () => { firstDay: number };
          weekInfo?: { firstDay: number };
        }
      ).getWeekInfo?.() ?? (new Intl.Locale(locale) as Intl.Locale & { weekInfo?: { firstDay: number } }).weekInfo;
    if (info) return info.firstDay % 7;
  } catch {
    // An unknown locale falls through.
  }
  return 1;
}
