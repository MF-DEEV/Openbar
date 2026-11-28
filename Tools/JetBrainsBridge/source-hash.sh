#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
#
# The hash of the plugin's sources: each file's path and content, in a fixed
# order, so the same sources give the same hash on every system.
set -eu
cd "$(dirname "$0")"
find src/main -type f | LC_ALL=C sort | while IFS= read -r file; do
    printf '%s\n' "$file"
    # Line endings as Git checks them out on Windows don't change the hash.
    tr -d '\r' < "$file"
done | { command -v sha256sum >/dev/null 2>&1 && sha256sum || shasum -a 256; } | cut -d' ' -f1
