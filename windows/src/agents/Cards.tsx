// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The AI page's cards (NotchAgentsView.swift:186-835; spec 03 §10.3):
// limits per agent, spending, the turns at work, the trend, models,
// projects and thirteen weeks of activity.

import {
  ArrowUpRight,
  ChartColumn,
  CircleDollarSign,
  Cpu,
  Flame,
  Folder,
  Grid3x3,
  RotateCw,
  Waves,
} from "lucide-react";
import { useState } from "react";
import { bridge } from "../shared/bridge";
import {
  cacheHitRate,
  clock,
  cost,
  countdown,
  dateTime,
  modelName,
  percent,
  PROVIDER_NAMES,
  PROVIDER_TINTS,
  limitTint,
  relative,
  shortDate,
  tokens,
  tokenTotal,
} from "./format";
import {
  CHART_HEIGHT,
  cardWindows,
  displayedReset,
  firstWeekday,
  fullyPriced,
  heatLevels,
  heatOpacity,
  limitsAreStale,
  pace,
  streak,
  sumTotals,
  usedFraction,
  weekColumns,
  weight,
  windowLabel,
} from "./model";
import { Card, CardHeader, Chip, Glyph, Meter, Pulse } from "./Parts";
import { A, periodName } from "./strings";
import type {
  AgentPeriod,
  AgentSnapshot,
  Bucket,
  LimitDisplay,
  LimitWindow,
  LiveSession,
  Provider,
  Share,
  Totals,
} from "./types";
import { useClock } from "./useClock";

// MARK: Limits

export function LimitsCard(props: { provider: Provider; snapshot: AgentSnapshot; now: number; display: LimitDisplay }) {
  const { provider, snapshot, now, display } = props;
  const limits = snapshot.limits[provider];
  const windows = cardWindows(limits, now);
  const stale = limitsAreStale(limits, provider, now);
  const tint = PROVIDER_TINTS[provider];
  const plan = snapshot.plans[provider];
  const working = snapshot.live.some((s) => s.provider === provider);
  const old = limits && now - limits.observedAt > 600_000 ? A.updated(relative(limits.observedAt, now)) : null;
  return (
    <Card>
      <CardHeader title={PROVIDER_NAMES[provider]} provider={provider} tint={tint}>
        {plan ? <Chip tint={tint}>{plan.name}</Chip> : null}
        {working ? <Pulse tint={tint} /> : null}
      </CardHeader>
      {windows.length === 0 ? (
        <Estimate provider={provider} snapshot={snapshot} now={now} />
      ) : (
        <>
          <div className="ag-meters" style={{ opacity: stale ? 0.6 : 1 }}>
            {windows.map((w) => (
              <LimitRow key={w.id} provider={provider} window={w} now={now} display={display} old={old} />
            ))}
          </div>
          {windows.length === 1 && old ? <div className="ag-caption is-tertiary">{old}</div> : null}
        </>
      )}
    </Card>
  );
}

function LimitRow(props: {
  provider: Provider;
  window: LimitWindow;
  now: number;
  display: LimitDisplay;
  old: string | null;
}) {
  const { provider, window, now, display } = props;
  const used = usedFraction(window);
  const remaining = display === "remaining";
  const fraction = remaining ? 1 - used : used;
  const tint = limitTint(provider, used);
  const elapsed = pace(window, now);
  const resets = displayedReset(window, now);
  const label = windowLabel(window);
  const help = [label, A.used(percent(used)), A.left(percent(1 - used))];
  if (resets !== null) {
    help.push(dateTime(resets));
    if (window.estimatedResetsAt !== null) help.push(A.estimated);
  }
  if (props.old) help.push(props.old);
  return (
    <div className="ag-limit" title={help.join(" · ")}>
      <div className="ag-limit-line">
        <span className="ag-limit-label">{label}</span>
        {resets !== null ? (
          <span className="ag-countdown">
            <RotateCw size={8} strokeWidth={2.5} aria-hidden />
            {(window.estimatedResetsAt !== null ? "≈ " : "") + countdown(resets, now)}
          </span>
        ) : null}
        <span className="ag-spacer" />
        <span className="ag-percent" style={{ color: tint === PROVIDER_TINTS[provider] ? undefined : tint }}>
          {percent(fraction)}
        </span>
      </div>
      <Meter value={fraction} pace={elapsed === null ? null : remaining ? 1 - elapsed : elapsed} tint={tint} />
    </div>
  );
}

