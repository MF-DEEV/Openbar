// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { expect, it } from "vitest";
import { isValidAlias, remoteStateText } from "./SessionsSettings";

it("takes ssh config aliases and never something ssh would read as an option", () => {
  expect(isValidAlias("openbar-linux")).toBe(true);
  expect(isValidAlias("dev@box.example.com")).toBe(true);
  expect(isValidAlias("[::1]")).toBe(true);
  expect(isValidAlias("-oProxyCommand=calc")).toBe(false);
  expect(isValidAlias("a b")).toBe(false);
  expect(isValidAlias("box;rm")).toBe(false);
  expect(isValidAlias("")).toBe(false);
});

it("says how a host stands", () => {
  expect(remoteStateText(null)).toMatch(/Connecting/);
  expect(remoteStateText({ state: "disconnected", reason: "Connection refused" })).toContain("Connection refused");
  expect(remoteStateText({ state: "connected", hasClient: true, hooksInstalled: true })).toMatch(/hooks set up/);
  expect(remoteStateText({ state: "connected", hasClient: true, hooksInstalled: false })).toMatch(/not set up/);
  expect(remoteStateText({ state: "connected", hasClient: false, hooksInstalled: false })).toMatch(
    /nc, socat or python3/,
  );
});
