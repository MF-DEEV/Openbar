#!/usr/bin/env python3
"""Prototype: extract Vorssaint's compiled-in Swift string tables to JSON.

The Mac app keeps every user-facing string in Swift structs (see
windows/docs/spec/06-i18n.md). This script tokenizes the Swift source (strings,
comments, interpolation, raw and multi-line literals) instead of running a
line regex, then reads three shapes:

  1. `struct FooStrings { let a: String ... }`             -> the field list
  2. `static let enUS = FooStrings(a: "...", ...)`          -> one table
     (also `static let enUS = Strings(...)`, the main catalog)
  3. `static func foo(_ language: AppLanguage) -> FooStrings {
         switch language {
         case .enUS: return FooStrings(a: "...")   // inline table
         case .ptBR: return .ptBR                  // reference to (2)
         case .zhTW, .zhHK: return .zhTW           // alias
         case .ja: return .init(a: "...")          // inline .init
     }}`

Output: one JSON file per language: {"<accessor>": {"<field>": "<value>"}}.
The accessor is the static func name (`notchFiles`, `clipboard`, ...) and `s`
for the main `Strings` catalog (Swift's `l10n.s`). Values that are not plain
string literals (interpolation, references, dictionaries, function calls) are
reported, never guessed.

Usage:
  python3 extract_swift_strings.py <Core dir> [--out DIR] [--only FILE ...]
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
from dataclasses import dataclass, field

LANGS = ["enUS", "ptBR", "tr", "ru", "es", "sk", "de", "fr", "it", "ja", "ko", "uk", "zhHans", "zhTW", "zhHK"]
BCP47 = {"enUS": "en-US", "ptBR": "pt-BR", "zhHans": "zh-Hans", "zhTW": "zh-TW", "zhHK": "zh-HK"}


# ---------------------------------------------------------------- tokenizer

@dataclass
class Tok:
    kind: str          # ident | string | punct | number | op
    text: str          # identifier/punct text, or decoded string value
    line: int
    interp: bool = False   # string contained \( ... )


ESCAPES = {"n": "\n", "t": "\t", "r": "\r", "0": "\0", "\\": "\\", '"': '"', "'": "'"}


def tokenize(src: str) -> list[Tok]:
    toks: list[Tok] = []
    i, n, line = 0, len(src), 1
    while i < n:
        c = src[i]
        if c == "\n":
            line += 1; i += 1; continue
        if c in " \t\r":
            i += 1; continue
        if src.startswith("//", i):
            j = src.find("\n", i)
            i = n if j < 0 else j
            continue
        if src.startswith("/*", i):
            depth, i = 1, i + 2
            while i < n and depth:
                if src.startswith("/*", i): depth += 1; i += 2
                elif src.startswith("*/", i): depth -= 1; i += 2
                else:
                    if src[i] == "\n": line += 1
                    i += 1
            continue
        # string literal, optionally raw (#...#) and/or multi-line (""")
        m = re.match(r'(#*)("""|")', src[i:])
        if m:
            hashes, quote = m.group(1), m.group(2)
            start_line = line
            i += len(m.group(0))
            closing = quote + hashes
            out, interp = [], False
            if quote == '"""':
                # skip to end of opening line
                j = src.find("\n", i); line += 1; i = j + 1
            esc = "\\" + hashes
            while i < n and not src.startswith(closing, i):
                if src.startswith(esc, i):
                    k = i + len(esc)
                    ch = src[k] if k < n else ""
                    if ch == "(":
                        # interpolation: skip balanced parens
                        interp, depth, k = True, 1, k + 1
                        while k < n and depth:
                            if src[k] == "(": depth += 1
                            elif src[k] == ")": depth -= 1
                            k += 1
                        out.append("￼")  # placeholder for unresolved interpolation
                        i = k; continue
                    if ch == "u" and src.startswith("{", k + 1):
                        e = src.find("}", k)
                        out.append(chr(int(src[k + 2:e], 16))); i = e + 1; continue
                    if ch == "\n" and quote == '"""':  # line continuation
                        line += 1; i = k + 1; continue
                    out.append(ESCAPES.get(ch, ch)); i = k + 1; continue
                if src[i] == "\n":
                    line += 1
                out.append(src[i]); i += 1
            i += len(closing)
            value = "".join(out)
            if quote == '"""':
                # strip the indentation of the closing delimiter line
                lines = value.split("\n")
                indent = len(lines[-1]) - len(lines[-1].lstrip()) if lines else 0
                value = "\n".join(l[indent:] for l in lines[:-1])
            toks.append(Tok("string", value, start_line, interp))
            continue
        if c.isalpha() or c == "_":
            j = i
            while j < n and (src[j].isalnum() or src[j] == "_"):
                j += 1
            toks.append(Tok("ident", src[i:j], line)); i = j; continue
        if c.isdigit():
            j = i
            while j < n and (src[j].isalnum() or src[j] in "._"):
                j += 1
            toks.append(Tok("number", src[i:j], line)); i = j; continue
        if src.startswith("->", i):
            toks.append(Tok("op", "->", line)); i += 2; continue
        toks.append(Tok("punct", c, line)); i += 1
    return toks


