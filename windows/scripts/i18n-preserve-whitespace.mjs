// SPDX-License-Identifier: GPL-3.0-or-later
// Preserve intentional inline spacing when updating Windows-only translations.
import { readFileSync, writeFileSync, readdirSync } from "node:fs";
const directory = "src/i18n/overrides/";
const english = JSON.parse(readFileSync(directory + "en-US.json", "utf8"));
for (const filename of readdirSync(directory).filter((f) => f.endsWith(".json"))) {
  const path = directory + filename,
    table = JSON.parse(readFileSync(path, "utf8"));
  for (const [key, value] of Object.entries(english)) {
    const leading = value.match(/^\s*/)[0],
      trailing = value.match(/\s*$/)[0];
    if (leading || trailing) table[key] = leading + table[key].trim() + trailing;
  }
  writeFileSync(path, JSON.stringify(table, null, 2) + "\n");
}
