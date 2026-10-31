// SPDX-License-Identifier: GPL-3.0-or-later
import { useSyncExternalStore } from "react";
import englishSource from "./generated/en-US.json";
import englishOverrides from "./overrides/en-US.json";
import aliases from "./aliases.json";
import type { CatalogKey } from "./keys";
import { formatSwift } from "./format";
import { type Language, type LanguagePreference, isLanguage, systemLanguage } from "./languages";
export { LANGUAGES, pluralForm, systemLanguage } from "./languages";
export type { Language, LanguagePreference } from "./languages";
export { formatSwift } from "./format";
export type StringKey = CatalogKey | keyof typeof aliases;
function flatten(source: object, prefix = "", out: Record<string, string> = {}): Record<string, string> {
  for (const [key, value] of Object.entries(source)) {
    const name = prefix ? `${prefix}.${key}` : key;
    if (typeof value === "string") out[name] = value;
    else if (value && typeof value === "object") flatten(value, name, out);
  }
  return out;
}
const fallback: Record<string, string> = { ...flatten(englishSource), ...englishOverrides };
const messageKeys = new Map(
  Object.entries(fallback)
    .filter(([, value]) => value.length >= 8)
    .map(([key, value]) => [value, key]),
);
const loaders = import.meta.glob<{ default: object }>("./generated/*.json");
const overrideLoaders = import.meta.glob<{ default: Record<string, string> }>("./overrides/*.json");
let table: Record<string, string> = {};
let language: Language = "en-US";
let revision = 0;
let request = 0;
const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};
const snapshot = () => revision;
function changed() {
  revision++;
  listeners.forEach((listener) => listener());
}
export interface RegionalSettings {
  preferred: string[];
  region: string;
  hourCycle: "h12" | "h23";
  firstWeekday: number;
}
const browserLocale = typeof navigator === "undefined" ? "en-US" : navigator.language;
const browserFormat = new Intl.DateTimeFormat(undefined, { hour: "numeric" }).resolvedOptions();
let regional: RegionalSettings = {
  preferred: [browserLocale],
  region: new Intl.Locale(browserLocale).maximize().region ?? "US",
  hourCycle: browserFormat.hourCycle === "h12" || browserFormat.hourCycle === "h11" ? "h12" : "h23",
  firstWeekday: 1,
};
export function setRegionalSettings(settings: RegionalSettings) {
  regional = settings;
  changed();
}
export const getRegionalSettings = () => regional;
export const getLanguage = () => language;
export function formattingLocale(): string {
  const script = language.startsWith("zh") ? new Intl.Locale(language).maximize().script : undefined;
  return new Intl.Locale(language, { region: regional.region, ...(script ? { script } : {}) }).baseName;
}
export function dateTimeFormat(options: Intl.DateTimeFormatOptions, explicitLocale?: string) {
  return new Intl.DateTimeFormat(explicitLocale ?? formattingLocale(), {
    ...(!explicitLocale ? { hourCycle: regional.hourCycle } : {}),
    ...options,
  });
}
export function formatUnit(
  count: number,
  unit: "second" | "minute" | "hour" | "day",
  style: "long" | "short" = "long",
): string {
  const manyZero = count === 0 && (language === "fr" || language === "pt-BR");
  const formatter = new Intl.NumberFormat(formattingLocale(), { style: "unit", unit, unitDisplay: style });
  return manyZero
    ? formatter
        .formatToParts(2)
        .map((part) => (part.type === "integer" ? "0" : part.value))
        .join("")
    : formatter.format(count);
}
/** Memoized/portalled components subscribe too; switching never remounts them. */
export function useLocale(): Language {
  useSyncExternalStore(subscribe, snapshot, snapshot);
  return language;
}
/** Existing API, now reactive and still accepting legacy island keys. */
export function setLocaleTable(next: Partial<Record<StringKey, string>>) {
  table = Object.fromEntries(
    Object.entries(next)
      .filter((entry): entry is [string, string] => typeof entry[1] === "string")
      .map(([key, value]) => [(aliases as Record<string, string>)[key] ?? key, value]),
  );
  for (const [key, value] of Object.entries(table)) if (value.length >= 8) messageKeys.set(value, key);
  changed();
}
/** Only for app-generated status/error fields. Never apply to user content. */
export function localizeMessage<T extends string | null | undefined>(message: T): T {
  const key = message && messageKeys.get(message);
  return (key ? t(key as StringKey) : message) as T;
}
export async function applyLanguage(preference: LanguagePreference | string): Promise<Language> {
  const id = ++request;
  const next = isLanguage(preference) ? preference : systemLanguage(regional.preferred[0] ?? "en");
  const source = next === "en-US" ? englishSource : (await loaders[`./generated/${next}.json`]?.())?.default;
  const overrides =
    next === "en-US" ? englishOverrides : (await overrideLoaders[`./overrides/${next}.json`]?.())?.default;
  if (id !== request) return language;
  language = next;
  if (typeof document !== "undefined") document.documentElement.lang = next;
  setLocaleTable({ ...flatten(source ?? englishSource), ...overrides });
  return next;
}
export function t(key: StringKey): string;
export function t<N extends keyof typeof englishSource>(
  namespace: N,
  key: keyof (typeof englishSource)[N] & string,
): string;
export function t(key: string, field?: string): string {
  const path = field ? `${key}.${field}` : ((aliases as Record<string, string>)[key] ?? key);
  return table[path] ?? fallback[path] ?? path;
}
export function tf(key: StringKey, ...args: (string | number)[]): string;
export function tf<N extends keyof typeof englishSource>(
  namespace: N,
  field: keyof (typeof englishSource)[N] & string,
  ...args: (string | number)[]
): string;
export function tf(key: string, ...args: (string | number)[]): string {
  if (Object.hasOwn(englishSource, key) && typeof args[0] === "string")
    return formatSwift(t(`${key}.${args[0]}` as StringKey), ...args.slice(1));
  return formatSwift(t(key as StringKey), ...args);
}
