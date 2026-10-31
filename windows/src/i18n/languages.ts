// SPDX-License-Identifier: GPL-3.0-or-later
// AppLanguage order and native display names from Core/Localization.swift.
export const LANGUAGES = [
  { value: "en-US", label: "English (US)" },
  { value: "pt-BR", label: "Português (Brasil)" },
  { value: "tr", label: "Türkçe" },
  { value: "ru", label: "Русский" },
  { value: "es", label: "Español" },
  { value: "sk", label: "Slovenčina" },
  { value: "de", label: "Deutsch" },
  { value: "fr", label: "Français" },
  { value: "it", label: "Italiano" },
  { value: "ja", label: "日本語" },
  { value: "ko", label: "한국어" },
  { value: "uk", label: "Українська" },
  { value: "zh-Hans", label: "简体中文" },
  { value: "zh-TW", label: "繁體中文（台灣）" },
  { value: "zh-HK", label: "繁體中文（香港）" },
] as const;
export type Language = (typeof LANGUAGES)[number]["value"];
export type LanguagePreference = Language | "auto";
export const isLanguage = (value: unknown): value is Language => LANGUAGES.some((l) => l.value === value);
export function systemLanguage(preferred: string): Language {
  const p = preferred.toLowerCase().replaceAll("_", "-");
  if (p.startsWith("zh-hk") || p.startsWith("zh-hant-hk")) return "zh-HK";
  if (p.startsWith("zh-tw") || p.startsWith("zh-hant")) return "zh-TW";
  if (p.startsWith("pt")) return "pt-BR";
  if (p.startsWith("zh")) return "zh-Hans";
  return LANGUAGES.find((l) => p.startsWith(l.value.toLowerCase()))?.value ?? "en-US";
}
/** Mac CountAgreement, including French/Portuguese zero and Slovak 21. */
export function pluralForm(lang: Language, count: number): "one" | "few" | "many" {
  const n = Math.abs(count);
  if (lang === "ru" || lang === "uk") {
    if (n % 100 >= 11 && n % 100 <= 14) return "many";
    if (n % 10 === 1) return "one";
    if (n % 10 >= 2 && n % 10 <= 4) return "few";
    return "many";
  }
  if (lang === "sk") return n === 1 ? "one" : n >= 2 && n <= 4 ? "few" : "many";
  return n === 1 ? "one" : "many";
}