# ---------------------------------------------------------------- parser

@dataclass
class Table:
    struct: str
    values: dict          # field -> str | None (None = unresolved)
    unresolved: dict      # field -> raw expression text
    file: str
    line: int


@dataclass
class Accessor:
    name: str
    struct: str
    file: str
    cases: dict = field(default_factory=dict)   # lang -> ("inline", Table) | ("ref", ident)
    owner: str | None = None                    # type the func is declared on
    delegate: tuple | None = None               # (Type, func) for `Type.func(language)` bodies


class Parser:
    def __init__(self, toks: list[Tok], file: str):
        self.t, self.i, self.file = toks, 0, file
        self.structs: dict[str, list[tuple[str, str]]] = {}
        self.tables: dict[tuple[str, str], Table] = {}   # (struct, ident) -> table
        self.accessors: list[Accessor] = []

    def peek(self, k=0):
        j = self.i + k
        return self.t[j] if j < len(self.t) else Tok("eof", "", -1)

    def is_(self, k, text, kind=None):
        tok = self.peek(k)
        return tok.text == text and (kind is None or tok.kind == kind)

    def skip_balanced(self, open_, close):
        """self.i at `open_`; returns index just past the matching close."""
        depth = 0
        while self.i < len(self.t):
            tok = self.t[self.i]
            if tok.kind == "punct" and tok.text == open_: depth += 1
            elif tok.kind == "punct" and tok.text == close:
                depth -= 1
                if depth == 0:
                    self.i += 1; return
            self.i += 1

    def parse(self):
        ext_stack: list[tuple[str, int]] = []   # (type name, brace depth at open)
        depth = 0
        while self.i < len(self.t):
            tok = self.peek()
            if tok.kind == "punct" and tok.text == "{":
                depth += 1; self.i += 1; continue
            if tok.kind == "punct" and tok.text == "}":
                depth -= 1
                while ext_stack and ext_stack[-1][1] > depth:
                    ext_stack.pop()
                self.i += 1; continue
            if tok.kind == "ident" and tok.text in ("struct", "extension", "enum") and self.peek(1).kind == "ident":
                name = self.peek(1).text
                # find the opening brace of the declaration
                j = self.i + 2
                while j < len(self.t) and not (self.t[j].kind == "punct" and self.t[j].text in "{;"):
                    j += 1
                if j < len(self.t) and self.t[j].text == "{":
                    if tok.text == "struct" and (name.endswith("Strings") or name == "Strings"):
                        self.i = j
                        self.read_struct_fields(name)
                        continue
                    ext_stack.append((name, depth + 1))
                    self.i = j
                    continue
            if tok.kind == "ident" and tok.text == "static":
                owner = ext_stack[-1][0] if ext_stack else None
                if self.is_(1, "let") and self.peek(2).kind == "ident" and self.is_(3, "="):
                    ident = self.peek(2).text
                    self.i += 4
                    t = self.try_constructor(owner)
                    if t:
                        self.tables[(t.struct, ident)] = t
                    continue
                if self.is_(1, "func") and self.peek(2).kind == "ident":
                    self.read_accessor(owner=owner)
                    continue
            self.i += 1

    def read_struct_fields(self, name):
        # self.i at '{'
        fields: list[tuple[str, str]] = []
        depth = 0
        while self.i < len(self.t):
            tok = self.peek()
            if tok.kind == "punct" and tok.text == "{": depth += 1
            elif tok.kind == "punct" and tok.text == "}":
                depth -= 1
                if depth == 0:
                    self.i += 1; break
            elif depth == 1 and tok.kind == "ident" and tok.text == "static" and self.is_(1, "let") \
                    and self.peek(2).kind == "ident" and self.is_(3, "="):
                # a table declared inside the struct body itself
                ident = self.peek(2).text
                self.i += 4
                t = self.try_constructor(name)
                if t:
                    self.tables[(t.struct, ident)] = t
                continue
            elif depth == 1 and tok.kind == "ident" and tok.text == "static" and self.is_(1, "func") \
                    and self.peek(2).kind == "ident":
                self.read_accessor(owner=name)
                continue
            elif depth == 1 and tok.kind == "ident" and tok.text == "let" and self.peek(1).kind == "ident" and self.is_(2, ":"):
                fname = self.peek(1).text
                # type: tokens until next declaration keyword at this depth
                j, typ = self.i + 3, []
                while j < len(self.t) and not (self.t[j].kind == "ident" and self.t[j].text in
                                                ("let", "var", "func", "static", "init", "enum", "struct", "private")) \
                        and not (self.t[j].kind == "punct" and self.t[j].text == "}"):
                    typ.append(self.t[j].text); j += 1
                fields.append((fname, "".join(typ)))
                self.i = j; continue
            self.i += 1
        # a struct may be split across extensions; keep the first declaration
        self.structs.setdefault(name, fields)

    def try_constructor(self, owner) -> Table | None:
        """At `Type(` / `.init(` after `=` or `return`; parse argument list."""
        tok = self.peek()
        struct = None
        if tok.kind == "ident" and self.is_(1, "("):
            struct = owner if tok.text == "Self" else tok.text; self.i += 1
        elif tok.text == "." and self.is_(1, "init") and self.is_(2, "("):
            struct = owner; self.i += 2
        if struct is None or not (struct.endswith("Strings") or struct == "Strings" or struct == owner):
            return None
        line = self.peek().line
        args = self.read_args()
        values, unresolved = {}, {}
        for label, expr in args:
            if label is None:
                continue
            v = literal_value(expr)
            if v is None:
                unresolved[label] = " ".join(x.text if x.kind != "string" else json.dumps(x.text, ensure_ascii=False)
                                             for x in expr)[:160]
                values[label] = None
            else:
                values[label] = v
        return Table(struct, values, unresolved, self.file, line)

    def read_args(self):
        # self.i at '('
        assert self.peek().text == "("
        self.i += 1
        args, cur, depth = [], [], 0
        while self.i < len(self.t):
            tok = self.peek()
            if tok.kind == "punct" and tok.text in "([{": depth += 1
            if tok.kind == "punct" and tok.text in ")]}":
                if depth == 0:
                    self.i += 1
                    if cur: args.append(cur)
                    break
                depth -= 1
            if depth == 0 and tok.kind == "punct" and tok.text == ",":
                args.append(cur); cur = []; self.i += 1; continue
            cur.append(tok); self.i += 1
        out = []
        for a in args:
            if len(a) >= 2 and a[0].kind == "ident" and a[1].text == ":":
                out.append((a[0].text, a[2:]))
            else:
                out.append((None, a))
        return out

    def read_accessor(self, owner=None):
        # static func name(_ language: AppLanguage) -> Type {
        name = self.peek(2).text
        j = self.i + 3
        sig = []
        while j < len(self.t) and not (self.t[j].kind == "punct" and self.t[j].text == "{"):
            sig.append(self.t[j]); j += 1
        texts = [x.text for x in sig]
        if texts[:5] != ["(", "_", "language", ":", "AppLanguage"] or "->" not in texts:
            self.i += 3; return
        ret = texts[texts.index("->") + 1]
        if ret == "Self" and owner:
            ret = owner
        acc = Accessor(name, ret, self.file, owner=owner)
        self.i = j
        # walk the body
        depth = 0
        pending: list[str] = []
        while self.i < len(self.t):
            tok = self.peek()
            if tok.kind == "punct" and tok.text == "{":
                depth += 1; self.i += 1; continue
            if tok.kind == "punct" and tok.text == "}":
                depth -= 1; self.i += 1
                if depth == 0: break
                continue
            if not acc.cases and tok.kind == "ident" and self.is_(1, ".") and self.peek(2).kind == "ident" \
                    and self.is_(3, "(") and self.is_(4, "language"):
                acc.delegate = (tok.text, self.peek(2).text)
            if tok.kind == "ident" and tok.text in ("case", "default"):
                pending = []
                self.i += 1
                if tok.text == "default":
                    pending = ["*"]
                else:
                    while self.is_(0, "."):
                        pending.append(self.peek(1).text); self.i += 2
                        if self.is_(0, ","): self.i += 1
                continue
            if tok.kind == "ident" and tok.text == "return" and pending:
                self.i += 1
                if self.is_(0, ".") and self.peek(1).kind == "ident" and not self.is_(1, "init"):
                    for lang in pending: acc.cases[lang] = ("ref", self.peek(1).text)
                    self.i += 2
                elif self.peek().kind == "ident" and self.is_(1, ".") and self.peek(2).kind == "ident" and not self.is_(3, "("):
                    for lang in pending: acc.cases[lang] = ("ref", self.peek(2).text)
                    self.i += 3
                else:
                    t = self.try_constructor(ret)
                    if t:
                        for lang in pending: acc.cases[lang] = ("inline", t)
                    else:
                        for lang in pending: acc.cases[lang] = ("unparsed", self.peek().line)
                pending = []
                continue
            self.i += 1
        self.accessors.append(acc)