/** Without a reading: Claude's 5-hour block from this PC's use, OpenCode's day, or Codex's wait. */
function Estimate({ provider, snapshot, now }: { provider: Provider; snapshot: AgentSnapshot; now: number }) {
  const tint = PROVIDER_TINTS[provider];
  const block = snapshot.claudeBlock;
  if (provider === "claude") {
    return (
      <>
        {block ? (
          <div className="ag-limit" title={A.estimated}>
            <div className="ag-limit-line">
              <span className="ag-limit-label">{A.session}</span>
              <span className="ag-countdown">
                <RotateCw size={8} strokeWidth={2.5} aria-hidden />
                {countdown(block.end, now)}
              </span>
              <span className="ag-spacer" />
              <span className="ag-percent">{cost(block.totals.cost)}</span>
            </div>
            <Meter
              value={block.end > block.start ? (now - block.start) / (block.end - block.start) : 0}
              tint={tint}
              dimmed
            />
          </div>
        ) : (
          <div className="ag-note">{A.noSession}</div>
        )}
        <button
          type="button"
          className="ag-link"
          style={{ color: tint }}
          onClick={() => void bridge.openSettings()}
          title={A.claudeLimitsHint}
        >
          {A.claudeLimitsHint}
          <ArrowUpRight size={10} strokeWidth={2.5} aria-hidden />
        </button>
      </>
    );
  }
  const last = snapshot.lastActivity[provider];
  const lastUsed = last ? <span className="ag-caption is-tertiary">{relative(last, now)}</span> : null;
  if (provider === "opencode") {
    const today = snapshot.periods.today.byProvider.opencode;
    if (today && (tokenTotal(today.tokens) > 0 || today.requests > 0 || today.cost > 0)) {
      const value = today.unpriced > 0 ? (today.cost > 0 ? `≥ ${cost(today.cost)}` : "—") : cost(today.cost);
      const rate = cacheHitRate(today.tokens);
      return (
        <div className="ag-limit">
          <div className="ag-limit-line" title={today.unpriced > 0 ? A.unpriced : A.valueNote}>
            <span className="ag-limit-label">{A.today}</span>
            <span className="ag-spacer" />
            <span className="ag-percent">{value}</span>
          </div>
          <div className="ag-limit-line">
            <span className="ag-caption">{tokens(tokenTotal(today.tokens))}</span>
            {rate && rate > 0 ? <span className="ag-caption is-tertiary">· {A.cached(percent(rate))}</span> : null}
            <span className="ag-spacer" />
            {lastUsed}
          </div>
        </div>
      );
    }
    return (
      <>
        <div className="ag-note">{A.noSession}</div>
        {lastUsed}
      </>
    );
  }
  return (
    <>
      <div className="ag-note is-two-lines">{A.waitingForLimits}</div>
      {lastUsed}
    </>
  );
}

// MARK: Spending

export function SpendCard(props: { snapshot: AgentSnapshot; providers: Provider[]; period: AgentPeriod }) {
  const { snapshot, providers, period } = props;
  const usage = snapshot.periods[period];
  const priced = fullyPriced(usage);
  const rate = cacheHitRate(usage.total.tokens);
  const footer = [A.tokens(tokens(tokenTotal(usage.total.tokens)))];
  if (rate && rate > 0) footer.push(A.cached(percent(rate)));
  return (
    <Card>
      <CardHeader title={A.spendCard} icon={CircleDollarSign}>
        {period === "month"
          ? providers.map((provider) => {
              const plan = snapshot.plans[provider];
              const spent = usage.byProvider[provider]?.cost ?? 0;
              if (!plan?.monthlyPrice || plan.monthlyPrice <= 0 || spent <= 0) return null;
              const ratio = spent / plan.monthlyPrice;
              const multiple = `${ratio.toFixed(ratio < 10 ? 1 : 0)}×`;
              return (
                <Chip
                  key={provider}
                  tint={PROVIDER_TINTS[provider]}
                  title={A.planMultiple(multiple, `${PROVIDER_NAMES[provider]} ${plan.name}`)}
                >
                  {multiple}
                </Chip>
              );
            })
          : null}
      </CardHeader>
      <div className="ag-spend-line" title={priced ? A.valueNote : A.unpriced}>
        <span className="ag-spend-value">{(priced ? "" : "≥ ") + cost(usage.total.cost)}</span>
        <span className="ag-caption">{A.apiValue}</span>
      </div>
      <SplitBar usage={usage.byProvider} providers={providers} byCost={priced} />
      <div className="ag-footer" title={A.saved(cost(usage.total.savings))}>
        {footer.join(" · ")}
      </div>
    </Card>
  );
}

