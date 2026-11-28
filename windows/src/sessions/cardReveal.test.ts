// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import { afterEach, describe, expect, it, vi } from "vitest";
import { keepCardInSight, revealOffset } from "./cardReveal";

describe("revealOffset", () => {
  it("keeps the page at the top while the whole card fits there", () => {
    expect(revealOffset({ top: 50, bottom: 120, buttonsBottom: 100 }, 140)).toBe(0);
  });

  it("scrolls just far enough down for a card below the first screen", () => {
    // A background session's page: header, then a card ending at 240 in a 140 px page.
    expect(revealOffset({ top: 120, bottom: 240, buttonsBottom: 215 }, 140)).toBe(106);
  });

  it("shows the buttons of a card taller than the page", () => {
    // The island's report: 150 px card in a 140 px page, its hint below the buttons.
    const offset = revealOffset({ top: 55, bottom: 205, buttonsBottom: 180 }, 140);
    expect(offset).toBe(46);
    expect(180 - offset).toBeLessThanOrEqual(140);
  });

  it("never scrolls above the top", () => {
    expect(revealOffset({ top: 0, bottom: 10, buttonsBottom: 8 }, 500)).toBe(0);
  });
});

/** A page that scrolls a card, laid out as the island draws it: the page's top
 *  at 200 px on screen, the card at `cardTop` in the page's content, all of it
 *  drawn at `scale` (the island scales its content while it opens). */
function fakePage(
  classes: string[],
  clientHeight: number,
  cardTop: number,
  cardHeight: number,
  buttonsAt: number,
  scale = 1,
) {
  const listeners = new Map<string, () => void>();
  const page = {
    scrollTop: 0,
    clientHeight,
    get offsetHeight() {
      return page.clientHeight;
    },
    classList: { contains: (name: string) => classes.includes(name) },
    getBoundingClientRect: () => ({ top: 200, height: page.clientHeight * scale }),
    addEventListener: (type: string, fn: () => void) => listeners.set(type, fn),
    removeEventListener: (type: string) => listeners.delete(type),
  };
  const onScreen = (at: number) => 200 + (at - page.scrollTop) * scale;
  const buttons = { getBoundingClientRect: () => ({ bottom: onScreen(buttonsAt) }) };
  const card = {
    parentElement: page,
    getBoundingClientRect: () => ({ top: onScreen(cardTop), bottom: onScreen(cardTop + cardHeight) }),
    querySelectorAll: () => [buttons],
  };
  return { page, card: card as unknown as HTMLElement, listeners };
}

describe("keepCardInSight", () => {
  const observers: { fire: () => void; observed: number; disconnected: boolean }[] = [];
  afterEach(() => {
    observers.length = 0;
    vi.unstubAllGlobals();
  });
  const stubObserver = () =>
    vi.stubGlobal(
      "ResizeObserver",
      class {
        record = { fire: () => {}, observed: 0, disconnected: false };
        constructor(callback: () => void) {
          this.record.fire = callback;
          observers.push(this.record);
        }
        observe() {
          this.record.observed += 1;
        }
        disconnect() {
          this.record.disconnected = true;
        }
      },
    );

  it("brings the buttons into sight when the card appears below a page scrolled to its end", () => {
    stubObserver();
    const { page, card } = fakePage(["s-scroll"], 140, 55, 150, 180);
    page.scrollTop = 400;
    const stop = keepCardInSight(card);
    expect(page.scrollTop).toBe(46);
    stop();
    expect(observers[0]!.disconnected).toBe(true);
  });

  it("places the card again as the island grows, until the person scrolls", () => {
    stubObserver();
    const { page, card, listeners } = fakePage(["s-scroll"], 0, 55, 150, 180);
    keepCardInSight(card);
    // Still opening: nothing to measure against yet.
    expect(page.scrollTop).toBe(0);
    page.clientHeight = 140;
    observers[0]!.fire();
    expect(page.scrollTop).toBe(46);
    page.clientHeight = 400;
    observers[0]!.fire();
    expect(page.scrollTop).toBe(0);
    listeners.get("wheel")!();
    page.clientHeight = 140;
    observers[0]!.fire();
    expect(page.scrollTop).toBe(0);
  });

  it("measures in the page's own pixels while the island is scaled", () => {
    stubObserver();
    for (const scale of [0.8, 0.93]) {
      const { page, card } = fakePage(["s-scroll"], 140, 55, 150, 180, scale);
      page.scrollTop = 400;
      keepCardInSight(card)();
      expect(page.scrollTop).toBe(46);
    }
  });

  it("leaves the enlarged view and other parents alone", () => {
    stubObserver();
    for (const classes of [["s-scroll", "is-large"], ["s-section"]]) {
      const { page, card } = fakePage(classes, 140, 55, 150, 180);
      page.scrollTop = 400;
      keepCardInSight(card)();
      expect(page.scrollTop).toBe(400);
    }
    expect(observers).toHaveLength(0);
  });

  it("still places the card once without ResizeObserver", () => {
    const { page, card } = fakePage(["s-scroll"], 140, 55, 150, 180);
    page.scrollTop = 400;
    keepCardInSight(card)();
    expect(page.scrollTop).toBe(46);
  });
});
