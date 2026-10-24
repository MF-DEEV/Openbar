// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Where a session's Clawd goes. Until the Clawd port lands this is the phase
// badge the Mac shows with buddies off; the sizes are the Clawd's.

import { PhaseBadge } from "./Avatar";
import { dozing } from "./format";
import type { Session } from "./types";

const SIZES = {
  row: { w: 32, h: 30, badge: 26 },
  page: { w: 40, h: 36, badge: 30 },
  large: { w: 48, h: 42, badge: 34 },
};

export function SessionAvatar(props: { session: Session; now: number; lookedAt?: number; size: keyof typeof SIZES }) {
  const s = SIZES[props.size];
  const asleep = dozing(props.session, props.now, props.lookedAt);
  return (
    <span className="s-avatar" style={{ width: s.w, height: s.h, opacity: asleep ? 0.65 : 1 }}>
      <PhaseBadge phase={props.session.phase} size={s.badge} />
    </span>
  );
}
