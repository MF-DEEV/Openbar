// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV and the Openbar contributors

// Recent batch approvals as a plain browser shows them, where no app answers (#281).

import type { BatchApproval } from "../sessions/types";

/** Shown in a plain browser, where no app answers. */
export const DEMO: BatchApproval[] = [
  {
    batch: "demo-2",
    at: Date.now() / 1000 - 90,
    sessionId: "demo-a",
    session: "Fix the flaky upload test",
    project: "openbar",
    agent: "claude",
    tool: "Bash",
    detail: "bun test src/upload.test.ts --rerun-each 5",
  },
  {
    batch: "demo-2",
    at: Date.now() / 1000 - 90,
    sessionId: "demo-b",
    session: "Tidy the README",
    project: "docs",
    agent: "codex",
    tool: "Edit",
    detail: "README.md",
    paths: ["README.md", "docs/README.md"],
  },
  {
    batch: "demo-1",
    at: Date.now() / 1000 - 3600 * 5,
    sessionId: "demo-a",
    session: "Fix the flaky upload test",
    project: "openbar",
    agent: "claude",
    tool: "Read",
    detail: "src/upload.ts",
  },
];
