#!/usr/bin/env bash
# Ground-truth check for extract_swift_strings.py (macOS, needs swiftc).
# Compiles real Swift string tables with tiny stubs, dumps every language via
# Mirror, and diffs the result against the extractor's JSON.
# Usage: windows/tools/i18n-extract/ground_truth.sh   (from the repo root)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
CORE="$ROOT/Sources/Vorssaint/Core"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

python3 -I "$ROOT/windows/tools/i18n-extract/extract_swift_strings.py" "$CORE" --out "$WORK/json" >/dev/null

DUMP='func dump(_ v: Any) -> [String: String] {
    var out: [String: String] = [:]
    for c in Mirror(reflecting: v).children { if let l = c.label, let s = c.value as? String { out[l] = s } }
    return out
}
func write(_ all: [String: [String: [String: String]]]) {
    let data = try! JSONSerialization.data(withJSONObject: all, options: [.sortedKeys])
    FileManager.default.createFile(atPath: CommandLine.arguments[1], contents: data)
}'

# 1. Two feature tables (inline cases and static-let tables).
mkdir -p "$WORK/a"
cat > "$WORK/a/main.swift" <<EOF
import Foundation
enum AppLanguage: String, CaseIterable { case enUS = "en-US", ptBR = "pt-BR", tr, ru, es, sk, de, fr, it, ja, ko, uk, zhHans = "zh-Hans", zhTW = "zh-TW", zhHK = "zh-HK" }
enum FeatureStrings {}
$DUMP
var all: [String: [String: [String: String]]] = [:]
for l in AppLanguage.allCases { all[l.rawValue] = ["notchFiles": dump(FeatureStrings.notchFiles(l)), "notch": dump(FeatureStrings.notch(l))] }
write(all)
EOF
swiftc -Onone -o "$WORK/a/dump" "$WORK/a/main.swift" "$CORE/NotchFilesStrings.swift" "$CORE/NotchStrings.swift"
"$WORK/a/dump" "$WORK/a/truth.json"

# 2. The main catalog (Localization.swift + Localizations/*.swift).
mkdir -p "$WORK/b"
cat > "$WORK/b/main.swift" <<EOF
import Foundation
enum DefaultsKey { static let language = "appLanguage" }
$DUMP
var all: [String: [String: [String: String]]] = [:]
for l in AppLanguage.allCases { all[l.rawValue] = ["s": dump(Strings.localized(l))] }
write(all)
EOF
swiftc -Onone -o "$WORK/b/dump" "$WORK/b/main.swift" "$CORE/Localization.swift" "$CORE"/Localizations/*.swift
"$WORK/b/dump" "$WORK/b/truth.json"

python3 -I - "$WORK" <<'EOF'
import json, sys
work = sys.argv[1]
bad = n = 0
for truth in ("a", "b"):
    t = json.load(open(f"{work}/{truth}/truth.json"))
    for lang, accs in t.items():
        e = json.load(open(f"{work}/json/{lang}.json", encoding="utf-8"))
        for acc, fields in accs.items():
            got = e.get(acc, {})
            if set(fields) != set(got):
                bad += 1; print("key set differs:", lang, acc)
            for k, v in fields.items():
                n += 1
                if got.get(k) != v:
                    bad += 1; print("mismatch:", lang, acc, k)
print(f"compared {n} values, {bad} problems")
sys.exit(1 if bad else 0)
EOF
