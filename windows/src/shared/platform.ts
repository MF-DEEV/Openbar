// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

/** Whether this page runs in WebKitGTK on Linux. */
export const onLinux = () => typeof navigator !== "undefined" && /\bLinux\b/.test(navigator.userAgent);

/** Where the PC can listen for a voice reply: Windows only so far, the Linux build has no microphone support. */
export const voiceHeard = () => !onLinux();
