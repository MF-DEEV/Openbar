#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Runs the bridge's own tests with any JDK 17 or later; the IDE's classes
# aren't needed, since the tested code keeps them behind an interface.
#
#   Tools/JetBrainsBridge/test.sh [--check-dist]
#
# --check-dist also checks that dist/openbar-terminal-bridge.jar was built
# from the sources as they are (build.sh writes their hash into it).
set -eu
cd "$(dirname "$0")"

JAVAC="${JAVA_HOME:+$JAVA_HOME/bin/}javac"
JAVA="${JAVA_HOME:+$JAVA_HOME/bin/}java"
OUT="build/test"
rm -rf "$OUT"
mkdir -p "$OUT"
# The classes that touch no IDE API, and the tests.
"$JAVAC" --release 17 -encoding UTF-8 -d "$OUT" \
    src/main/java/com/openbar/bridge/Json.java \
    src/main/java/com/openbar/bridge/Terminals.java \
    src/main/java/com/openbar/bridge/Protocol.java \
    src/main/java/com/openbar/bridge/Places.java \
    src/main/java/com/openbar/bridge/BridgeServer.java \
    src/test/java/com/openbar/bridge/BridgeTests.java
"$JAVA" -cp "$OUT" com.openbar.bridge.BridgeTests

if [ "${1:-}" = "--check-dist" ]; then
    built=$("$JAVA" -cp "$OUT" com.openbar.bridge.BridgeTests --built-from dist/openbar-terminal-bridge.jar)
    now=$(./source-hash.sh)
    if [ "$built" != "$now" ]; then
        echo "dist/openbar-terminal-bridge.jar was built from other sources ($built, now $now): run build.sh" >&2
        exit 1
    fi
    echo "dist/openbar-terminal-bridge.jar matches the sources"
fi
rm -rf build
