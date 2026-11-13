// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The first-run window's words: the Mac's (OnboardingStrings.swift), with the
// Windows overrides where the Mac names its notch or menu bar.

import { t } from "../i18n";

export const O = {
  get back() {
    return t("s.obBack");
  },
  get continue() {
    return t("s.obContinue");
  },
  get start() {
    return t("s.obStart");
  },
  get welcomeBody() {
    return t("onboarding.welcomeBody");
  },
  get setupAgentsName() {
    return t("onboarding.setupAgentsName");
  },
  get agentsTitle() {
    return t("onboarding.agentsTitle");
  },
  get agentsBody() {
    return t("onboarding.agentsBody");
  },
  get agentConnected() {
    return t("onboarding.agentConnected");
  },
  get agentConnecting() {
    return t("onboarding.agentConnecting");
  },
  get agentReady() {
    return t("onboarding.agentReady");
  },
  get agentNotInstalled() {
    return t("onboarding.agentNotInstalled");
  },
  get agentFailed() {
    return t("onboarding.agentFailed");
  },
  get retry() {
    return t("onboarding.retry");
  },
  get tryTitle() {
    return t("onboarding.tryTitle");
  },
  get tryBody() {
    return t("onboarding.tryBody");
  },
  get tryButton() {
    return t("onboarding.tryButton");
  },
  get tryAgain() {
    return t("onboarding.tryAgain");
  },
  get tryDenied() {
    return t("onboarding.tryDenied");
  },
  get tryResult() {
    return t("onboarding.tryResult");
  },
  get allowed() {
    return t("onboarding.allowed");
  },
  get keyAllow() {
    return t("onboarding.keyAllow");
  },
  get keyDeny() {
    return t("onboarding.keyDeny");
  },
  get keyAlways() {
    return t("onboarding.keyAlways");
  },
  get anyApp() {
    return t("onboarding.anyApp");
  },
  get doneTitle() {
    return t("onboarding.doneTitle");
  },
  get skip() {
    return t("onboarding.skip");
  },
  get welcomeTitle() {
    return t("windows.onboarding.welcomeTitle");
  },
  get welcomeToolkit() {
    return t("windows.onboarding.welcomeToolkit");
  },
  get doneBody() {
    return t("windows.onboarding.doneBody");
  },
  get doneTray() {
    return t("windows.onboarding.doneTray");
  },
  get doneIsland() {
    return t("windows.onboarding.doneIsland");
  },
};
