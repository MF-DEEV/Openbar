// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// A Clawd's costume and the lot that draws it (ClawdCostume.swift; spec
// 02-sessions §5.1-5.3). The lot must give the very same costume as the Mac
// for the same session id, so the 64-bit math runs on BigInt and the part
// lists keep the Mac's order: a part added in the middle would redress every
// Clawd nobody dressed by hand.

export const SHELLS = ["clay", "coral", "amber", "moss", "mint", "teal", "sky", "iris", "orchid", "rose"] as const;
export const HATS = ["none", "beanie", "cap", "beret", "topHat", "crown", "party", "propeller", "chef"] as const;
export const EYEWEAR = ["none", "glasses", "shades", "round", "monocle", "anaglyph"] as const;
export const EXTRAS = ["none", "bow", "scarf", "bowTie", "flower", "headphones", "coffee"] as const;

export type ClawdShell = (typeof SHELLS)[number];
export type ClawdHat = (typeof HATS)[number];
export type ClawdEyewear = (typeof EYEWEAR)[number];
export type ClawdExtra = (typeof EXTRAS)[number];

export interface ClawdCostume {
  shell: ClawdShell;
  hat: ClawdHat;
  eyewear: ClawdEyewear;
  extra: ClawdExtra;
}

export type Rgb = readonly [number, number, number];

/** sRGB out of 255. Clay is Claude Code's own `clawd_body`. */
export const SHELL_RGB: Readonly<Record<ClawdShell, Rgb>> = {
  clay: [215, 119, 87],
  coral: [226, 86, 72],
  amber: [230, 166, 58],
  moss: [112, 168, 70],
  mint: [72, 184, 146],
  teal: [40, 160, 170],
  sky: [68, 146, 224],
  iris: [116, 112, 226],
  orchid: [178, 98, 214],
  rose: [230, 112, 156],
};

/** Clawd as Claude Code draws it. */
export const CLASSIC: Readonly<ClawdCostume> = Object.freeze({
  shell: "clay",
  hat: "none",
  eyewear: "none",
  extra: "none",
});

/**
 * Who wears the outfit: Clawd for Claude Code, a little terminal robot for
 * Codex. The costume catalog and codes are shared; the figure and the shell
 * that comes up most often differ.
 */
export type Character = "clawd" | "codex";

/** The shell each character's lots favour: Claude's clay, Codex's mint. */
export const FAVOURED_SHELL: Readonly<Record<Character, ClawdShell>> = { clawd: "clay", codex: "mint" };

/** How often a lot draws something rather than nothing. */
export const HAT_CHANCE = 0.7;
export const EYEWEAR_CHANCE = 0.4;
export const EXTRA_CHANCE = 0.45;
/** Claude's own color comes up this many times as often as any other. */
export const CLAY_WEIGHT = 3;

const MASK = 0xffff_ffff_ffff_ffffn;

/** FNV-1a 64 over the key's UTF-8 bytes: the same number on every launch and platform. */
export function seed(key: string): bigint {
  let hash = 0xcbf2_9ce4_8422_2325n;
  for (const byte of new TextEncoder().encode(key)) {
    hash ^= BigInt(byte);
    hash = (hash * 0x0000_0100_0000_01b3n) & MASK;
  }
  return hash;
}

/** A source of 64-bit numbers, as Swift's RandomNumberGenerator. */
export interface RandomSource {
  next(): bigint;
}

/** SplitMix64: the same numbers for the same seed everywhere. */
export class ClawdRandom implements RandomSource {
  private state: bigint;

  constructor(seedValue: bigint) {
    this.state = seedValue & MASK;
  }

  next(): bigint {
    this.state = (this.state + 0x9e37_79b9_7f4a_7c15n) & MASK;
    let value = this.state;
    value = ((value ^ (value >> 30n)) * 0xbf58_476d_1ce4_e5b9n) & MASK;
    value = ((value ^ (value >> 27n)) * 0x94d0_49bb_1331_11ebn) & MASK;
    return value ^ (value >> 31n);
  }
}

