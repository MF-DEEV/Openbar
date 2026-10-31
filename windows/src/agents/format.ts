// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Numbers the AI page reads (AgentFormat in NotchAgentSupport.swift:336-409;
// spec 03 §10.6): costs in US dollars, the currency both providers list
// their prices in, and model names built from the id (AgentPricing.swift
// :269-299).

import { formattingLocale, dateTimeFormat, tf } from "../i18n";

import type { Provider, Tokens } from "./types";

/** One decimal below ten, none above, always rounded down: "4.2", "48". */
function scaled(value: number): string {
  const digits = value < 10 ? 1 : 0;
  const factor = digits === 1 ? 10 : 1;
  const rounded = Math.floor(value * factor) / factor;
  return rounded.toLocaleString(formattingLocale(), { maximumFractionDigits: digits, minimumFractionDigits: 0 });
}

/** "$4.56", "$123", "$12K". */
export function cost(value: number): string {
  if (!Number.isFinite(value)) return "$0";
  const magnitude = Math.abs(value);
  if (magnitude >= 10_000) return `$${scaled(magnitude / 1000)}K`;
  const digits = magnitude >= 100 ? 0 : 2;
  const text = magnitude.toLocaleString(formattingLocale(), {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  });
  return `${value < 0 ? "-" : ""}$${text}`;
}

/** "842", "4.2K", "48K", "1.2M", "3B": truncated, never rounded up. */
export function tokens(value: number): string {
  const magnitude = Math.max(0, Number.isFinite(value) ? value : 0);
  if (magnitude < 1000) return Math.floor(magnitude).toLocaleString(formattingLocale());
  if (magnitude < 1_000_000) return `${scaled(magnitude / 1000)}K`;
  if (magnitude < 1_000_000_000) return `${scaled(magnitude / 1_000_000)}M`;
  return `${scaled(magnitude / 1_000_000_000)}B`;
}

/** A 0…1 fraction as a whole percent. */
export function percent(fraction: number): string {
  const value = Number.isFinite(fraction) ? Math.min(1, Math.max(0, fraction)) : 0;
  return `${Math.round(value * 100)}%`;
}

/**
 * "4m 12s", "1h 05m", "2d 3h": abbreviated, the two largest units (or
 * `units`), never rounded up.
 */
export function duration(seconds: number, units: 1 | 2 = 2): string {
  const total = Math.floor(Math.max(0, Number.isFinite(seconds) ? seconds : 0));
  const pad = (n: number) => String(n).padStart(2, "0");
  if (total >= 86_400) {
    const d = Math.floor(total / 86_400);
    const h = Math.floor((total % 86_400) / 3600);
    return units === 1 || h === 0 ? `${d}d` : tf("windows.agents.valueDValueH", d, h);
  }
  if (total >= 3600) {
    const h = Math.floor(total / 3600);
    const m = Math.floor((total % 3600) / 60);
    return units === 1 ? `${h}h` : tf("windows.agents.valueHValueM", h, pad(m));
  }
  const m = Math.floor(total / 60);
  const s = total % 60;
  if (m === 0) return `${s}s`;
  return units === 1 ? `${m}m` : tf("windows.agents.valueMValueS", m, pad(s));
}

/** A running stopwatch: "0:42", "12:05", "1:02:05". */
export function clock(seconds: number): string {
  const total = Math.floor(Math.max(0, Number.isFinite(seconds) ? seconds : 0));
  const pad = (n: number) => String(n).padStart(2, "0");
  if (total >= 3600) return `${Math.floor(total / 3600)}:${pad(Math.floor(total / 60) % 60)}:${pad(total % 60)}`;
  return `${Math.floor(total / 60)}:${pad(total % 60)}`;
}

/** How long until a window renews: never seconds, at least a minute (NotchAgentsView.countdown). */
export function countdown(to: number, now: number): string {
  const seconds = (to - now) / 1000;
  return duration(Math.max(60, seconds), seconds >= 3600 ? 2 : 1);
}

/** "5 min. ago", "yesterday", "in 2 hr.": the Mac's named relative date, abbreviated unless `long`. */
export function relative(date: number, now: number, style: "short" | "long" = "short"): string {
  const relativeFormat = new Intl.RelativeTimeFormat(formattingLocale(), { numeric: "auto", style });
  const seconds = (date - now) / 1000;
  const magnitude = Math.abs(seconds);
  if (magnitude < 60) return relativeFormat.format(0, "second");
  if (magnitude < 3600) return relativeFormat.format(Math.round(seconds / 60), "minute");
  if (magnitude < 86_400) return relativeFormat.format(Math.round(seconds / 3600), "hour");
  if (magnitude < 7 * 86_400) return relativeFormat.format(Math.round(seconds / 86_400), "day");
  return relativeFormat.format(Math.round(seconds / (7 * 86_400)), "week");
}

