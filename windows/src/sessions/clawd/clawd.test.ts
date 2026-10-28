// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Ported from Tests/ClawdCostumeTests.swift and Tests/NotchClawdSpriteTests.swift.

import { afterEach, describe, expect, it, vi } from "vitest";
import {
  CLASSIC,
  ClawdRandom,
  costumeCode,
  COSTUMES_KEPT,
  EXTRA_CHANCE,
  EXTRAS,
  EYEWEAR,
  EYEWEAR_CHANCE,
  HAT_CHANCE,
  HATS,
  keptCode,
  localCostumeStore,
  parseCostume,
  randomCostume,
  resolvedCostume,
  sameCostume,
  seed,
  seeded,
  SHELL_RGB,
  shellForAgentColor,
  SHELLS,
  subagentKey,
  withKept,
  type ClawdCostume,
} from "./costume";
import {
  ACCENT,
  basePose,
  blinks,
  BODY_X,
  BODY_Y,
  CHEER_LENGTH,
  clippedPixels,
  color,
  FRAME,
  grid,
  HEIGHT,
  layout,
  letters,
  motionForNotice,
  motionForPhase,
  motionForSubagent,
  MOTIONS,
  pixels,
  pose,
  poseKey,
  WIDTH,
  type ClawdMotion,
  type ClawdPose,
} from "./sprite";

const key = (c: ClawdCostume) => costumeCode(c);

describe("clawd costume lot", () => {
  it("is FNV-1a over the id's bytes", () => {
    expect(seed("")).toBe(0xcbf2_9ce4_8422_2325n);
    expect(seed("a")).toBe(0xaf63_dc4c_8601_ec8cn);
  });

  it("is SplitMix64, giving its published numbers", () => {
    const g = new ClawdRandom(0n);
    expect(g.next()).toBe(0xe220_a839_7b1d_cdafn);
    expect(g.next()).toBe(0x6e78_9e6a_a1b9_65f4n);
    const a = new ClawdRandom(42n);
    const first = [a.next(), a.next(), a.next(), a.next()];
    const b = new ClawdRandom(42n);
    expect([b.next(), b.next(), b.next(), b.next()]).toEqual(first);
    expect(new Set(first).size).toBe(4);
  });

  it("draws the same costume as the Mac", () => {
    const id = "99c35831-5f0b-4c3e-9d0a-0f3c2a7b8e11";
    expect(sameCostume(seeded(id), seeded(id))).toBe(true);
    expect(costumeCode(seeded(id))).toBe("v1:clay.none.flashMask.none");
    expect(costumeCode(seeded("0f9a6c1e-2b7d-4e55-8a31-c4d2e6f7a890"))).toBe("v1:coral.beret.anaglyph.lanternEmblem");
    expect(costumeCode(seeded("s/a1"))).toBe("v1:coral.beret.none.strangeCape");
  });

  it("hashes non-ASCII ids as UTF-8", () => {
    // é is two bytes, c3 a9; by hand: ((basis ^ 0xc3) * p ^ 0xa9) * p.
    const p = 0x100000001b3n;
    const m = (1n << 64n) - 1n;
    const expected = (((((0xcbf29ce484222325n ^ 0xc3n) * p) & m) ^ 0xa9n) * p) & m;
    expect(seed("é")).toBe(expected);
  });

  it("draws every accessory for new Codex chats too", () => {
    const costumes = Array.from({ length: 600 }, (_, i) => seeded(`new-codex-${i}`, null, "codex"));
    expect(new Set(costumes.map((c) => c.hat))).toEqual(new Set(HATS));
    expect(new Set(costumes.map((c) => c.eyewear))).toEqual(new Set(EYEWEAR));
    expect(new Set(costumes.map((c) => c.extra))).toEqual(new Set(EXTRAS));
  });

  it("gives new sessions every part about as often as meant", () => {
    const costumes = Array.from({ length: 600 }, (_, i) => seeded(`session-${i}`));
    expect(new Set(costumes.map((c) => c.shell))).toEqual(new Set(SHELLS));
    expect(new Set(costumes.map((c) => c.hat))).toEqual(new Set(HATS));
    expect(new Set(costumes.map((c) => c.eyewear))).toEqual(new Set(EYEWEAR));
    expect(new Set(costumes.map((c) => c.extra))).toEqual(new Set(EXTRAS));
    const share = (n: number) => n / costumes.length;
    expect(Math.abs(share(costumes.filter((c) => c.hat === "none").length) - (1 - HAT_CHANCE))).toBeLessThan(0.08);
    expect(Math.abs(share(costumes.filter((c) => c.eyewear === "none").length) - (1 - EYEWEAR_CHANCE))).toBeLessThan(
      0.08,
    );
    expect(Math.abs(share(costumes.filter((c) => c.extra === "none").length) - (1 - EXTRA_CHANCE))).toBeLessThan(0.08);
    const clay = costumes.filter((c) => c.shell === "clay").length;
    const sky = costumes.filter((c) => c.shell === "sky").length;
    expect(clay).toBeGreaterThan(sky * 1.8);
    expect(new Set(costumes.map(key)).size).toBeGreaterThan(400);
  });

  it("Surprise me always changes something", () => {
    const current = seeded("99c35831-5f0b-4c3e-9d0a-0f3c2a7b8e11");
    for (let i = 0; i < 30; i++) expect(sameCostume(randomCostume(current), current)).toBe(false);
  });

  it("gives Claude Code's agent colors their shells", () => {
    const colors = ["red", "orange", "yellow", "green", "cyan", "blue", "purple", "pink"];
    expect(colors.map(shellForAgentColor)).toEqual(["coral", "clay", "amber", "moss", "teal", "sky", "iris", "rose"]);
    expect(shellForAgentColor(" Blue\n")).toBe("sky");
    expect(shellForAgentColor("teal")).toBeNull();
    expect(shellForAgentColor("")).toBeNull();
    expect(seeded("s/a1", "green")).toEqual({ ...seeded("s/a1"), shell: "moss" });
    expect(seeded("s/a1", null)).toEqual(seeded("s/a1"));
    expect(seeded("s/a1", "mauve")).toEqual(seeded("s/a1"));
    expect(subagentKey("s", "a1")).toBe("s/a1");
  });
});