/** The system's random numbers, for "Surprise me", which need not match anything. */
export const systemRandom: RandomSource = {
  next() {
    const words = new Uint32Array(2);
    crypto.getRandomValues(words);
    return (BigInt(words[0]) << 32n) | BigInt(words[1]);
  },
};

/**
 * Nothing (the first part) with the chance left over, else one of the rest.
 * Always takes two numbers, the roll first, as the Mac does.
 */
function pick<T>(parts: readonly T[], chance: number, random: RandomSource): T {
  const roll = Number(random.next() >> 11n) / 2 ** 53;
  const index = Number(random.next() % BigInt(Math.max(1, parts.length - 1)));
  return roll < chance && parts.length > 1 ? parts[1 + index] : parts[0];
}

/** One part after another from the same lot: seven numbers in all. */
export function drawn(random: RandomSource, character: Character = "clawd"): ClawdCostume {
  const favoured = FAVOURED_SHELL[character];
  const weights = SHELLS.map((shell) => (shell === favoured ? CLAY_WEIGHT : 1));
  const total = weights.reduce((a, b) => a + b, 0);
  let ticket = Number(random.next() % BigInt(total));
  let shell: ClawdShell = favoured;
  for (let i = 0; i < SHELLS.length; i++) {
    if (ticket < weights[i]) {
      shell = SHELLS[i];
      break;
    }
    ticket -= weights[i];
  }
  return {
    shell,
    hat: pick(HATS, HAT_CHANCE, random),
    eyewear: pick(EYEWEAR, EYEWEAR_CHANCE, random),
    extra: pick(EXTRAS, EXTRA_CHANCE, random),
  };
}

/** The shell Claude Code's agent `color:` stands for, whatever its case and spacing. */
export function shellForAgentColor(color: string | null | undefined): ClawdShell | null {
  switch ((color ?? "").trim().toLowerCase()) {
    case "red":
      return "coral";
    case "orange":
      return "clay";
    case "yellow":
      return "amber";
    case "green":
      return "moss";
    case "cyan":
      return "teal";
    case "blue":
      return "sky";
    case "purple":
      return "iris";
    case "pink":
      return "rose";
    default:
      return null;
  }
}

/**
 * The costume `key` draws, the same each time. The key is the session id, or
 * `"<sessionId>/<agentId>"` for a subagent, whose definition's color, if
 * Claude Code knows it, sets the shell.
 */
export function seeded(key: string, agentColor?: string | null, character: Character = "clawd"): ClawdCostume {
  const costume = drawn(new ClawdRandom(seed(key)), character);
  const shell = shellForAgentColor(agentColor);
  return shell ? { ...costume, shell } : costume;
}

/** The key a subagent's costume and rhythm come from. */
export function subagentKey(sessionId: string, agentId: string): string {
  return `${sessionId}/${agentId}`;
}

export function sameCostume(a: ClawdCostume | null | undefined, b: ClawdCostume | null | undefined): boolean {
  if (!a || !b) return a === b;
  return a.shell === b.shell && a.hat === b.hat && a.eyewear === b.eyewear && a.extra === b.extra;
}

/** A costume by lot other than `current`, for "Surprise me". */
export function randomCostume(current: ClawdCostume | null | undefined, random: RandomSource = systemRandom) {
  let costume = drawn(random);
  // Ten draws alike in a row are as good as impossible; give up then.
  for (let i = 0; i < 10 && sameCostume(costume, current); i++) costume = drawn(random);
  return costume;
}

// MARK: Keeping

const VERSION = "v1:";

/** The costume as the Mac keeps it: `v1:<shell>.<hat>.<eyewear>.<extra>`. */
export function costumeCode(costume: ClawdCostume): string {
  return VERSION + [costume.shell, costume.hat, costume.eyewear, costume.extra].join(".");
}

function member<T extends string>(list: readonly T[], value: string): T | undefined {
  return (list as readonly string[]).includes(value) ? (value as T) : undefined;
}

