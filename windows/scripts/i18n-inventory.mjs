// SPDX-License-Identifier: GPL-3.0-or-later
import ts from "typescript";
import { readFileSync, readdirSync, writeFileSync, mkdirSync } from "node:fs";
import { resolve, relative, basename } from "node:path";

export function files(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory()
      ? e.name === "i18n"
        ? []
        : files(resolve(directory, e.name))
      : /\.tsx?$/.test(e.name) && !/\.(test|d)\.tsx?$/.test(e.name) && !/demo/i.test(e.name)
        ? [resolve(directory, e.name)]
        : [],
  );
}
export function flatten(value, prefix = "", out = {}) {
  for (const [key, entry] of Object.entries(value)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (typeof entry === "string") out[path] = entry;
    else flatten(entry, path, out);
  }
  return out;
}
export const normalize = (text) =>
  text
    .normalize("NFC")
    .replace(/%(?:\d+\$)?(?:@|l{0,2}[diu]|\.?\d*f)/g, "%@")
    .replace(/[‘’]/g, "'")
    .replace(/[“”]/g, '"')
    .replace(/…/g, "...")
    .replace(/\s+/g, " ")
    .trim()
    .toLowerCase();
export const english = flatten(JSON.parse(readFileSync("src/i18n/generated/en-US.json", "utf8")));
const translated = flatten(JSON.parse(readFileSync("src/i18n/generated/ja.json", "utf8")));
const reverse = new Map();
for (const [key, text] of Object.entries(english)) {
  const norm = normalize(text);
  reverse.set(norm, [...(reverse.get(norm) ?? []), key]);
}
export function sourceKey(text, feature) {
  const keys = reverse.get(normalize(text));
  const preferred = {
    sessions: ["notchClaudeSessions"],
    agents: ["notchAgents", "codexAccounts"],
    files: ["notchFiles", "s.shelf"],
    capture: ["screenshot"],
    clipboard: ["clipboard"],
    music: ["notchMusicExtras", "notch"],
    calendar: ["notchCalendar"],
    watch: ["notchWatch"],
    settings: ["notchEditor", "notch", "settings", "s"],
    system: ["s.monitor", "connectedDevices"],
    timer: ["notchActivities", "s.timer"],
    mixer: ["mixer", "s.mixer"],
    tools: ["quickToggles", "screenshot", "s"],
    scratchpad: ["scratchpad"],
    camera: ["cameraPreview"],
    captures: ["recentCaptures"],
    notifications: ["notchNotifications"],
  }[feature] ?? ["s", "notch"];
  return keys?.toSorted((a, b) => {
    const score = (k) => {
      const i = preferred.findIndex((p) => k.startsWith(p));
      return i < 0 ? 100 : i;
    };
    return (
      Number(english[a] !== text) - Number(english[b] !== text) ||
      Number(translated[a] === english[a]) - Number(translated[b] === english[b]) ||
      score(a) - score(b) ||
      a.localeCompare(b)
    );
  })[0];
}
const attributes = new Set([
  "title",
  "label",
  "caption",
  "placeholder",
  "aria-label",
  "alt",
  "detail",
  "lead",
  "description",
  "emptyTitle",
  "emptyDetail",
  "hint",
]);
export function translatable(node, source, file) {
  if (/DevDesktop|ClawdPreview|[/\\](rig|shapes|color|tint|glyphs|search|shortcut)\.tsx?$/.test(file)) return false;
  for (let a = node.parent; a && !ts.isSourceFile(a); a = a.parent) {
    if (
      ts.isCallExpression(a) &&
      /^(t|tf)$/.test(a.expression.getText(source)) &&
      a.arguments[0] &&
      node.pos >= a.arguments[0].pos &&
      node.end <= a.arguments[0].end
    )
      return false;
    if (
      ts.isJsxAttribute(a) &&
      [
        "style",
        "className",
        "id",
        "key",
        "src",
        "href",
        "type",
        "value",
        "role",
        "aria-controls",
        "aria-activedescendant",
        "d",
        "viewBox",
        "clipPath",
        "fill",
        "stroke",
        "transform",
        "filter",
      ].includes(a.name.getText(source))
    )
      return false;
    if (
      basename(file) !== "strings.ts" &&
      ts.isPropertyAssignment(a) &&
      ["style", "className", "fontFamily", "transform", "filter", "boxShadow", "fill", "stroke", "background"].includes(
        a.name.getText(source),
      )
    )
      return false;
    if (
      ts.isFunctionLike(a) &&
      (a.name?.text === "counted" || a.parent?.name?.text === "counted") &&
      file.replaceAll("\\", "/").endsWith("files/strings.ts")
    )
      return false;
  }
  if (ts.isJsxText(node)) return /[A-Za-z]/.test(node.text.trim());
  if (!ts.isStringLiteral(node) && !ts.isNoSubstitutionTemplateLiteral(node) && !ts.isTemplateExpression(node))
    return false;
  const p = node.parent;
  if (
    (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) &&
    /^[a-z][\w]*(?:\.[\w]+){2,}$/.test(node.text)
  )
    return false;
  if (ts.isCallExpression(p) && /^(t|tf|counted|formatUnit)$/.test(p.expression.getText(source))) return false;
  if (
    ts.isImportDeclaration(p) ||
    ts.isExportDeclaration(p) ||
    ts.isLiteralTypeNode(p) ||
    ts.isCaseClause(p) ||
    ts.isElementAccessExpression(p)
  )
    return false;
  if (ts.isPropertyAssignment(p) && p.name === node) return false;
  if (
    ts.isBinaryExpression(p) &&
    [
      ts.SyntaxKind.EqualsEqualsEqualsToken,
      ts.SyntaxKind.ExclamationEqualsEqualsToken,
      ts.SyntaxKind.EqualsEqualsToken,
      ts.SyntaxKind.ExclamationEqualsToken,
    ].includes(p.operatorToken.kind)
  )
    return false;
  if (basename(file) === "strings.ts") return ts.isTemplateExpression(node) || /[A-Za-z]/.test(node.text);
  if (ts.isJsxAttribute(p)) return attributes.has(p.name.getText(source)) && node.text.length > 0;
  if (
    ts.isPropertyAssignment(p) &&
    [
      "className",
      "id",
      "value",
      "type",
      "key",
      "kind",
      "path",
      "url",
      "src",
      "provider",
      "command",
      "method",
      "format",
      "role",
      "hotkey",
      "shortcut",
      "icon",
      "color",
      "fontFamily",
    ].includes(p.name.getText(source))
  )
    return false;
  if (ts.isCallExpression(p) || ts.isNewExpression(p)) {
    const name = p.expression.getText(source);
    if (!/(set(Error|Failure|PasteFailure|Status|Message)|confirm|alert|Error|reject)$/.test(name)) return false;
  }
  const text = ts.isTemplateExpression(node)
    ? node.head.text + node.templateSpans.map((s) => `%@${s.literal.text}`).join("")
    : node.text;
  if (
    /^(?:url\(#|path\(|translate\(|M[-\d%@])/.test(text) ||
    /^(?:px|[KMGT]?B\/s|Mbps|HEX|RGB|HSL)$/i.test(text.trim())
  )
    return false;
  if (
    !/[A-Za-z]/.test(text) ||
    /^([.\/#?@]|https?:|data:|[A-Z]:\\)/.test(text) ||
    /\.(png|jpg|svg|json|exe|html|ts|tsx|cmd|webp|mp3|css)$/.test(text)
  )
    return false;
  if (!ts.isTemplateExpression(node) && !text.includes(" ") && text[0] === text[0].toLowerCase()) return false;
  if (ts.isJsxExpression(p)) return true;
  if (
    ts.isBinaryExpression(p) &&
    [ts.SyntaxKind.BarBarToken, ts.SyntaxKind.QuestionQuestionToken, ts.SyntaxKind.PlusToken].includes(
      p.operatorToken.kind,
    )
  )
    return text.includes(" ") || !!sourceKey(text, "");
  if (ts.isPropertyAssignment(p) && attributes.has(p.name.getText(source))) return true;
  if (ts.isConditionalExpression(p) && p.condition !== node) return text.includes(" ") || !!sourceKey(text, "");
  if (ts.isReturnStatement(p)) return text.includes(" ") || !!sourceKey(text, "");
  if (ts.isVariableDeclaration(p)) return /[A-Z]/.test(text[0]) && text.includes(" ");
  if ((ts.isCallExpression(p) || ts.isNewExpression(p)) && text.includes(" ")) return true;
  return false;
}
export function inventory() {
  const result = [];
  for (const file of files(resolve("src"))) {
    const text = readFileSync(file, "utf8");
    const source = ts.createSourceFile(
      file,
      text,
      ts.ScriptTarget.Latest,
      true,
      file.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS,
    );
    const feature = relative(resolve("src"), file).split(/[\\/]/)[0];
    const visit = (node) => {
      if (translatable(node, source, file)) {
        const text = ts.isTemplateExpression(node)
          ? node.head.text + node.templateSpans.map((s) => `%@${s.literal.text}`).join("")
          : ts.isJsxText(node)
            ? node.text.replace(/\s+/g, " ").trim()
            : node.text;
        result.push({
          file: relative(resolve("."), file).replaceAll("\\", "/"),
          line: source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1,
          text,
          key: sourceKey(text, feature) ?? null,
          template: ts.isTemplateExpression(node),
        });
      }
      ts.forEachChild(node, visit);
    };
    visit(source);
  }
  return result;
}
if (process.argv[1]?.endsWith("i18n-inventory.mjs")) {
  const result = inventory();
  mkdirSync("build", { recursive: true });
  writeFileSync("build/i18n-inventory.json", JSON.stringify(result, null, 2));
  const unique = [...new Map(result.map((r) => [r.text, r])).values()];
  writeFileSync(
    "build/i18n-unmatched.json",
    JSON.stringify(
      unique.filter((r) => !r.key),
      null,
      2,
    ),
  );
  console.log(
    JSON.stringify({
      occurrences: result.length,
      unique: unique.length,
      matched: unique.filter((r) => r.key).length,
      unmatched: unique.filter((r) => !r.key).length,
    }),
  );
}
