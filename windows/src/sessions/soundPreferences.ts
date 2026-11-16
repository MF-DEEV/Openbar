// SPDX-License-Identifier: GPL-3.0-or-later
// Session sounds (#232). Empty remains the #167 Openbar chime; "none" is silence.
export const SOUND_EVENTS = ["permission", "question", "finished", "error", "limit", "started"] as const;
export type SoundEventKind = (typeof SOUND_EVENTS)[number];
export const SOUND_KEYS = {
  permission: "soundPermission",
  question: "soundQuestion",
  finished: "soundFinished",
  error: "soundError",
  limit: "soundLimit",
  started: "soundStarted",
} as const;
export interface SessionSoundPrefs {
  soundPermission: string;
  soundQuestion: string;
  soundFinished: string;
  soundError: string;
  soundLimit: string;
  soundStarted: string;
  soundVolume: number;
  /** Epoch seconds. Expiration is checked against the wall clock at playback. */
  soundMutedUntil: number;
}
export const DEFAULT_SOUND_PREFS: SessionSoundPrefs = {
  soundPermission: "",
  soundQuestion: "",
  soundFinished: "none",
  soundError: "none",
  soundLimit: "none",
  soundStarted: "none",
  soundVolume: 100,
  soundMutedUntil: 0,
};

/** Read-through migration, without rewriting a person's settings file. New
 * event choices always win, including None. Newly introduced events are opt-in. */
export function resolveSessionSounds(value: unknown): SessionSoundPrefs {
  const raw = value && typeof value === "object" ? (value as Record<string, unknown>) : {};
  const valid = (v: unknown): v is string => typeof v === "string" && v.length <= 1024 && !/[\x00-\x1f]/.test(v);
  const waiting = raw.waitingSound === false ? "none" : valid(raw.waitingSoundChoice) ? raw.waitingSoundChoice : "";
  const result = { ...DEFAULT_SOUND_PREFS, soundPermission: waiting, soundQuestion: waiting };
  for (const kind of SOUND_EVENTS) {
    const key = SOUND_KEYS[kind];
    if (valid(raw[key])) result[key] = raw[key];
  }
  if (typeof raw.soundVolume === "number" && Number.isFinite(raw.soundVolume))
    result.soundVolume = Math.max(0, Math.min(100, raw.soundVolume));
  if (typeof raw.soundMutedUntil === "number" && Number.isFinite(raw.soundMutedUntil))
    result.soundMutedUntil = Math.max(0, raw.soundMutedUntil);
  return result;
}

export const soundsMuted = (prefs: SessionSoundPrefs, now = Date.now() / 1000) => prefs.soundMutedUntil > now;

/** Tomorrow means the next local midnight, including DST transitions. */
export function muteUntil(period: "hour" | "tomorrow", now = new Date()): number {
  if (period === "hour") return now.getTime() / 1000 + 3600;
  return new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1).getTime() / 1000;
}