/** A day kept in UTC, like the price list's, read as that same day anywhere. */
export function utcDay(day: string): string {
  const date = new Date(`${day}T00:00:00Z`);
  if (Number.isNaN(date.getTime())) return day;
  return dateTimeFormat({ timeZone: "UTC", month: "short", day: "numeric", year: "numeric" }).format(date);
}

/** "Oct 9" */
export const shortDate = (ms: number) => dateTimeFormat({ month: "short", day: "numeric" }).format(new Date(ms));
/** "Oct 9, 2:30 PM" */
export const dateTime = (ms: number) =>
  dateTimeFormat({ month: "short", day: "numeric", hour: "numeric", minute: "2-digit" }).format(new Date(ms));

// MARK: Tokens

export const tokenTotal = (t: Tokens) => t.input + t.cacheWrite + t.cacheRead + t.output;
/** Of everything the model read, the share that came from the cache. */
export function cacheHitRate(t: Tokens): number | null {
  const prompt = t.input + t.cacheWrite + t.cacheRead;
  return prompt > 0 ? t.cacheRead / prompt : null;
}

// MARK: Providers

export const PROVIDER_NAMES: Record<Provider, string> = {
  claude: "Claude",
  codex: "Codex",
  opencode: "OpenCode",
  copilot: "GitHub Copilot",
};

/** Each agent keeps one color across the page, the strip and notices (§10.1). */
export const PROVIDER_TINTS: Record<Provider, string> = {
  claude: "#D97857",
  codex: "#7D99FF",
  opencode: "#0FBA82",
  copilot: "#4DC7AD",
};

export const LIMIT_ORANGE = "#FF9F0A";
export const LIMIT_RED = "#FF453A";

/** The agent's own color while there is room, orange from 80 % used, red from 95 %. */
export function limitTint(provider: Provider, usedFraction: number): string {
  if (usedFraction >= 0.95) return LIMIT_RED;
  if (usedFraction >= 0.8) return LIMIT_ORANGE;
  return PROVIDER_TINTS[provider];
}

/** Copilot never reports plan limits (AgentProvider.reportsLimits). */
export const reportsLimits = (p: Provider) => p !== "copilot";

// MARK: Model names

/** Provider prefixes, snapshot dates and context tags vary; the family does not. */
export function normalizedModel(model: string): string {
  let id = model.toLowerCase().trim();
  const claude = id.indexOf("claude-");
  if (claude >= 0) id = id.slice(claude);
  if (id.startsWith("claude-")) id = id.replaceAll(".", "-");
  const slash = id.lastIndexOf("/");
  if (slash >= 0) id = id.slice(slash + 1);
  for (const marker of ["@", "["]) {
    const at = id.indexOf(marker);
    if (at >= 0) id = id.slice(0, at);
  }
  return id;
}

const isNumber = (s: string) => /^[0-9]+$/.test(s);
const isVersion = (s: string) => /^[0-9.]+$/.test(s);
const capitalized = (s: string) => (s ? s[0]!.toUpperCase() + s.slice(1) : s);

/** "claude-opus-5-5" reads "Opus 5.5" and "gpt-6-astra" reads "GPT-6 Astra". */
export function modelName(model: string): string {
  const id = normalizedModel(model);
  if (!id) return "";
  if (id.startsWith("claude-")) {
    const family = id.split(":")[0]!;
    const parts = family
      .slice(7)
      .split("-")
      .filter((p) => p.length > 0)
      .filter((p) => !(p.length >= 6 && isNumber(p)))
      .filter((p) => !(p.startsWith("v") && /^[0-9]/.test(p.slice(1))))
      .filter((p) => p !== "latest");
    const words = parts.filter((p) => !isVersion(p));
    const version = parts.filter(isVersion).join(".");
    return [capitalized(words[0] ?? ""), version, ...words.slice(1).map(capitalized)].filter(Boolean).join(" ");
  }
  if (!id.startsWith("gpt-")) return id;
  let parts = id
    .slice(4)
    .split("-")
    .filter((p) => p.length > 0)
    .filter((p) => p !== "latest")
    .filter((p) => !(p.length >= 6 && isNumber(p)));
  if (parts.length >= 4) {
    const year = Number(parts[parts.length - 3]);
    const tail = parts.slice(-2);
    if (year >= 2000 && year <= 2100 && tail.every((p) => p.length === 2 && isNumber(p))) parts = parts.slice(0, -3);
  }
  const first = parts[0];
  if (!first) return "GPT";
  const head = /^[0-9]/.test(first) ? `GPT-${first}` : tf("windows.agents.gPTValue", capitalized(first));
  return [head, ...parts.slice(1).map(capitalized)].join(" ");
}