describe("clawd costume keeping", () => {
  const costume: ClawdCostume = { shell: "sky", hat: "topHat", eyewear: "shades", extra: "bowTie" };
  const fallback: ClawdCostume = { shell: "rose", hat: "crown", eyewear: "monocle", extra: "coffee" };

  it("keeps as a short versioned line and reads back", () => {
    expect(costumeCode(costume)).toBe("v1:sky.topHat.shades.bowTie");
    expect(parseCostume(costumeCode(costume), CLASSIC)).toEqual(costume);
    for (const c of [
      CLASSIC,
      ...Array.from({ length: 50 }, (_, i) => seeded(`k${i}`)),
      ...HATS.map((hat) => ({ ...CLASSIC, hat })),
      ...EYEWEAR.map((eyewear) => ({ ...CLASSIC, eyewear })),
      ...EXTRAS.map((extra) => ({ ...CLASSIC, extra })),
    ]) {
      expect(parseCostume(costumeCode(c), CLASSIC)).toEqual(c);
    }
  });

  it("takes a part a later version added from the session's lot", () => {
    expect(parseCostume("v1:sky.sombrero.shades.cape", fallback)).toEqual({
      shell: "sky",
      hat: "crown",
      eyewear: "shades",
      extra: "coffee",
    });
  });

  it("ignores anything unreadable", () => {
    for (const garbage of [
      "",
      "sky.cap.none.none",
      "v2:sky.cap.none.none",
      "v1:sky.cap.none",
      "v1:sky.cap.none.none.bow",
      "v1:" + "x".repeat(200),
    ]) {
      expect(parseCostume(garbage, fallback)).toBeNull();
    }
  });

  it("wears the person's choice, or else its own lot", () => {
    const id = "session-1";
    expect(resolvedCostume(id, null)).toEqual(seeded(id));
    expect(resolvedCostume(id, "nonsense")).toEqual(seeded(id));
    expect(resolvedCostume(id, costumeCode(costume))).toEqual(costume);
    expect(keptCode(seeded(id), id)).toBeNull();
    expect(keptCode(null, id)).toBeNull();
    const other = randomCostume(seeded(id));
    expect(keptCode(other, id)).toBe(costumeCode(other));
  });

  it("keeps choices by session, dropping sessions no longer running first", () => {
    let kept = withKept({}, "a", "v1:sky.cap.none.none", new Set(["a"]));
    expect(kept).toEqual({ a: "v1:sky.cap.none.none" });
    kept = withKept(kept, "a", null, new Set(["a"]));
    expect(kept).toEqual({});
    const many: Record<string, string> = {};
    for (let i = 0; i < COSTUMES_KEPT; i++) many[`old${i}`] = "x";
    kept = withKept({ ...many, live: "keep" }, "new", "fresh", new Set(["live", "new"]));
    expect(Object.keys(kept).length).toBe(COSTUMES_KEPT);
    expect(kept.live).toBe("keep");
    expect(kept.new).toBe("fresh");
    // Sorted-key order: old0, old1, old10, ... go first.
    expect(kept.old0).toBeUndefined();
    expect(kept.old1).toBeUndefined();
    expect(kept.old2).toBe("x");
  });

  describe("localStorage store", () => {
    afterEach(() => vi.unstubAllGlobals());

    it("round-trips through storage", () => {
      const data = new Map<string, string>();
      vi.stubGlobal("localStorage", {
        getItem: (k: string) => data.get(k) ?? null,
        setItem: (k: string, v: string) => void data.set(k, v),
      });
      const store = localCostumeStore();
      store.set("s1", "v1:sky.cap.none.none");
      expect(store.get("s1")).toBe("v1:sky.cap.none.none");
      expect(JSON.parse(data.get("notchSessionsCostumes")!)).toEqual({ s1: "v1:sky.cap.none.none" });
      store.set("s1", null);
      expect(store.all()).toEqual({});
      data.set("notchSessionsCostumes", '{"a":1,"b":"v1:x.x.x.x"}');
      expect(store.all()).toEqual({ b: "v1:x.x.x.x" });
      data.set("notchSessionsCostumes", "not json");
      expect(store.all()).toEqual({});
    });

    it("keeps nothing, without throwing, when storage throws or is missing", () => {
      vi.stubGlobal("localStorage", {
        getItem: () => {
          throw new Error("blocked");
        },
        setItem: () => {
          throw new Error("blocked");
        },
      });
      const store = localCostumeStore();
      expect(() => store.set("s1", "v1:sky.cap.none.none")).not.toThrow();
      expect(store.get("s1")).toBeNull();
      vi.stubGlobal("localStorage", undefined);
      expect(localCostumeStore().all()).toEqual({});
    });
  });
});

