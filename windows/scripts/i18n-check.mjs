// SPDX-License-Identifier: GPL-3.0-or-later
import assert from "node:assert/strict";
import { readFileSync, writeFileSync } from "node:fs";
import ts from "typescript";
import { flatten, files, translatable } from "./i18n-inventory.mjs";
const languages = JSON.parse(readFileSync("src/i18n/extraction-report.json", "utf8")).languages;
const read = (path) => JSON.parse(readFileSync(path, "utf8"));
const english = flatten(read("src/i18n/generated/en-US.json"));
const windows = read("src/i18n/overrides/en-US.json");
const englishKeys = Object.keys(english).sort(),
  overrideKeys = Object.keys(windows).sort();
function signature(text) {
  let next = 1;
  const out = [];
  for (const match of text.matchAll(/%(?:(\d+)\$)?(@|l{0,2}[diu]|(?:\.\d+)?f|%)/g)) {
    if (match[2] === "%") continue;
    out.push(
      `${match[1] ? Number(match[1]) : next++}:${match[2] === "@" ? "string" : match[2].endsWith("f") ? "float" : "integer"}`,
    );
  }
  return out.sort().join(",");
}
const errors = [];
for (const language of languages) {
  const table = flatten(read(`src/i18n/generated/${language}.json`));
  const overrides = read(`src/i18n/overrides/${language}.json`);
  if (JSON.stringify(Object.keys(table).sort()) !== JSON.stringify(englishKeys))
    errors.push(`${language}: generated key set differs`);
  if (JSON.stringify(Object.keys(overrides).sort()) !== JSON.stringify(overrideKeys))
    errors.push(`${language}: Windows key set differs`);
  for (const [key, text] of Object.entries({ ...table, ...overrides })) {
    const base = windows[key] ?? english[key];
    if (typeof text !== "string" || (!text && base)) errors.push(`${language}:${key}: empty translation`);
    if (/__[PL]\d+__/.test(text)) errors.push(`${language}:${key}: unresolved generation token`);
    if (base && signature(text) !== signature(base))
      errors.push(`${language}:${key}: printf mismatch ${signature(base)} -> ${signature(text)}`);
  }
}
const residual = [];
let checked = 0;
for (const file of files("src")) {
  const source = ts.createSourceFile(
    file,
    readFileSync(file, "utf8"),
    ts.ScriptTarget.Latest,
    true,
    file.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS,
  );
  const visit = (node) => {
    if (
      ts.isCallExpression(node) &&
      ["t", "tf"].includes(node.expression.getText(source)) &&
      node.arguments[0] &&
      ts.isStringLiteral(node.arguments[0])
    ) {
      const key = node.arguments[0].text;
      if (!english[key] && !windows[key] && !read("src/i18n/aliases.json")[key] && !key.startsWith("module."))
        errors.push(`${file}: unknown key ${key}`);
      checked++;
    }
    if (translatable(node, source, file)) {
      const text = node.getText(source);
      // Typed key fragments in the custom Mac plural selector are not UI text.
      if (!file.replaceAll("\\", "/").endsWith("files/strings.ts") || !/Singular|Few|Plural|s\.shelfTooltip/.test(text))
        residual.push(`${file}:${source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1}: ${text}`);
    }
    ts.forEachChild(node, visit);
  };
  visit(source);
}
const report = {
  languages: languages.length,
  generatedKeys: englishKeys.length,
  windowsOverrides: overrideKeys.length,
  translationCalls: checked,
  errors,
  residual,
};
writeFileSync("build/i18n-check.json", JSON.stringify(report, null, 2) + "\n");
console.log(JSON.stringify(report, null, 2));
assert.equal(errors.length, 0, "Translation completeness/placeholder validation failed");
assert.equal(residual.length, 0, "User-facing text remains outside the catalogs");
