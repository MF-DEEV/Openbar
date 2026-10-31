// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// English copy for the Scratchpad section (ScratchpadStrings.swift, enUS).

import { t, tf } from "../i18n";

export const S = {
  get pageTitle() {
    return t("scratchpad.pageTitle");
  },
  get placeholder() {
    return t("scratchpad.placeholder");
  },
  get copyAll() {
    return t("scratchpad.copyAll");
  },
  get copied() {
    return t("scratchpad.copied");
  },
  get exportAction() {
    return t("scratchpad.exportAction");
  },
  get exportFailed() {
    return t("scratchpad.exportFailed");
  },
  get loadFailed() {
    return t("scratchpad.loadFailed");
  },
  get saveFailed() {
    return t("scratchpad.saveFailed");
  },
  get clearAction() {
    return t("scratchpad.clearAction");
  },
  get newPad() {
    return t("scratchpad.newPad");
  },
  get padActions() {
    return t("scratchpad.padActions");
  },
  get renamePad() {
    return t("scratchpad.renamePad");
  },
  get closePad() {
    return t("scratchpad.closePad");
  },
  get saveName() {
    return t("scratchpad.saveName");
  },
  get cancel() {
    return t("scratchpad.cancel");
  },
  get name() {
    return t("notchEditor.buttonName");
  },
  deletePad: (name: string) => tf("scratchpad.deletePadMessageFormat", name),
  padLimit: (count: number) => tf("scratchpad.padLimitFormat", count),
  get previewFormatting() {
    return t("scratchpad.previewFormatting");
  },
  get editText() {
    return t("scratchpad.editText");
  },
  get taskDone() {
    return t("scratchpad.taskDone");
  },
  get taskNotDone() {
    return t("scratchpad.taskNotDone");
  },
} as const;