describe("clawd motions", () => {
  it("follows the session's phase", () => {
    const pairs: [Parameters<typeof motionForPhase>[0], ClawdMotion][] = [
      ["generating", "think"],
      ["runningTool", "work"],
      ["background", "slow"],
      ["waitingPermission", "alert"],
      ["waitingInput", "ask"],
      ["finished", "content"],
      ["idle", "rest"],
    ];
    for (const [phase, motion] of pairs) expect(motionForPhase(phase)).toBe(motion);
    expect(motionForPhase("idle", true)).toBe("doze");
    expect(motionForPhase("runningTool", true)).toBe("work");
    expect(motionForSubagent(true)).toBe("work");
    expect(motionForSubagent(false)).toBe("done");
    expect(motionForNotice("finished")).toBe("content");
    expect(motionForNotice("input")).toBe("ask");
    expect(motionForNotice("permission")).toBe("alert");
  });
});

// Every pose a Clawd is ever drawn in: each motion over ten seconds, the
// cheer, and the still poses.
function reachingPoses(): ClawdPose[] {
  const poses = new Map<string, ClawdPose>();
  for (const motion of MOTIONS) {
    for (let step = 0; step < 120; step++) {
      const time = step / 12;
      for (const p of [
        pose(motion, time, 7n),
        pose(motion, time, 7n, { cheer: time }),
        pose(motion, time, 7n, { still: true }),
      ]) {
        poses.set(poseKey(p), p);
      }
    }
  }
  return [...poses.values()];
}

