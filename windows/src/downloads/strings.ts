// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Downloads section's words, as the Mac says them (NotchFilesStrings,
// Localization.swift shelfActionOpen); Finder becomes File Explorer.

import { t } from "../i18n";

export const S = {
  get title() {
    return t("notchFiles.downloadsTitle");
  },
  get waiting() {
    return t("windows.downloads.newDownloadsAndFileTransfersAppearHereAutomatically");
  },
  get completed() {
    return t("notchFiles.completed");
  },
  get saved() {
    return t("notchFiles.saved");
  },
  get stopped() {
    return t("windows.downloads.stopped");
  },
  get inProgress() {
    return t("notchFiles.inProgress");
  },
  get open() {
    return t("s.diskOpenInFinder");
  },
  get showInExplorer() {
    return t("windows.downloads.showInExplorer");
  },
  get addToFiles() {
    return t("windows.downloads.addToFiles");
  },
  get addedToFiles() {
    return t("windows.downloads.addedToFiles");
  },
  get addFolder() {
    return t("notchFiles.addFolder");
  },
  get addFolderTitle() {
    return t("windows.downloads.addFolderTitle");
  },
};