/**
 * Reads a kept costume. A part this version does not know comes from
 * `fallback`; anything else unreadable is null.
 */
export function parseCostume(code: string, fallback: ClawdCostume): ClawdCostume | null {
  // Swift counts characters (grapheme clusters); for anything a costume can
  // hold that is the same as code points, and the limit is only a guard.
  if (!code.startsWith(VERSION) || [...code].length > 80) return null;
  const parts = code.slice(VERSION.length).split(".");
  if (parts.length !== 4) return null;
  return {
    shell: member(SHELLS, parts[0]) ?? fallback.shell,
    hat: member(HATS, parts[1]) ?? fallback.hat,
    eyewear: member(EYEWEAR, parts[2]) ?? fallback.eyewear,
    extra: member(EXTRAS, parts[3]) ?? fallback.extra,
  };
}

/** What a session wears: what the person chose for it, or its own lot. */
export function resolvedCostume(
  key: string,
  kept?: string | null,
  agentColor?: string | null,
  character: Character = "clawd",
): ClawdCostume {
  const own = seeded(key, agentColor, character);
  return (kept != null ? parseCostume(kept, own) : null) ?? own;
}

/**
 * What to keep for a session dressed in `costume`: nothing when it is the
 * session's own lot, so the choice follows the lot if it ever changes.
 */
export function keptCode(
  costume: ClawdCostume | null | undefined,
  key: string,
  character: Character = "clawd",
): string | null {
  if (!costume || sameCostume(costume, seeded(key, null, character))) return null;
  return costumeCode(costume);
}

/** How many sessions' costumes are kept (`namesKept` on the Mac). */
export const COSTUMES_KEPT = 100;

/**
 * The kept map with one value changed or, with null, removed. Past `limit`,
 * entries of sessions no longer running go first, in sorted-key order.
 */
export function withKept(
  values: Readonly<Record<string, string>>,
  id: string,
  value: string | null,
  live: ReadonlySet<string>,
  limit = COSTUMES_KEPT,
): Record<string, string> {
  const result: Record<string, string> = { ...values };
  if (value == null) delete result[id];
  else result[id] = value;
  let count = Object.keys(result).length;
  if (count > limit) {
    for (const key of Object.keys(result).sort()) {
      if (count <= limit) break;
      if (live.has(key) || key === id) continue;
      delete result[key];
      count--;
    }
  }
  return result;
}

/** Where costumes chosen by hand are kept, by session id. */
export interface CostumeStore {
  /** Every kept code by session id. */
  all(): Record<string, string>;
  get(sessionId: string): string | null;
  /** Keeps `code` (null removes it). `live` are the running sessions, kept first past the limit. */
  set(sessionId: string, code: string | null, live?: ReadonlySet<string>): void;
}

/** The Mac's key, `notchSessionsCostumes`: a `{sessionId: code}` object. */
export const COSTUMES_STORAGE_KEY = "notchSessionsCostumes";

/**
 * A store in localStorage. Every access is guarded: storage can be missing
 * or throw (private mode, blocked site data), and then nothing is kept and
 * every Clawd wears its lot.
 */
export function localCostumeStore(key = COSTUMES_STORAGE_KEY): CostumeStore {
  const read = (): Record<string, string> => {
    try {
      const raw = globalThis.localStorage?.getItem(key);
      const parsed: unknown = raw ? JSON.parse(raw) : {};
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
      return Object.fromEntries(
        Object.entries(parsed as Record<string, unknown>).filter(
          (entry): entry is [string, string] => typeof entry[1] === "string",
        ),
      );
    } catch {
      return {};
    }
  };
  return {
    all: read,
    get: (sessionId) => read()[sessionId] ?? null,
    set(sessionId, code, live = new Set()) {
      try {
        globalThis.localStorage?.setItem(key, JSON.stringify(withKept(read(), sessionId, code, live)));
      } catch {
        // Not kept; the Clawd still wears it until the island reloads.
      }
    },
  };
}
