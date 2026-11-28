#!/bin/zsh
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Builds the JetBrains bridge plugin against an installed JetBrains IDE and
# puts it in dist/, where the apps take it from:
#
#   Tools/JetBrainsBridge/build.sh ["/Applications/IntelliJ IDEA.app/Contents"]
#
# Any IDE from 2025.1 on will do; the plugin uses only what they all have
# and reaches the rest at run time. The jar records a hash of the sources
# it was built from, which `test.sh --check-dist` compares, so a change to
# the sources without a new jar fails in CI.
set -euo pipefail
cd "${0:A:h}"

IDE="${1:-${JETBRAINS_IDE:-/Applications/IntelliJ IDEA.app/Contents}}"
[[ -d "$IDE/lib" ]] || { print -u2 "No JetBrains IDE at $IDE (give its home, the folder holding lib/)"; exit 1; }
JAVAC=""
for candidate in "$IDE/jbr/Contents/Home/bin/javac" "$IDE/jbr/bin/javac" "${JAVA_HOME:-}/bin/javac"; do
    [[ -x "$candidate" ]] && { JAVAC="$candidate"; break; }
done
[[ -n "$JAVAC" ]] || { print -u2 "No javac in the IDE's runtime or JAVA_HOME"; exit 1; }

BUILD="build/plugin"
rm -rf "$BUILD"
mkdir -p "$BUILD/classes" dist
CLASSPATH=$(find "$IDE/lib" "$IDE/plugins/terminal/lib" -name '*.jar' | sort | paste -sd: -)
"$JAVAC" --release 17 -nowarn -encoding UTF-8 -cp "$CLASSPATH" -d "$BUILD/classes" src/main/java/com/openbar/bridge/*.java
cp -R src/main/resources/. "$BUILD/classes/"
./source-hash.sh > "$BUILD/classes/META-INF/openbar-source.sha256"
rm -f dist/openbar-terminal-bridge.jar
# A fixed date and order keep the jar the same from one build to the next.
find "$BUILD/classes" -exec touch -t 198001010000.02 {} +
(cd "$BUILD/classes" && find . -type f | LC_ALL=C sort | zip -X -D -q ../../../dist/openbar-terminal-bridge.jar -@)
rm -rf build
print "dist/openbar-terminal-bridge.jar ($(./source-hash.sh | cut -c1-12))"
