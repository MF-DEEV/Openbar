#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors
#
# Builds the Linux packages (.deb, .rpm, .AppImage) inside a fresh Ubuntu 22.04
# container, so they need no newer glibc than 2.35. Run it from the repository
# root with Docker or nerdctl, once per architecture:
#
#   docker run --rm --platform linux/amd64 -v "$PWD:/src:ro" -v "$PWD/dist-linux:/out" \
#     ubuntu:22.04 bash /src/windows/tools/linux-packages.sh
#
# (linux/arm64 for the ARM packages). The packages land in ./dist-linux.
set -euo pipefail

RUST_TOOLCHAIN="${RUST_TOOLCHAIN:-1.99.1}"
SRC="${SRC:-/src}"
OUT="${OUT:-/out}"
WORK="${WORK:-/build}"

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq --no-install-recommends \
  build-essential ca-certificates curl file git pkg-config unzip xz-utils \
  libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev \
  libssl-dev libxdo-dev libfuse2 >/dev/null

export PATH="$HOME/.cargo/bin:$HOME/.bun/bin:$PATH"
command -v cargo >/dev/null ||
  curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain "$RUST_TOOLCHAIN" >/dev/null
command -v bun >/dev/null || curl -fsSL https://bun.sh/install | bash >/dev/null

# A copy, so the source can stay read-only and Windows' node_modules stay out.
rm -rf "$WORK" && mkdir -p "$WORK"
tar -C "$SRC" --exclude=node_modules --exclude=target --exclude=dist --exclude=dist-linux -cf - windows | tar -C "$WORK" -xf -
cd "$WORK/windows"
bun install --frozen-lockfile

# linuxdeploy is an AppImage itself; a container has no FUSE to mount it with.
export APPIMAGE_EXTRACT_AND_RUN=1
bun run tauri build --bundles deb,rpm,appimage

mkdir -p "$OUT"
bundle=src-tauri/target/release/bundle
cp -v "$bundle"/deb/*.deb "$bundle"/rpm/*.rpm "$bundle"/appimage/*.AppImage "$OUT"/
