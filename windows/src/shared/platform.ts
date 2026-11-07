// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

/** Whether this page runs in WebKitGTK on Linux. */
export const onLinux = () => typeof navigator !== "undefined" && /\bLinux\b/.test(navigator.userAgent);
