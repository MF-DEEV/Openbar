// One-time, syntax-aware migration. The repeatable Swift extraction is
// scripts/i18n-extract.mjs; this tool also records every source-key decision.
import ts from "typescript";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { resolve, relative, dirname, basename } from "node:path";
import { createHash } from "node:crypto";
import { files, flatten, english, sourceKey, translatable } from "./i18n-inventory.mjs";

const japanese = flatten(JSON.parse(readFileSync("src/i18n/generated/ja.json", "utf8")));
const windows = JSON.parse(readFileSync("src/i18n/overrides/en-US.json", "utf8")),
  manifest = JSON.parse(readFileSync("src/i18n/source-map.json", "utf8"));
const allFiles = files(resolve("src"));
const identifiers = new Set();
function keyFor(text, feature, node, source) {
  let key = sourceKey(text, feature);
  if (!key) key = Object.keys(windows).find((k) => windows[k] === text && k.startsWith(`windows.${feature}.`));
  if (!key) {
    const parts = [];
    for (let p = node.parent; p && !ts.isSourceFile(p); p = p.parent)
      if (ts.isPropertyAssignment(p) || ts.isMethodDeclaration(p))
        parts.unshift(p.name.getText(source).replaceAll('"', ""));
    const slug = text
      .replace(/%@/g, " value ")
      .replace(/[^a-zA-Z0-9 ]/g, " ")
      .trim()
      .split(/\s+/)
      .filter(Boolean)
      .slice(0, 8)
      .map((w, i) => (i ? w[0].toUpperCase() + w.slice(1) : w[0].toLowerCase() + w.slice(1)))
      .join("");
    key = `windows.${feature}.${parts.length ? parts.join(".") : slug || "text"}`;
    if (windows[key] && windows[key] !== text) key += createHash("sha256").update(text).digest("hex").slice(0, 6);
    windows[key] = text;
  } else if (
    english[key] !== undefined &&
    japanese[key] === english[key] &&
    /[a-zA-Z]{3}/.test(text) &&
    !/^(Claude Code|Codex|OpenCode|GitHub Copilot|API|CPU|GPU|USB|RGB|HEX|HSL|LRCLIB|Wi-Fi|Bluetooth|Pomodoro|Clawd|VS Code)$/.test(
      text,
    )
  ) {
    windows[key] = english[key];
  }
  return key;
}
for (const file of allFiles) {
  const original = readFileSync(file, "utf8");
  const source = ts.createSourceFile(
    file,
    original,
    ts.ScriptTarget.Latest,
    true,
    file.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS,
  );
  const feature = relative(resolve("src"), file).split(/[\\/]/)[0];
  const edits = [],
    used = new Set(),
    replacements = new Map(),
    lazy = new Map();
  const visit = (node) => {
    if (translatable(node, source, file)) {
      const template = ts.isTemplateExpression(node);
      const text = template
        ? node.head.text + node.templateSpans.map((s) => `%@${s.literal.text}`).join("")
        : ts.isJsxText(node)
          ? node.text.replace(/\s+/g, " ").trim()
          : node.text;
      const key = keyFor(text, feature, node, source);
      manifest.push({
        file: relative(resolve("."), file).replaceAll("\\", "/"),
        key,
        english: text,
        source: english[key] ? key : null,
      });
      replacements.set(node, { key, template });
    }
    ts.forEachChild(node, visit);
  };
  visit(source);
  const expr = (node) => {
    const match = replacements.get(node);
    if (match) {
      used.add(match.template ? "tf" : "t");
      return match.template
        ? `tf(${JSON.stringify(match.key)}, ${node.templateSpans.map((s) => expr(s.expression)).join(", ")})`
        : `t(${JSON.stringify(match.key)})`;
    }
    let value = node.getText(source);
    const children = [];
    const collect = (n) => {
      if (replacements.has(n)) children.push(n);
      else ts.forEachChild(n, collect);
    };
    ts.forEachChild(node, collect);
    for (const child of children.toSorted((a, b) => b.getStart(source) - a.getStart(source))) {
      const raw = expr(child),
        replacement = ts.isJsxText(child) || ts.isJsxAttribute(child.parent) ? `{${raw}}` : raw;
      value =
        value.slice(0, child.getStart(source) - node.getStart(source)) +
        replacement +
        value.slice(child.end - node.getStart(source));
    }
    return value;
  };
  // Module-level primitives must not freeze a translated value at import time.
  for (const statement of source.statements)
    if (
      ts.isVariableStatement(statement) &&
      !statement.modifiers?.some((m) => m.kind === ts.SyntaxKind.ExportKeyword)
    ) {
      for (const declaration of statement.declarationList.declarations)
        if (declaration.initializer && ts.isIdentifier(declaration.name) && replacements.has(declaration.initializer))
          lazy.set(declaration.name.text, declaration);
    }
  const ancestorsContainMatch = (node) => {
    for (let p = node.parent; p && !ts.isSourceFile(p); p = p.parent) if (replacements.has(p)) return true;
    return false;
  };
  const isModuleObject = (node) => {
    for (let p = node.parent; p && !ts.isSourceFile(p); p = p.parent) {
      if (ts.isFunctionLike(p)) return false;
    }
    return true;
  };
  const stringsImports = new Set();
  for (const statement of source.statements)
    if (ts.isImportDeclaration(statement) && statement.moduleSpecifier.text.endsWith("/strings")) {
      for (const e of statement.importClause?.namedBindings?.elements ?? []) stringsImports.add(e.name.text);
    }
  const rewrite = (node) => {
    if (replacements.has(node) && !ancestorsContainMatch(node)) {
      const value = expr(node),
        parent = node.parent;
      if (ts.isPropertyAssignment(parent) && parent.initializer === node && isModuleObject(parent)) {
        edits.push({
          start: parent.getStart(source),
          end: parent.end,
          text: `get ${parent.name.getText(source)}() { return ${value}; }`,
        });
      } else {
        const wrapper = ts.isJsxText(node) || ts.isJsxAttribute(parent);
        edits.push({
          start: node.getStart(source),
          end: node.end,
          text: wrapper
            ? `{${value}}`
            : lazy.has(parent.name?.text) && ts.isVariableDeclaration(parent)
              ? `() => ${value}`
              : value,
        });
      }
      return;
    }
    if (ts.isPropertyAssignment(node) && isModuleObject(node) && !replacements.has(node.initializer)) {
      const root = node.initializer.getText(source).split(".")[0];
      if (stringsImports.has(root))
        edits.push({
          start: node.getStart(source),
          end: node.end,
          text: `get ${node.name.getText(source)}() { return ${node.initializer.getText(source)}; }`,
        });
    }
    if (
      ts.isIdentifier(node) &&
      lazy.has(node.text) &&
      lazy.get(node.text).name !== node &&
      !ts.isPropertyAccessExpression(node.parent) &&
      !(ts.isPropertyAssignment(node.parent) && node.parent.name === node)
    ) {
      edits.push({ start: node.getStart(source), end: node.end, text: `${node.text}()` });
    }
    // Subscribe each component, including memoized children and portalled menus.
    // Preserve component identity and state while switching languages.
    if (
      file.endsWith(".tsx") &&
      (ts.isFunctionDeclaration(node) || ts.isFunctionExpression(node) || ts.isArrowFunction(node)) &&
      node.body
    ) {
      let name = node.name?.text;
      if (!name) {
        let p = node.parent;
        while (p && !ts.isSourceFile(p) && !ts.isVariableDeclaration(p) && !ts.isFunctionLike(p)) p = p.parent;
        if (p && ts.isVariableDeclaration(p) && ts.isIdentifier(p.name)) name = p.name.text;
      }
      let jsx = false;
      const hasJsx = (n) => {
        if (ts.isJsxElement(n) || ts.isJsxSelfClosingElement(n) || ts.isJsxFragment(n)) jsx = true;
        ts.forEachChild(n, hasJsx);
      };
      hasJsx(node.body);
      if (name && /^[A-Z]/.test(name) && jsx && !node.body.getText(source).includes("useLocale(")) {
        used.add("useLocale");
        if (ts.isBlock(node.body))
          edits.push({
            start: node.body.getStart(source) + 1,
            end: node.body.getStart(source) + 1,
            text: "\n  useLocale();",
          });
        else {
          // Expression-bodied components; preserve nested translations.
          const value = expr(node.body);
          edits.push({
            start: node.body.getStart(source),
            end: node.body.end,
            text: `{ useLocale(); return (${value}); }`,
          });
          return;
        }
      }
    }
    ts.forEachChild(node, rewrite);
  };
  rewrite(source);
  if (!edits.length) continue;
  let output = original;
  for (const edit of edits.toSorted((a, b) => b.start - a.start || b.end - a.end))
    output = output.slice(0, edit.start) + edit.text + output.slice(edit.end);
  if (used.size) {
    let path = relative(dirname(file), resolve("src/i18n/index")).replaceAll("\\", "/");
    if (!path.startsWith(".")) path = "./" + path;
    output = `import { ${[...used].join(", ")} } from ${JSON.stringify(path)};\n` + output;
  }
  writeFileSync(file, output);
}
mkdirSync("src/i18n/overrides", { recursive: true });
writeFileSync("src/i18n/overrides/en-US.json", JSON.stringify(windows, null, 2) + "\n");
writeFileSync("src/i18n/source-map.json", JSON.stringify(manifest, null, 2) + "\n");
console.log(`${manifest.length} text uses migrated, ${Object.keys(windows).length} Windows overrides`);
