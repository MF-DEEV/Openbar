#!/usr/bin/env bash
# Runs a cargo-xwin build (or `tauri build --runner cargo-xwin`) with the
# CMake toolchain whisper.cpp needs when cross-building from a Mac:
#   tools/xwin-cargo.sh cargo xwin build --release --target aarch64-pc-windows-msvc
#   tools/xwin-cargo.sh bunx tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc
# cmake-rs reads CMAKE_TOOLCHAIN_FILE_<target> with the target's dashes before
# the underscored name cargo-xwin sets, so this one wins. Bash can't export a
# name with dashes, hence env.
set -euo pipefail
cmake_dir="$(cd "$(dirname "$0")/../src-tauri/cmake" && pwd)"
exec env \
  "CMAKE_TOOLCHAIN_FILE_aarch64-pc-windows-msvc=$cmake_dir/xwin-aarch64-pc-windows-msvc.cmake" \
  "CMAKE_TOOLCHAIN_FILE_x86_64-pc-windows-msvc=$cmake_dir/xwin-x86_64-pc-windows-msvc.cmake" \
  "$@"
