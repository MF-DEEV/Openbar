// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// English copy for the Tools section, from the Mac (Localization.swift,
// QuickToggleStrings.swift, ScreenshotStrings.swift), with Windows' own
// names where the Mac names a Mac thing (Recycle Bin, not Trash).

import { t, tf } from "../i18n";

export const S = {
  get customize() {
    return t("notch.customizeTools");
  },
  get done() {
    return t("screenshot.done");
  },
  get editHint() {
    return t("s.launcherEditHint");
  },
  get addSection() {
    return t("s.launcherAddSection");
  },
  get empty() {
    return t("s.launcherEmptyState");
  },
  get hideItem() {
    return t("s.panelHideItem");
  },
  showItem: (title: string) => tf("windows.tools.showItem", title),
  get options() {
    return t("s.keepAwakeOptions");
  },
  get close() {
    return t("s.menuClose");
  },
  get back() {
    return t("s.actionBack");
  },

  get keepAwake() {
    return t("s.keepAwakeTitle");
  },
  get toggles() {
    return t("quickToggles.pageTitle");
  },
  get micMute() {
    return t("s.micMuteName");
  },
  get micUnmute() {
    return t("s.micUnmuteName");
  },
  get screenOCR() {
    return t("s.ocrName");
  },
  get colorPicker() {
    return t("s.colorPickerName");
  },
  get clipboard() {
    return t("clipboard.title");
  },
  get screenshot() {
    return t("screenshot.editorTitle");
  },
  get screenRecorder() {
    return t("recorder.pageTitle");
  },
  get recordingFile() {
    return t("recorder.fileNamePrefix");
  },
  get recordingStop() {
    return t("recorder.stopButton");
  },
  get recordingSaved() {
    return t("windows.tools.recordingSaved");
  },
  get recordingNoSound() {
    return t("windows.tools.recordingNoSound");
  },
  get recordingFailed() {
    return t("recorder.recordFailed");
  },
  get scratchpad() {
    return t("scratchpad.pageTitle");
  },

  get colorFormat() {
    return t("s.colorPickerFormatLabel");
  },
  get bareHex() {
    return t("s.colorPickerBareHexToggle");
  },

  // Notices
  get screenshotCopied() {
    return t("screenshot.copiedHUD");
  },
  savedTo: (folder: string) => tf("screenshot.savedHUDFormat", folder),
  savedAndCopied: (folder: string) => tf("screenshot.savedAndCopiedHUDFormat", folder),
  get captureFailed() {
    return t("screenshot.captureFailed");
  },
  get textCopied() {
    return t("s.ocrCopied");
  },
  get noText() {
    return t("s.ocrNoText");
  },
  get noLanguage() {
    return t("windows.tools.noLanguage");
  },
  get noLanguageDetail() {
    return t("windows.tools.noLanguageDetail");
  },
  get colorCopied() {
    return t("windows.tools.colorCopied");
  },

  // Quick toggles
  get darkMode() {
    return t("windows.tools.darkMode");
  },
  get hiddenFiles() {
    return t("windows.tools.hiddenFiles");
  },
  get desktopIcons() {
    return t("windows.tools.desktopIcons");
  },
  get darkModeToDark() {
    return t("quickToggles.darkModeToDark");
  },
  get darkModeToLight() {
    return t("quickToggles.darkModeToLight");
  },
  get darkModeCaption() {
    return t("quickToggles.darkModeCaption");
  },
  get micMuteCaption() {
    return t("windows.tools.micMuteCaption");
  },
  get emptyTrashTitle() {
    return t("windows.tools.emptyTrashTitle");
  },
  get emptyTrashCaption() {
    return t("windows.tools.emptyTrashCaption");
  },
  get emptyTrashConfirmTitle() {
    return t("windows.tools.emptyTrashConfirmTitle");
  },
  get emptyTrashConfirmMessage() {
    return t("windows.tools.emptyTrashConfirmMessage");
  },
  get emptyTrashConfirmButton() {
    return t("windows.tools.emptyTrashConfirmButton");
  },
  get cancel() {
    return t("screenshot.cancel");
  },
  get ejectTitle() {
    return t("quickToggles.ejectTitle");
  },
  get ejectCaption() {
    return t("quickToggles.ejectCaption");
  },
  get ejecting() {
    return t("s.diskEjecting");
  },
  get ejectFailed() {
    return t("s.diskEjectFailed");
  },
  get noExternalDisk() {
    return t("s.diskNoExternal");
  },
  get hiddenFilesShow() {
    return t("quickToggles.hiddenFilesShow");
  },
  get hiddenFilesHide() {
    return t("quickToggles.hiddenFilesHide");
  },
  get desktopIconsShow() {
    return t("quickToggles.desktopIconsShow");
  },
  get desktopIconsHide() {
    return t("quickToggles.desktopIconsHide");
  },
  get explorerCaption() {
    return t("windows.tools.explorerCaption");
  },
  get desktopCaption() {
    return t("windows.tools.desktopCaption");
  },
  get lockScreenTitle() {
    return t("quickToggles.lockScreenTitle");
  },
  get lockScreenCaption() {
    return t("windows.tools.lockScreenCaption");
  },
  get displayOffTitle() {
    return t("quickToggles.displayOffTitle");
  },
  get displayOffCaption() {
    return t("windows.tools.displayOffCaption");
  },
  get screenSaverTitle() {
    return t("quickToggles.screenSaverTitle");
  },
  get screenSaverCaption() {
    return t("quickToggles.screenSaverCaption");
  },
  get actionFailed() {
    return t("quickToggles.actionFailed");
  },
} as const;