/** Every agent keeps a 4 pt sliver; the rest shares what is left, with 2 pt gaps. */
function SplitBar(props: { usage: Partial<Record<Provider, Totals>>; providers: Provider[]; byCost: boolean }) {
  const parts = props.providers.map((p) => ({ provider: p, value: weight(props.usage[p], props.byCost) }));
  const shown = parts.filter((p) => p.value > 0);
  const total = shown.reduce((sum, p) => sum + p.value, 0);
  if (total <= 0) return <div className="ag-split is-empty" aria-hidden />;
  const fixed = 4 * shown.length + 2 * (shown.length - 1);
  return (
    <div className="ag-split" aria-hidden>
      {shown.map((p) => (
        <span
          key={p.provider}
          style={{
            background: PROVIDER_TINTS[p.provider],
            width: `calc(4px + (100% - ${fixed}px) * ${p.value / total})`,
          }}
        />
      ))}
    </div>
  );
}

// MARK: Now

export function LiveCard(props: { snapshot: AgentSnapshot; providers: Provider[]; now: number }) {
  const live = props.snapshot.live.filter((s) => props.providers.includes(s.provider));
  const first = live[0];
  return (
    <Card>
      <CardHeader title={A.liveCard} icon={Waves} tint={first ? PROVIDER_TINTS[first.provider] : undefined}>
        {live.length > 2 ? <Chip>+{live.length - 2}</Chip> : null}
        {first ? <Pulse tint={PROVIDER_TINTS[first.provider]} /> : null}
      </CardHeader>
      {live.length === 0 ? (
        <div className="ag-rows">
          {props.providers.map((provider) => {
            const last = props.snapshot.lastActivity[provider];
            return (
              <div key={provider} className="ag-idle-row">
                <Glyph provider={provider} size={9} working={false} />
                <span className="ag-idle-name">{PROVIDER_NAMES[provider]}</span>
                <span className="ag-spacer" />
                <span className="ag-caption">{last ? relative(last, props.now) : A.idle}</span>
              </div>
            );
          })}
        </div>
      ) : (
        <LiveRows live={live.slice(0, 2)} />
      )}
    </Card>
  );
}

/** Ticks every second for the elapsed clocks. */
function LiveRows({ live }: { live: LiveSession[] }) {
  const now = useClock(1000);
  return (
    <div className="ag-rows">
      {live.map((session) => {
        const sub = [
          modelName(session.model),
          session.tokens.output > 0 ? A.written(tokens(session.tokens.output)) : "",
          session.cost > 0 ? cost(session.cost) : "",
        ].filter(Boolean);
        return (
          <div
            key={session.id}
            className="ag-live-row"
            title={`${A.tokens(tokens(tokenTotal(session.tokens)))} · ${A.cached(percent(cacheHitRate(session.tokens) ?? 0))}`}
          >
            <div className="ag-limit-line">
              <Glyph provider={session.provider} size={9} />
              <span className="ag-live-name">
                {middleTruncate(session.project || PROVIDER_NAMES[session.provider])}
              </span>
              <span className="ag-spacer" />
              <span className="ag-live-clock" style={{ color: PROVIDER_TINTS[session.provider] }}>
                {clock((now - session.started) / 1000)}
              </span>
            </div>
            <div className="ag-caption">{sub.join(" · ")}</div>
          </div>
        );
      })}
    </div>
  );
}

/** Long project names lose their middle, so both ends still read. */
export function middleTruncate(text: string, max = 28): string {
  if (text.length <= max) return text;
  const head = Math.ceil((max - 1) / 2);
  return `${text.slice(0, head)}…${text.slice(text.length - (max - 1 - head))}`;
}

// MARK: Trend