describe("clawd drawing", () => {
  it("fits every costume in every pose, with a color for every letter", () => {
    const costumes: ClawdCostume[] = [
      ...SHELLS.map((shell) => ({ shell, hat: "party", eyewear: "anaglyph", extra: "coffee" }) as ClawdCostume),
      ...HATS.map((hat) => ({ shell: "clay", hat, eyewear: "none", extra: "headphones" }) as ClawdCostume),
      ...EYEWEAR.map((eyewear) => ({ shell: "clay", hat: "topHat", eyewear, extra: "none" }) as ClawdCostume),
      ...EXTRAS.map((extra) => ({ shell: "clay", hat: "chef", eyewear: "none", extra }) as ClawdCostume),
      CLASSIC,
    ];
    const poses = reachingPoses();
    const clipped: string[] = [];
    const unpainted = new Set<string>();
    for (const character of ["clawd", "codex"] as const) {
      for (const c of costumes) {
        for (const p of poses) {
          if (clippedPixels(c, p, character) > 0) clipped.push(`${character} ${costumeCode(c)} ${poseKey(p)}`);
          for (const l of letters(c, p, character)) if (l && !color(l, c, character)) unpainted.add(l);
        }
      }
    }
    expect(clipped).toEqual([]);
    expect([...unpainted]).toEqual([]);
  });

  it("can be dressed with the expanded hero catalog", () => {
    expect(SHELLS.length * HATS.length * EYEWEAR.length * EXTRAS.length).toBe(11 * 13 * 20 * 25);
  });

  const at = (c: ClawdCostume, column: number, row: number) =>
    letters(c, basePose())[(BODY_Y + row) * WIDTH + BODY_X + column];

  it("has two eyes, a body and legs where Claude Code draws them", () => {
    expect(at(CLASSIC, 3, 2)).toBe("E");
    expect(at(CLASSIC, 9, 3)).toBe("E");
    expect(at(CLASSIC, 6, 0)).toBe("C");
    expect(at(CLASSIC, 0, 9)).toBe("c");
  });

  it("covers the eyes with opaque glasses and hero masks", () => {
    for (const eyewear of EYEWEAR) {
      const worn: ClawdCostume = { ...CLASSIC, eyewear };
      const eyes = [
        [3, 2],
        [3, 3],
        [9, 2],
        [9, 3],
      ].map(([x, y]) => at(worn, x, y));
      if (eyewear === "shades" || eyewear === "anaglyph" || eyewear.endsWith("Mask")) expect(eyes).not.toContain("E");
      else expect(eyes.every((e) => e === "E")).toBe(true);
    }
  });

  it("tells hats from shells and shells from each other", () => {
    for (const shell of SHELLS) expect(ACCENT[shell]).not.toBe(shell);
    expect(new Set(SHELLS.map((s) => SHELL_RGB[s].join())).size).toBe(SHELLS.length);
    expect(SHELL_RGB.clay).toEqual([215, 119, 87]);
    expect(color("c", CLASSIC)).toEqual([167, 92, 67]);
    expect(color("A", CLASSIC)).toEqual(SHELL_RGB.sky);
  });

  it("is a 24×22 canvas of opaque or clear pixels", () => {
    const bytes = pixels(CLASSIC, basePose());
    expect(bytes.length).toBe(WIDTH * HEIGHT * 4);
    for (let i = 3; i < bytes.length; i += 4) expect(bytes[i] === 0 || bytes[i] === 255).toBe(true);
    const rows = grid(CLASSIC, basePose());
    expect(rows.length).toBe(HEIGHT);
    expect(rows.every((r) => r.length === WIDTH)).toBe(true);
    expect(rows[BODY_Y][BODY_X]).toBe("#d77757");
    expect(rows[0][0]).toBeNull();
  });

  it("turns a propeller from one step to the next", () => {
    const propeller: ClawdCostume = { ...CLASSIC, hat: "propeller" };
    expect(letters(propeller, basePose({ tick: 0 }))).not.toEqual(letters(propeller, basePose({ tick: 1 })));
  });

  it("draws the classic Clawd as the Mac does", () => {
    // A picture of the stand pose, so a stamp that moves shows up here.
    const art = letters(CLASSIC, basePose())
      .map((l) => l ?? ".")
      .join("")
      .match(/.{24}/g)!
      .slice(10);
    expect(art).toEqual([
      "........................",
      "........................",
      ".....CCCCCCCCCCCCC......",
      ".....CCCCCCCCCCCCC......",
      ".....CCCECCCCCECCC......",
      ".....CCCECCCCCECCC......",
      "...CCCCCCCCCCCCCCCCC....",
      "...CCCCCCCCCCCCCCCCC....",
      ".....CCCCCCCCCCCCC......",
      ".....CCCCCCCCCCCCC......",
      ".....c.c.......c.c......",
      ".....c.c.......c.c......",
    ]);
  });
});

