// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, expect, it, vi } from "vitest";
import { SessionDetail } from "./SessionDetail";
import { demoSnapshot } from "./demo";
import { useSessions } from "./store";

// Server rendering asks Zustand for its initial snapshot. Use the supplied
// test state so each case can exercise the saved preview preference.
vi.mock("./store", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./store")>();
  const store = actual.useSessions;
  return {
    ...actual,
    useSessions: Object.assign(
      (select: (state: ReturnType<typeof store.getState>) => unknown) => select(store.getState()),
      store,
    ),
  };
});

beforeEach(() => {
  useSessions.setState({ snapshot: { ...demoSnapshot(), previews: true }, wardrobe: false, renaming: false });
});

it("renders user messages and agent replies in order", () => {
  const session = {
    ...demoSnapshot().sessions[0]!,
    provider: "codex" as const,
    phase: "idle" as const,
    lastText: "Last answer",
    chat: [
      { id: 1, role: "user" as const, text: "My question", at: 1, endsTurn: false },
      { id: 2, role: "assistant" as const, text: "Last answer", at: 2, endsTurn: true },
    ],
  };
  const html = renderToStaticMarkup(<SessionDetail session={session} enlarged />);
  expect(html).toContain('role="log"');
  expect(html).not.toContain('class="s-message-role"');
  expect(html).toContain('class="s-message-time"');
  expect(html.indexOf("My question")).toBeLessThan(html.indexOf("Last answer"));
  expect(html.match(/Last answer/g)).toHaveLength(1);
});

it("replaces image labels with thumbnail controls in the compact title and conversation", () => {
  const session = {
    ...demoSnapshot().sessions[0]!,
    title: "[Image #1]",
    chat: [{ id: 1, role: "user" as const, text: "[Image #1]", images: ["C:\\photo.png"], at: 1, endsTurn: false }],
  };
  for (const enlarged of [false, true]) {
    const html = renderToStaticMarkup(<SessionDetail session={session} enlarged={enlarged} />);
    expect(html).toContain('aria-label="Open image 1"');
    expect(html).not.toContain("[Image #1]");
  }
  useSessions.setState((s) => ({ snapshot: { ...s.snapshot, previews: false } }));
  expect(renderToStaticMarkup(<SessionDetail session={session} enlarged />)).not.toContain('aria-label="Open image 1"');
});

it("keeps image labels when the original attachment is unavailable", () => {
  const session = { ...demoSnapshot().sessions[0]!, title: "[Image #1]", chat: [] };
  expect(renderToStaticMarkup(<SessionDetail session={session} />)).toContain("[Image #1]");
});

it("shows pasted images in the composer and allows an image-only reply", () => {
  const session = { ...demoSnapshot().sessions[0]!, phase: "idle" as const, permission: undefined };
  useSessions.setState({
    drafts: {},
    attachments: {
      [session.id]: [{ path: "C:\\photo.png", preview: "data:image/png;base64,abc" }],
    },
  });
  const html = renderToStaticMarkup(<SessionDetail session={session} />);
  expect(html).toContain('aria-label="Attached images"');
  expect(html).toContain('aria-label="Remove image 1"');
  expect(html).toContain('class="s-send is-ready"');
  useSessions.getState().removeAttachment(session.id, "C:\\photo.png");
  expect(useSessions.getState().attachments[session.id]).toEqual([]);
});

it("hides conversation content when previews are off and offers to enable it", () => {
  useSessions.setState((s) => ({ snapshot: { ...s.snapshot, previews: false } }));
  const session = {
    ...demoSnapshot().sessions[0]!,
    lastText: "Private answer",
    chat: [{ id: 1, role: "user" as const, text: "Private question", at: 1, endsTurn: false }],
  };
  const html = renderToStaticMarkup(<SessionDetail session={session} />);
  expect(html).not.toContain("Private question");
  expect(html).not.toContain("Private answer");
  expect(html).toContain("Show conversation");
});

it("shows an empty state when previews are on and there are no messages", () => {
  const session = { ...demoSnapshot().sessions[0]!, chat: [], lastText: undefined };
  expect(renderToStaticMarkup(<SessionDetail session={session} enlarged />)).toContain("Nothing said yet.");
});

it("keeps the compact Mac layout focused on session details and the last reply", () => {
  const session = {
    ...demoSnapshot().sessions[2]!,
    phase: "idle" as const,
    lastText: "Latest answer",
    chat: [
      { id: 1, role: "user" as const, text: "Earlier prompt", at: 1, endsTurn: false },
      { id: 2, role: "assistant" as const, text: "Earlier commentary", at: 2, endsTurn: false },
    ],
  };
  const html = renderToStaticMarkup(<SessionDetail session={session} />);
  expect(html).toContain("Recent");
  expect(html).toContain("Subagents");
  expect(html).toContain("Last reply");
  expect(html).toContain("Latest answer");
  expect(html).not.toContain("Earlier prompt");
  expect(html).not.toContain("Earlier commentary");
  expect(html).not.toContain('role="log"');
  expect(html).toContain('class="s-reply-field"');
});

it("matches the Mac enlarged view with a summary, Markdown, details and multiline composer", () => {
  const session = {
    ...demoSnapshot().sessions[2]!,
    phase: "finished" as const,
    chat: [
      { id: 1, role: "user" as const, text: "Please review", at: 1, endsTurn: false },
      { id: 2, role: "assistant" as const, text: "Reviewing", at: 2, endsTurn: false },
      {
        id: 3,
        role: "assistant" as const,
        text: "**Done**\n\n- Tests passed\n- [Release](https://example.com/release)\n\n`npm test`",
        at: 3,
        endsTurn: true,
      },
    ],
  };
  const html = renderToStaticMarkup(<SessionDetail session={session} enlarged />);
  expect(html).toContain('class="s-large-body"');
  expect(html).toContain('aria-label="Session details"');
  expect(html).toContain('aria-label="Hide details"');
  expect(html).toContain('class="s-message is-assistant is-on-the-way"');
  expect(html.match(/<span>Summary<\/span>/g)).toHaveLength(1);
  expect(html).toContain("<strong>Done</strong>");
  expect(html).toContain("<li>Tests passed</li>");
  expect(html).toContain('href="https://example.com/release"');
  expect(html).toContain("<code>npm test</code>");
  expect(html).toContain("<textarea");
  expect(html).toContain("Ctrl+Enter sends");
});