export function TrendCard(props: { snapshot: AgentSnapshot; providers: Provider[]; period: AgentPeriod }) {
  const { snapshot, providers, period } = props;
  const [hovered, setHovered] = useState<number | null>(null);
  const buckets = period === "today" ? snapshot.hours : snapshot.days.slice(-(period === "week" ? 7 : 30));
  const byCost = fullyPriced(snapshot.periods[period]);
  const value = (t: Totals, inCost: boolean) => (inCost ? cost(t.cost) : A.tokens(tokens(tokenTotal(t.tokens))));
  const label = (ms: number, long: boolean) => {
    const date = new Date(ms);
    if (period === "today") return date.toLocaleTimeString("en-US", { hour: "numeric" });
    if (period === "week" && !long) return date.toLocaleDateString("en-US", { weekday: "narrow" });
    return shortDate(ms);
  };
  const pointed = hovered !== null ? buckets.find((b) => b.start === hovered) : undefined;
  const caption = pointed
    ? [
        label(pointed.start, true),
        value(total(pointed), byCost),
        ...(byCost ? [value(total(pointed), false)] : []),
      ].join(" · ")
    : `${periodName(period)} · ${value(snapshot.periods[period].total, byCost)}`;
  const marks =
    period === "week"
      ? buckets.map((_, i) => i)
      : buckets.length
        ? [0, Math.floor(buckets.length / 2), buckets.length - 1]
        : [];
  return (
    <Card>
      <CardHeader title={A.trendCard} icon={ChartColumn}>
        <span className="ag-caption is-medium">{caption}</span>
      </CardHeader>
      <Bars buckets={buckets} providers={providers} byCost={byCost} hovered={hovered} onHover={setHovered} />
      <div className={`ag-axis${period === "week" ? " is-even" : ""}`} aria-hidden>
        {marks.map((i) => (
          <span key={i}>{label(buckets[i]!.start, false)}</span>
        ))}
      </div>
    </Card>
  );
}

const total = (bucket: Bucket) => sumTotals(Object.values(bucket.byProvider));

/** Stacked bars, one per bucket; hovering one dims the rest to half. */
function Bars(props: {
  buckets: Bucket[];
  providers: Provider[];
  byCost: boolean;
  hovered: number | null;
  onHover(start: number | null): void;
}) {
  const values = props.buckets.map((b) => props.providers.map((p) => weight(b.byProvider[p], props.byCost)));
  const peak = Math.max(...values.map((v) => v.reduce((a, b) => a + b, 0)), Number.MIN_VALUE);
  const gap = props.buckets.length > 24 ? 2 : 4;
  return (
    <div className="ag-bars" style={{ gap }} onPointerLeave={() => props.onHover(null)} aria-hidden>
      {props.buckets.map((bucket, index) => {
        const parts = values[index]!;
        const sum = parts.reduce((a, b) => a + b, 0);
        const dim = props.hovered !== null && props.hovered !== bucket.start;
        return (
          <span key={bucket.start} className="ag-bar" onPointerEnter={() => props.onHover(bucket.start)}>
            {sum <= 0 ? (
              <span className="ag-bar-empty" />
            ) : (
              <span className="ag-bar-stack" style={{ height: `${(sum / peak) * 100}%` }}>
                {props.providers.map((p, slot) =>
                  parts[slot]! > 0 ? (
                    <span
                      key={p}
                      style={{
                        flexGrow: parts[slot]!,
                        background: PROVIDER_TINTS[p],
                        opacity: dim ? 0.5 : 0.95,
                      }}
                    />
                  ) : null,
                )}
              </span>
            )}
          </span>
        );
      })}
    </div>
  );
}

// MARK: Models and projects

export function ShareCard(props: { title: string; kind: "models" | "projects"; shares: Share[]; byCost: boolean }) {
  const shown = props.shares.slice(0, 3);
  const peak = Math.max(...shown.map((s) => weight(s.totals, props.byCost)), Number.MIN_VALUE);
  return (
    <Card>
      <CardHeader title={props.title} icon={props.kind === "models" ? Cpu : Folder} />
      {shown.length === 0 ? (
        <div className="ag-note">{A.noActivity}</div>
      ) : (
        <div className="ag-rows">
          {shown.map((share) => {
            const tint = share.provider ? PROVIDER_TINTS[share.provider] : "#ffffff";
            return (
              <div
                key={share.id}
                className="ag-share"
                title={[share.name, A.tokens(tokens(tokenTotal(share.totals.tokens))), cost(share.totals.cost)].join(
                  " · ",
                )}
              >
                <span className="ag-share-name">{middleTruncate(share.name, 24)}</span>
                <span className="ag-share-track">
                  <span
                    style={{
                      width: Math.max(3, (44 * weight(share.totals, props.byCost)) / peak),
                      background: tint,
                    }}
                  />
                </span>
                <span className="ag-share-value">
                  {props.byCost ? cost(share.totals.cost) : tokens(tokenTotal(share.totals.tokens))}
                </span>
              </div>
            );
          })}
        </div>
      )}
    </Card>
  );
}