def literal_value(expr: list[Tok]):
    """A string literal, literals joined by `+`, or a `[.case: "literal", ...]`
    dictionary (returned as a dict). Anything else -> None."""
    if not expr:
        return None
    if expr[0].text == "[" and expr[-1].text == "]":
        body, out = expr[1:-1], {}
        k = 0
        while k < len(body):
            if body[k].text == "," :
                k += 1; continue
            if k + 3 < len(body) + 1 and body[k].text == "." and k + 3 < len(body) and body[k + 2].text == ":" \
                    and body[k + 3].kind == "string" and not body[k + 3].interp:
                out[body[k + 1].text] = body[k + 3].text
                k += 4; continue
            return None
        return out
    parts = []
    for k, tok in enumerate(expr):
        if k % 2 == 0:
            if tok.kind != "string" or tok.interp:
                return None
            parts.append(tok.text)
        elif tok.text != "+":
            return None
    return "".join(parts) if len(expr) % 2 == 1 else None


# ---------------------------------------------------------------- driver

FORMAT_RE = re.compile(r"%(?:\d+\$)?(?:@|l{0,2}d|l{0,2}u|\.?\d*f|s|%)")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("core", help="Sources/Vorssaint/Core")
    ap.add_argument("--out", help="directory for <lang>.json files")
    ap.add_argument("--only", nargs="*", help="limit to these file names (plus Localizations/ for Strings)")
    a = ap.parse_args()

    files = []
    for root, _, names in os.walk(a.core):
        for nm in sorted(names):
            if nm.endswith(".swift") and ("Strings" in nm or nm == "Localization.swift" or root.endswith("Localizations")):
                if a.only and nm not in a.only:
                    continue
                files.append(os.path.join(root, nm))

    structs, tables, accessors = {}, {}, []
    for f in files:
        with open(f, encoding="utf-8") as fh:
            p = Parser(tokenize(fh.read()), os.path.relpath(f, a.core))
        p.parse()
        for k, v in p.structs.items(): structs.setdefault(k, v)
        tables.update(p.tables)
        accessors.extend(p.accessors)

    # the main catalog has no accessor func of its own; Strings.localized() maps
    # languages 1:1 to static lets of the same name.
    for x in accessors:
        if x.struct == "Strings" and x.owner == "Strings" and x.name == "localized":
            x.name, x.owner = "s", None      # Swift's l10n.s
    if any(s == "Strings" for (s, _) in tables) and not any(x.name == "s" for x in accessors):
        acc = Accessor("s", "Strings", "Localization.swift")
        for lang in LANGS: acc.cases[lang] = ("ref", lang)
        accessors.append(acc)

    by_key = {(x.owner, x.name): x for x in accessors}
    for acc in accessors:
        seen = 0
        while acc.delegate and not acc.cases and seen < 4:
            target = by_key.get(acc.delegate)
            if target is None:
                break
            acc.cases, acc.delegate = dict(target.cases), target.delegate
            seen += 1
    # accessors declared on the strings type itself (`localized`) are reached
    # through the FeatureStrings one that delegates to them
    accessors = [x for x in accessors if x.owner in (None, "FeatureStrings", "Strings") or x.owner == x.struct and
                 not any(y.delegate is None and y.struct == x.struct and y is not x and y.cases == x.cases
                         for y in accessors if y.owner in (None, "FeatureStrings"))]
    per_lang = {lang: {} for lang in LANGS}
    report = {"files": len(files), "structs": len(structs), "accessors": 0, "problems": [],
              "fields": 0, "values": 0, "unresolved": 0, "format_specifiers": {}}
    for acc in accessors:
        if acc.struct not in structs:
            report["problems"].append(f"{acc.name} -> {acc.struct}: return type is not a known strings struct; skipped")
            continue
        report["accessors"] += 1
        fields = structs[acc.struct]
        report["fields"] += len(fields)
        for lang in LANGS:
            case = acc.cases.get(lang) or acc.cases.get("*")
            if case is None:
                report["problems"].append(f"{acc.name}: no case for {lang}"); continue
            if case[0] == "ref":
                table = tables.get((acc.struct, case[1]))
                if table is None:
                    report["problems"].append(f"{acc.name}.{lang}: reference .{case[1]} not found"); continue
            elif case[0] == "inline":
                table = case[1]
            else:
                report["problems"].append(f"{acc.name}.{lang}: unparsed return at line {case[1]}"); continue
            key = acc.name if acc.owner in (None, "FeatureStrings") else f"{acc.owner}.{acc.name}"
            if lang == "enUS" and key in per_lang[lang]:
                report["problems"].append(f"{key}: two accessors share this key")
            out = per_lang[lang].setdefault(key, {})
            for fname, ftype in fields:
                if fname not in table.values:
                    report["problems"].append(f"{acc.name}.{lang}: missing field {fname}"); continue
                v = table.values[fname]
                if v is None:
                    report["unresolved"] += 1
                    if lang == "enUS":
                        report["problems"].append(f"{acc.name}.{fname} ({ftype}): non-literal -> {table.unresolved[fname]}")
                    continue
                out[fname] = v
                report["values"] += 1
                for spec in FORMAT_RE.findall(v if isinstance(v, str) else " ".join(v.values())):
                    report["format_specifiers"][spec] = report["format_specifiers"].get(spec, 0) + 1
            extra = set(table.values) - {f for f, _ in fields}
            if extra:
                report["problems"].append(f"{acc.name}.{lang}: extra labels {sorted(extra)}")

    if a.out:
        os.makedirs(a.out, exist_ok=True)
        for lang, data in per_lang.items():
            with open(os.path.join(a.out, BCP47.get(lang, lang) + ".json"), "w", encoding="utf-8") as fh:
                json.dump(data, fh, ensure_ascii=False, indent=1, sort_keys=True)
    problems = report.pop("problems")
    print(json.dumps(report, indent=1, ensure_ascii=False))
    print(f"problems: {len(problems)}")
    for p in problems[:80]:
        print("  -", p)


if __name__ == "__main__":
    sys.exit(main())
