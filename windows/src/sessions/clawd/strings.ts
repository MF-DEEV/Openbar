// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The wardrobe's English words, from NotchClaudeSessionStrings.swift. They
// move to the generated `notchClaudeSessions` table once the locale pipeline
// (spec 06-i18n) lands; until then the Wardrobe takes overrides by prop.

import { t } from "../../i18n";

import type { ClawdEyewear, ClawdExtra, ClawdHat, ClawdShell } from "./costume";

export const clawdStrings = {
  get wardrobe() {
    return t("notchClaudeSessions.wardrobe");
  },
  get dressUp() {
    return t("notchClaudeSessions.dressUp");
  },
  get dressUpFormat() {
    return t("notchClaudeSessions.dressUpFormat");
  },
  get surpriseMe() {
    return t("notchClaudeSessions.surpriseMe");
  },
  get ownOutfit() {
    return t("notchClaudeSessions.ownOutfit");
  },
  get goToSession() {
    return t("notchClaudeSessions.goToSession");
  },
  get shellTitle() {
    return t("notchClaudeSessions.shellTitle");
  },
  get hatTitle() {
    return t("notchClaudeSessions.hatTitle");
  },
  get eyewearTitle() {
    return t("notchClaudeSessions.eyewearTitle");
  },
  get extrasTitle() {
    return t("notchClaudeSessions.extrasTitle");
  },
  get nothing() {
    return t("recorder.pointerSmoothingOff");
  },
  shells: {
    get clay() {
      return t("notchClaudeSessions.shells.clay");
    },
    get coral() {
      return t("notchClaudeSessions.shells.coral");
    },
    get amber() {
      return t("notchClaudeSessions.shells.amber");
    },
    get moss() {
      return t("notchClaudeSessions.shells.moss");
    },
    get mint() {
      return t("notchMascot.mint");
    },
    get teal() {
      return t("notchClaudeSessions.shells.teal");
    },
    get sky() {
      return t("notchMascot.sky");
    },
    get iris() {
      return t("notchClaudeSessions.shells.iris");
    },
    get orchid() {
      return t("notchClaudeSessions.shells.orchid");
    },
    get rose() {
      return t("notchMascot.rose");
    },
    get midnight() {
      return t("notchClaudeSessions.shells.midnight");
    },
  } as Record<ClawdShell, string>,
  hats: {
    get beanie() {
      return t("notchClaudeSessions.hats.beanie");
    },
    get cap() {
      return t("notchClaudeSessions.hats.cap");
    },
    get beret() {
      return t("notchClaudeSessions.hats.beret");
    },
    get topHat() {
      return t("notchClaudeSessions.hats.topHat");
    },
    get crown() {
      return t("notchClaudeSessions.hats.crown");
    },
    get party() {
      return t("notchClaudeSessions.hats.party");
    },
    get propeller() {
      return t("notchClaudeSessions.hats.propeller");
    },
    get chef() {
      return t("notchClaudeSessions.hats.chef");
    },
    get doomHood() {
      return t("notchClaudeSessions.hats.doomHood");
    },
    get thorHelmet() {
      return t("notchClaudeSessions.hats.thorHelmet");
    },
    get lokiHorns() {
      return t("notchClaudeSessions.hats.lokiHorns");
    },
    get wonderTiara() {
      return t("notchClaudeSessions.hats.wonderTiara");
    },
  } as Record<Exclude<ClawdHat, "none">, string>,
  eyewears: {
    get glasses() {
      return t("notchClaudeSessions.eyewears.glasses");
    },
    get shades() {
      return t("notchClaudeSessions.eyewears.shades");
    },
    get round() {
      return t("notchClaudeSessions.eyewears.round");
    },
    get monocle() {
      return t("notchClaudeSessions.eyewears.monocle");
    },
    get anaglyph() {
      return t("notchClaudeSessions.eyewears.anaglyph");
    },
    get batmanMask() {
      return t("notchClaudeSessions.eyewears.batmanMask");
    },
    get ironManMask() {
      return t("notchClaudeSessions.eyewears.ironManMask");
    },
    get spiderManMask() {
      return t("notchClaudeSessions.eyewears.spiderManMask");
    },
    get doomMask() {
      return t("notchClaudeSessions.eyewears.doomMask");
    },
    get wolverineMask() {
      return t("notchClaudeSessions.eyewears.wolverineMask");
    },
    get flashMask() {
      return t("notchClaudeSessions.eyewears.flashMask");
    },
    get captainMask() {
      return t("notchClaudeSessions.eyewears.captainMask");
    },
    get deadpoolMask() {
      return t("notchClaudeSessions.eyewears.deadpoolMask");
    },
    get venomMask() {
      return t("notchClaudeSessions.eyewears.venomMask");
    },
    get blackPantherMask() {
      return t("notchClaudeSessions.eyewears.blackPantherMask");
    },
    get daredevilMask() {
      return t("notchClaudeSessions.eyewears.daredevilMask");
    },
    get starLordMask() {
      return t("notchClaudeSessions.eyewears.starLordMask");
    },
    get greenLanternMask() {
      return t("notchClaudeSessions.eyewears.greenLanternMask");
    },
    get robinMask() {
      return t("notchClaudeSessions.eyewears.robinMask");
    },
  } as Record<Exclude<ClawdEyewear, "none">, string>,
  extras: {
    get bow() {
      return t("notchClaudeSessions.extras.bow");
    },
    get scarf() {
      return t("notchClaudeSessions.extras.scarf");
    },
    get bowTie() {
      return t("notchClaudeSessions.extras.bowTie");
    },
    get flower() {
      return t("notchClaudeSessions.extras.flower");
    },
    get headphones() {
      return t("notchClaudeSessions.extras.headphones");
    },
    get coffee() {
      return t("s.keepAwakeActiveIconCoffee");
    },
    get doomCape() {
      return t("notchClaudeSessions.extras.doomCape");
    },
    get strangeCape() {
      return t("notchClaudeSessions.extras.strangeCape");
    },
    get thorCape() {
      return t("notchClaudeSessions.extras.thorCape");
    },
    get supermanCape() {
      return t("notchClaudeSessions.extras.supermanCape");
    },
    get batmanCape() {
      return t("notchClaudeSessions.extras.batmanCape");
    },
    get captainShield() {
      return t("notchClaudeSessions.extras.captainShield");
    },
    get mjolnir() {
      return t("notchClaudeSessions.extras.mjolnir");
    },
    get aquamanTrident() {
      return t("notchClaudeSessions.extras.aquamanTrident");
    },
    get lanternRing() {
      return t("notchClaudeSessions.extras.lanternRing");
    },
    get wolverineClaws() {
      return t("notchClaudeSessions.extras.wolverineClaws");
    },
    get batarang() {
      return t("notchClaudeSessions.extras.batarang");
    },
    get hawkeyeBow() {
      return t("notchClaudeSessions.extras.hawkeyeBow");
    },
    get strangeSpell() {
      return t("notchClaudeSessions.extras.strangeSpell");
    },
    get webShooter() {
      return t("notchClaudeSessions.extras.webShooter");
    },
    get ironReactor() {
      return t("notchClaudeSessions.extras.ironReactor");
    },
    get supermanEmblem() {
      return t("notchClaudeSessions.extras.supermanEmblem");
    },
    get flashEmblem() {
      return t("notchClaudeSessions.extras.flashEmblem");
    },
    get lanternEmblem() {
      return t("notchClaudeSessions.extras.lanternEmblem");
    },
  } as Record<Exclude<ClawdExtra, "none">, string>,
};

export type ClawdStrings = typeof clawdStrings;

/** "Change vorsaint’s outfit": the Clawd button's label names its session. */
export function dressUpLabel(name: string, strings: ClawdStrings = clawdStrings): string {
  return strings.dressUpFormat.replace("%@", name);
}
