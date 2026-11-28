// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { describe, expect, it } from "vitest";
import { containerTitle, hostTitle, isContainer, targetAlias, targetId, targetOf } from "./containers";
import { hooksGone } from "../settings/Containers";

describe("containers (#330)", () => {
  it("names a devcontainer by its folder and any other container by its name", () => {
    expect(targetAlias({ folder: "/home/me/shop" })).toBe("container:folder:/home/me/shop");
    expect(targetAlias({ name: "agent-box" })).toBe("container:name:agent-box");
    expect(containerTitle("container:folder:/home/me/shop/")).toBe("shop");
    expect(containerTitle("container:folder:C:\\Users\\me\\shop")).toBe("shop");
    expect(containerTitle("container:name:agent-box")).toBe("agent-box");
    expect(hostTitle("openbar-linux")).toBe("openbar-linux");
    expect(isContainer("openbar-linux")).toBe(false);
  });

  it("keeps a rebuilt devcontainer picked, and tells why its hooks are gone", () => {
    const info = { id: "new", name: "musing_goldwasser", image: "x", folder: "/p", user: "node", workspace: null };
    expect(targetId(targetOf(info))).toBe(targetId({ folder: "/p", setUp: "old" }));
    const connected = { state: "connected" as const, hasClient: true, hooksInstalled: false, container: "new" };
    expect(hooksGone(connected, { folder: "/p", setUp: "old" })).toBe("rebuilt");
    expect(hooksGone(connected, { folder: "/p", setUp: "new" })).toBe("gone");
    expect(hooksGone(connected, { folder: "/p" })).toBeNull();
    expect(hooksGone({ ...connected, hooksInstalled: true }, { folder: "/p", setUp: "old" })).toBeNull();
  });
});