// MARK: Activity

/** The heatmap's height inside a 118 pt card: padding, the 17 pt header and its 6 pt gap. */
const MAP_HEIGHT = CHART_HEIGHT - 20 - 17 - 6;
const CELL_GAP = 2.5;

export function ActivityCard({ snapshot }: { snapshot: AgentSnapshot }) {
  const [hovered, setHovered] = useState<number | null>(null);
  const days = snapshot.days;
  const totals = days.map(total);
  const byCost = totals.every((t) => t.unpriced === 0);
  const values = totals.map((t) => weight(t, byCost));
  const levels = heatLevels(values);
  const columns = weekColumns(
    days.map((d, i) => ({ start: d.start, index: i })),
    firstWeekday(),
  );
  const cell = Math.max(1, (MAP_HEIGHT - CELL_GAP * 6) / 7);
  const mapWidth = columns.length * cell + Math.max(0, columns.length - 1) * CELL_GAP;
  const value = (t: Totals) => (byCost ? cost(t.cost) : tokens(tokenTotal(t.tokens)));
  const active = totals.filter((t) => t.requests > 0).length;
  let busiest = -1;
  totals.forEach((t, i) => {
    if (t.requests > 0 && (busiest < 0 || values[i]! > values[busiest]!)) busiest = i;
  });
  const pointedIndex = hovered !== null ? days.findIndex((d) => d.start === hovered) : -1;
  const shownIndex = pointedIndex >= 0 ? pointedIndex : busiest;
  const streakDays = streak(snapshot);
  const todayStart = days[days.length - 1]?.start;
  return (
    <Card>
      <CardHeader title={A.activityCard} icon={Grid3x3}>
        {streakDays > 1 ? (
          <span className="ag-streak" title={A.streak} aria-label={`${A.streak}: ${streakDays}`}>
            <Flame size={10} fill="currentColor" strokeWidth={2} aria-hidden />
            {streakDays}
          </span>
        ) : null}
      </CardHeader>
      <div className="ag-activity">
        <div
          className="ag-heatmap"
          style={{ width: mapWidth, height: MAP_HEIGHT, gap: CELL_GAP }}
          onPointerLeave={() => setHovered(null)}
          aria-hidden
        >
          {columns.map((week, w) => (
            <div key={w} className="ag-heat-week" style={{ gap: CELL_GAP }}>
              {week.map((day, row) =>
                day ? (
                  <span
                    key={row}
                    className="ag-heat-cell"
                    onPointerEnter={() => setHovered(day.start)}
                    style={{
                      width: cell,
                      height: cell,
                      borderRadius: Math.min(2.5, cell / 3),
                      background: `rgba(255,255,255,${heatOpacity(values[day.index]!, levels)})`,
                      boxShadow:
                        hovered === day.start
                          ? "inset 0 0 0 1px rgba(255,255,255,0.9)"
                          : day.start === todayStart
                            ? "inset 0 0 0 1px rgba(255,255,255,0.45)"
                            : undefined,
                    }}
                  />
                ) : (
                  <span key={row} style={{ width: cell, height: cell }} />
                ),
              )}
            </div>
          ))}
        </div>
        <div className="ag-figures">
          <span className="ag-figures-total">{value(sumTotals(totals))}</span>
          <span className="ag-caption">{A.weeks(Math.floor(days.length / 7))}</span>
          <span className="ag-spacer-v" />
          <Figure label={A.activeDays} value={String(active)} />
          {shownIndex >= 0 ? (
            <Figure
              label={
                pointedIndex >= 0
                  ? new Date(days[shownIndex]!.start).toLocaleDateString("en-US", { weekday: "long" })
                  : A.busiestDay
              }
              value={`${shortDate(days[shownIndex]!.start)} · ${value(totals[shownIndex]!)}`}
            />
          ) : null}
        </div>
      </div>
    </Card>
  );
}

function Figure({ label, value }: { label: string; value: string }) {
  return (
    <span className="ag-figure">
      <span className="ag-figure-label">{label}</span>
      <span className="ag-figure-value">{value}</span>
    </span>
  );
}
