// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

/** A waiting ask's card in the session page, in the page's own coordinates
 *  (its scroll offset included). */
export interface CardSpan {
  top: number;
  bottom: number;
  /** The bottom of the card's last row of buttons: Allow, Deny, Submit. */
  buttonsBottom: number;
}

/** Room left under the card when the page has to scroll to it. */
const GAP = 6;

/**
 * The scroll offset that shows a waiting ask on the island's session page: the
 * top of the page while the whole card fits there, as on the Mac; else just far
 * enough down for the whole card; and when the card is taller than the page,
 * far enough for its buttons, keeping as much of the card above them as fits.
 */
export function revealOffset(card: CardSpan, viewport: number): number {
  const whole = card.bottom + GAP - viewport;
  const target = whole <= card.top ? whole : card.buttonsBottom + GAP - viewport;
  return Math.max(0, Math.round(target));
}

/**
 * Brings a waiting ask's card into sight in the session page that scrolls it,
 * and keeps it there while the island settles its size or the card its
 * content, until the person scrolls the page themselves. Anywhere else (the
 * enlarged view lays the card out on its own) it does nothing.
 */
export function keepCardInSight(card: HTMLElement): () => void {
  const page = card.parentElement;
  if (!page?.classList.contains("s-scroll") || page.classList.contains("is-large")) return () => {};
  let held = true;
  const place = () => {
    if (!held || page.clientHeight <= 0) return;
    // The island scales its content to fit while it opens, and on screen
    // everything is that much smaller: measure in the page's own pixels.
    const box = page.getBoundingClientRect();
    const scale = box.height > 0 && page.offsetHeight > 0 ? box.height / page.offsetHeight : 1;
    const at = (y: number) => (y - box.top) / scale + page.scrollTop;
    const own = card.getBoundingClientRect();
    const rows = card.querySelectorAll<HTMLElement>(".s-card-buttons");
    const buttons = rows.length ? rows[rows.length - 1]!.getBoundingClientRect() : own;
    page.scrollTop = revealOffset(
      { top: at(own.top), bottom: at(own.bottom), buttonsBottom: at(buttons.bottom) },
      page.clientHeight,
    );
  };
  const release = () => {
    held = false;
  };
  place();
  const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(place);
  observer?.observe(page);
  observer?.observe(card);
  page.addEventListener("wheel", release, { passive: true });
  page.addEventListener("touchstart", release, { passive: true });
  page.addEventListener("keydown", release);
  return () => {
    observer?.disconnect();
    page.removeEventListener("wheel", release);
    page.removeEventListener("touchstart", release);
    page.removeEventListener("keydown", release);
  };
}