describe("clawd poses", () => {
  const sd = seed("session");
  const over = (motion: ClawdMotion, seconds = 8) =>
    Array.from({ length: Math.ceil(seconds * 12) }, (_, i) => pose(motion, i / 12, sd));

  it("rests: blinks and breathes", () => {
    const rest = over("rest");
    expect(rest.some((p) => p.eyes === "closed")).toBe(true);
    expect(rest.some((p) => p.rise === 1)).toBe(true);
    expect(rest.some((p) => p.rise === 0)).toBe(true);
    expect(rest.filter((p) => p.eyes === "closed").length).toBeLessThanOrEqual(2 * Math.trunc(8 / 3.4 + 1));
    expect(new Set(rest.map(poseKey)).size).toBeGreaterThan(1);
  });

  it("dozes with shut eyes under rising z's", () => {
    const doze = over("doze");
    expect(doze.every((p) => p.eyes === "closed")).toBe(true);
    expect(new Set(doze.map((p) => JSON.stringify(p.bubble))).size).toBe(3);
  });

  it("thinks with busy hands, works with a waddle", () => {
    expect(new Set(over("think").map((p) => p.arms))).toEqual(new Set(["typeLeft", "typeRight"]));
    expect(new Set(over("work").map((p) => p.legs))).toEqual(new Set(["stand", "stepLeft", "stepRight"]));
  });

  it("hops and waves under ! or ?", () => {
    const alert = over("alert");
    expect(alert.every((p) => p.bubble?.kind === "alert")).toBe(true);
    expect(alert.some((p) => p.jump > 0)).toBe(true);
    expect(alert.some((p) => p.arms === "waveUp")).toBe(true);
    expect(alert.every((p) => p.jump === 0 || p.legs === "stand")).toBe(true);
    expect(over("ask").every((p) => p.bubble?.kind === "question")).toBe(true);
  });

  it("smiles when done, and a finished subagent keeps still", () => {
    expect(over("content").every((p) => p.eyes === "happy")).toBe(true);
    expect(new Set(over("done").map(poseKey)).size).toBe(1);
  });

  it("does not blink as one with its neighbour", () => {
    const other = seed("another session");
    const a = Array.from({ length: 240 }, (_, i) => blinks(i / 12, sd));
    const b = Array.from({ length: 240 }, (_, i) => blinks(i / 12, other));
    expect(a).not.toEqual(b);
  });

  it("keeps one still pose per motion under Reduce Motion", () => {
    for (const motion of MOTIONS) {
      const still = Array.from({ length: 24 }, (_, i) => pose(motion, i * 0.25, sd, { cheer: 0.5, still: true }));
      expect(new Set(still.map(poseKey)).size).toBe(1);
      expect(still[0].jump).toBe(0);
      expect(still[0].rise).toBe(0);
    }
    const still = (m: ClawdMotion) => pose(m, 0, sd, { still: true });
    expect(still("alert").bubble?.kind).toBe("alert");
    expect(still("ask").bubble?.kind).toBe("question");
    expect(still("doze").eyes).toBe("closed");
    expect(still("doze").bubble).toEqual({ kind: "sleep", phase: 1 });
    expect(still("content").eyes).toBe("happy");
    expect(still("think").arms).toBe("typeLeft");
  });

  it("cheers with a jump and both arms up, once", () => {
    const cheer = Array.from({ length: Math.ceil(CHEER_LENGTH * 12) }, (_, i) =>
      pose("content", 30, sd, { cheer: i / 12 }),
    );
    expect(cheer.some((p) => p.jump >= 4 && p.arms === "up")).toBe(true);
    expect(cheer.every((p) => p.eyes === "happy")).toBe(true);
    expect(pose("content", 30, sd, { cheer: CHEER_LENGTH })).toEqual(pose("content", 30, sd));
  });

  it("drops bubbles where there is no room, and treats nonsense times as the start", () => {
    expect(pose("alert", 0.2, sd, { bubbles: false }).bubble).toBeNull();
    expect(pose("doze", 1, sd, { bubbles: false }).bubble).toBeNull();
    expect(pose("rest", NaN, sd)).toEqual(pose("rest", 0, sd));
    expect(pose("rest", -5, sd)).toEqual(pose("rest", 0, sd));
  });
});

