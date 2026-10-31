// SPDX-License-Identifier: GPL-3.0-or-later
import { beforeEach, describe, expect, it } from "vitest";
import {
  LANGUAGES,
  applyLanguage,
  dateTimeFormat,
  formatSwift,
  formattingLocale,
  getLanguage,
  pluralForm,
  setLocaleTable,
  setRegionalSettings,
  systemLanguage,
  t,
  tf,
} from "./index";
import { loadPrefs, onPrefs, resolve, setPrefs } from "../shared/prefs";
import english from "./generated/en-US.json";

beforeEach(async () => {
  setRegionalSettings({ preferred: ["en-US"], region: "US", hourCycle: "h12", firstWeekday: 0 });
  await applyLanguage("en-US");
});

it("ships exactly the Mac's 15 native-named languages", () => {
  expect(LANGUAGES).toHaveLength(15);
  expect(new Set(LANGUAGES.map((l) => l.value)).size).toBe(15);
  expect(LANGUAGES.find((l) => l.value === "uk")?.label).toBe("Українська");
});
it.each([
  ["en-GB", "en-US"],
  ["pt-PT", "pt-BR"],
  ["zh-Hant-HK", "zh-HK"],
  ["zh-HK", "zh-HK"],
  ["zh-Hant-TW", "zh-TW"],
  ["zh-Hant", "zh-TW"],
  ["zh-CN", "zh-Hans"],
  ["ja-JP", "ja"],
  ["pl-PL", "en-US"],
  ["DE-de", "de"],
])("maps system language %s to %s", (input, expected) => expect(systemLanguage(input)).toBe(expected));

it.each(LANGUAGES.map((l) => [l.value] as const))(
  "loads %s through setLocaleTable and the existing t API",
  async (lang) => {
    await applyLanguage(lang);
    const source = await import(`./generated/${lang}.json`);
    expect(getLanguage()).toBe(lang);
    expect(t("s.languageLabel")).toBe(source.default.s.languageLabel);
    expect(t("module.music")).not.toBe("module.music");
    expect(tf("notchActivities.sessionProgress", 2, 5)).not.toMatch(/%(?:@|\d+\$|d)/);
  },
);

it("falls back to English for a missing value, retaining legacy island keys", () => {
  setLocaleTable({ "module.music": "Custom music" });
  expect(t("module.music")).toBe("Custom music");
  expect(t("s.languageLabel")).toBe(english.s.languageLabel);
  setLocaleTable({});
  expect(t("module.music")).toBe("Now Playing");
});
it("the last language request wins even when previous imports finish later", async () => {
  await Promise.all([applyLanguage("de"), applyLanguage("ja")]);
  expect(getLanguage()).toBe("ja");
});
it("combines the UI language with Windows region and explicit hour cycle", async () => {
  setRegionalSettings({ preferred: ["de-DE"], region: "PL", hourCycle: "h12", firstWeekday: 1 });
  await applyLanguage("auto");
  expect(formattingLocale()).toBe("de-PL");
  expect(dateTimeFormat({ hour: "numeric" }).resolvedOptions().hourCycle).toBe("h12");
  expect(dateTimeFormat({ hour: "numeric" }, "en-GB").resolvedOptions().hourCycle).toBe("h23");
});
it("persists all supported choices through the existing preference store and broadcasts changes", async () => {
  const changes: string[] = [];
  const off = await onPrefs((p) => changes.push(p.island.language));
  for (const { value } of LANGUAGES) {
    await setPrefs({ island: { language: value } });
    expect((await loadPrefs()).island.language).toBe(value);
  }
  expect(changes).toEqual(LANGUAGES.map((l) => l.value));
  off();
  expect(resolve({ island: { language: "unsupported" as never } }).island.language).toBe("auto");
  await setPrefs({ island: { language: "auto" } });
});

describe("Swift formatting and count agreement", () => {
  it("formats reordered, mixed, integer, float and escaped percent tokens", () => {
    expect(formatSwift("%2$@ %1$d %%", 21, "files")).toBe("files 21 %");
    expect(formatSwift("%2$@ %@ %d", 3, 8)).toBe("8 3 8");
    expect(formatSwift("%ld %lld %u %.2f %.0f", 4, 5, 6, 1.234, 2.9)).toBe("4 5 6 1.23 3");
    expect(formatSwift("Filename %year %mo %s")).toBe("Filename %year %mo %s");
  });
  it("keeps the Mac's zero and three-form rules", () => {
    for (const language of ["fr", "pt-BR"] as const) expect(pluralForm(language, 0)).toBe("many");
    for (const language of ["ru", "uk"] as const) {
      expect([1, 2, 5, 11, 14, 21, 22, 25].map((n) => pluralForm(language, n))).toEqual([
        "one",
        "few",
        "many",
        "many",
        "many",
        "one",
        "few",
        "many",
      ]);
    }
    expect([1, 2, 4, 5, 21, 22].map((n) => pluralForm("sk", n))).toEqual(["one", "few", "few", "many", "many", "many"]);
  });
});
