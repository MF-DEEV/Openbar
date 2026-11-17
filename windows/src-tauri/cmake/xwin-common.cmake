# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Vorssaint
#
# Cross-builds whisper.cpp (voice replies) from a Mac with cargo-xwin. It
# wraps cargo-xwin's own clang-cl toolchain and fixes two things a non-Windows
# build machine gets wrong in whisper-rs-sys:
#   - the C runtime: CMake's MSVC defaults say /MD, which would need the
#     Visual C++ Redistributable at run time; /MT links it in, as the rest of
#     the app does;
#   - ggml-blas: its build script links this library whenever the build
#     machine is a Mac, though no BLAS is built for Windows; an empty archive
#     stands in for it.
# Used through tools/xwin-cargo.sh; a build on Windows needs none of this.

if(DEFINED ENV{XWIN_CACHE_DIR})
  set(_vors_xwin "$ENV{XWIN_CACHE_DIR}")
elseif(EXISTS "$ENV{HOME}/Library/Caches/cargo-xwin")
  set(_vors_xwin "$ENV{HOME}/Library/Caches/cargo-xwin")
else()
  set(_vors_xwin "$ENV{HOME}/.cache/cargo-xwin")
endif()
include("${_vors_xwin}/cmake/clang-cl/${VORS_XWIN_TARGET}-toolchain.cmake")
set(CMAKE_USER_MAKE_RULES_OVERRIDE "${CMAKE_CURRENT_LIST_DIR}/xwin-static-crt.cmake")
set(CMAKE_MSVC_RUNTIME_LIBRARY MultiThreaded CACHE STRING "" FORCE)
file(WRITE "${CMAKE_BINARY_DIR}/ggml-blas.lib" "!<arch>\n")