describe("clawd fitting", () => {
  const boxes: [{ width: number; height: number }, number][] = [
    [{ width: 32, height: 30 }, 2],
    [{ width: 40, height: 36 }, 2],
    [{ width: 18, height: 18 }, 2],
    [{ width: 32, height: 30 }, 1],
    [{ width: 48, height: 42 }, 3],
    [{ width: 130, height: 110 }, 2],
    [{ width: 32, height: 30 }, 1.25],
    [{ width: 18, height: 14 }, 1.5],
  ];

  it.each(boxes)("fits %o at %dx on whole device pixels, centred", (box, scale) => {
    const fit = layout(box, scale);
    const device = fit.pixel * scale;
    expect(device).toBeCloseTo(Math.round(device), 9);
    expect(device).toBeGreaterThanOrEqual(1);
    const fw = FRAME.columns * fit.pixel;
    const fh = FRAME.rows * fit.pixel;
    const left = fit.x + FRAME.left * fit.pixel;
    const top = fit.y + FRAME.top * fit.pixel;
    if (fit.devicePixels > 1 || (box.width * scale) / FRAME.columns >= 1) {
      expect(fw).toBeLessThanOrEqual(box.width + 0.001);
      expect(fh).toBeLessThanOrEqual(box.height + 0.001);
    }
    expect(Math.abs(left + fw / 2 - box.width / 2)).toBeLessThanOrEqual(0.5 / scale + 0.001);
    expect(Math.abs(top + fh / 2 - box.height / 2)).toBeLessThanOrEqual(0.5 / scale + 0.001);
    expect(fit.x * scale).toBeCloseTo(Math.round(fit.x * scale), 9);
    expect(fit.y * scale).toBeCloseTo(Math.round(fit.y * scale), 9);
  });

  it("gives a list row three device pixels a canvas pixel at 2x", () => {
    expect(layout({ width: 32, height: 30 }, 2).pixel).toBe(1.5);
  });

  it("still draws something in an empty or nonsense box", () => {
    expect(layout({ width: 0, height: 0 }, 2).pixel).toBe(0.5);
    expect(layout({ width: 40, height: 40 }, NaN).pixel).toBeGreaterThan(0);
    const odd = layout({ width: NaN, height: Infinity }, 2);
    expect(odd.pixel).toBe(0.5);
    expect(Number.isFinite(odd.x) && Number.isFinite(odd.y)).toBe(true);
  });

  it("stands the frame on the canvas's bottom", () => {
    expect(FRAME.columns).toBe(17);
    expect(FRAME.top + FRAME.rows).toBe(HEIGHT);
  });
});
