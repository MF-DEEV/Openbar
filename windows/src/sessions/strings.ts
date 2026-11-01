// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The Sessions section's strings. On the Mac they are English in every
// language (NotchClaudeSessionStrings.swift); Mac-only wording is swapped
// for Windows here.

import { t, tf } from "../i18n";

export const S = {
  get title() {
    return t("notchClaudeSessions.title");
  },
  get emptyTitle() {
    return t("notchClaudeSessions.emptyTitle");
  },
  get emptyDetail() {
    return t("windows.sessions.emptyDetail");
  },
  get generating() {
    return t("notchLockScreen.working");
  },
  get runningTool() {
    return t("notchClaudeSessions.runningTool");
  },
  get waitingPermission() {
    return t("notchClaudeSessions.permissionNotice");
  },
  get waitingInput() {
    return t("notchClaudeSessions.waitingInput");
  },
  get background() {
    return t("notchClaudeSessions.background");
  },
  get finished() {
    return t("feedback.done");
  },
  get idle() {
    return t("notchAgents.idle");
  },
  get allow() {
    return t("notchClaudeSessions.allow");
  },
  get alwaysAllow() {
    return t("notchClaudeSessions.alwaysAllow");
  },
  get deny() {
    return t("notchClaudeSessions.deny");
  },
  get plan() {
    return t("notchClaudeSessions.plan");
  },
  get approvePlan() {
    return t("notchClaudeSessions.approvePlan");
  },
  get keepPlanning() {
    return t("notchClaudeSessions.keepPlanning");
  },
  get answerInTerminal() {
    return t("notchClaudeSessions.answerInTerminal");
  },
  get question() {
    return t("notchClaudeSessions.question");
  },
  get codexQuestion() {
    return t("notchClaudeSessions.questionCodex");
  },
  get pickAny() {
    return t("notchClaudeSessions.pickAny");
  },
  get ownAnswer() {
    return t("notchClaudeSessions.ownAnswer");
  },
  get sendAnswer() {
    return t("commandBar.kindAnswer");
  },
  get skip() {
    return t("notchClaudeSessions.skip");
  },
  get questionHint() {
    return t("windows.sessions.questionHint");
  },
  get codexQuestionHint() {
    return t("windows.sessions.codexQuestionHint");
  },
  get open() {
    return t("appUpdates.openApp");
  },
  get enlarge() {
    return t("notchClaudeSessions.enlarge");
  },
  get shrink() {
    return t("notchClaudeSessions.shrink");
  },
  leftRunning: (n: number) => tf("notchClaudeSessions.leftRunningFormat", n),
  get rename() {
    return t("finderRename.shortcutLabel");
  },
  get renamePlaceholder() {
    return t("notchClaudeSessions.renamePlaceholder");
  },
  get conversation() {
    return t("notchClaudeSessions.conversation");
  },
  get details() {
    return t("windows.sessions.details");
  },
  get showDetails() {
    return t("s.homebrewOperationShowDetails");
  },
  get hideDetails() {
    return t("s.homebrewOperationHideDetails");
  },
  get you() {
    return t("windows.sessions.you");
  },
  get enablePreviews() {
    return t("windows.sessions.enablePreviews");
  },
  get previewsFailed() {
    return t("windows.sessions.previewsFailed");
  },
  get noMessages() {
    return t("notchClaudeSessions.noMessages");
  },
  get answer() {
    return t("notchClaudeSessions.answer");
  },
  showAll: (n: number) => tf("notchClaudeSessions.showAllFormat", n),
  get showLess() {
    return t("notchClaudeSessions.showLess");
  },
  get smallerText() {
    return t("notchClaudeSessions.smallerText");
  },
  get largerText() {
    return t("notchClaudeSessions.largerText");
  },
  get sendShortcut() {
    return t("windows.sessions.sendShortcut");
  },
  get pasteImages() {
    return t("windows.sessions.pasteImages");
  },
  get removeImage() {
    return t("notchClaudeSessions.removeImage");
  },
  get send() {
    return t("notchClaudeSessions.send");
  },
  get replyPlaceholder() {
    return t("notchClaudeSessions.replyPlaceholder");
  },
  get replyPlaceholderCodex() {
    return t("notchClaudeSessions.replyPlaceholderCodex");
  },
  get replyUnavailable() {
    return t("windows.sessions.replyUnavailable");
  },
  get replyWhileWaiting() {
    return t("notchClaudeSessions.replyWhileWaiting");
  },
  get replyWhileDialog() {
    return t("notchClaudeSessions.replyWhileDialog");
  },
  tasks: (done: number, total: number) => tf("notchClaudeSessions.tasksFormat", done, total),
  get subagents() {
    return t("notchClaudeSessions.subagents");
  },
  moreSubagents: (n: number) => tf("notchClaudeSessions.moreSubagentsFormat", n),
  get recentTools() {
    return t("clipboard.recent");
  },
  get lastReply() {
    return t("notchClaudeSessions.lastReply");
  },
  get previewsOff() {
    return t("notchClaudeSessions.previewsOff");
  },
  get previewsOffCodex() {
    return t("notchClaudeSessions.previewsOffCodex");
  },
  get permissionNotice() {
    return t("notchClaudeSessions.permissionNotice");
  },
  get inputNotice() {
    return t("notchClaudeSessions.inputNotice");
  },
  get finishedNotice() {
    return t("feedback.done");
  },
  finishedAfter: (d: string) => tf("notchClaudeSessions.finishedAfterFormat", d),
  get shortcutsHint() {
    return t("windows.sessions.shortcutsHint");
  },
  get sendFailed() {
    return t("notchClaudeSessions.sendFailed");
  },
  get sendWhileWaiting() {
    return t("notchClaudeSessions.sendWhileWaiting");
  },
  get backgroundMark() {
    return t("windows.sessions.backgroundMark");
  },
  get backgroundHelp() {
    return t("windows.sessions.backgroundHelp");
  },
  get backgroundStopped() {
    return t("windows.sessions.backgroundStopped");
  },
  get backgroundFailed() {
    return t("windows.sessions.backgroundFailed");
  },
  get backgroundNeedsReply() {
    return t("windows.sessions.backgroundNeedsReply");
  },
  get logs() {
    return t("windows.sessions.logs");
  },
  get logsTitle() {
    return t("windows.sessions.logsTitle");
  },
  get logsEmpty() {
    return t("windows.sessions.logsEmpty");
  },
  get logsGone() {
    return t("windows.sessions.logsGone");
  },
  get stopSession() {
    return t("windows.sessions.stopSession");
  },
  get stopConfirm() {
    return t("windows.sessions.stopConfirm");
  },
  get removeSession() {
    return t("windows.sessions.removeSession");
  },
  get removeConfirm() {
    return t("windows.sessions.removeConfirm");
  },
  get keepSession() {
    return t("windows.sessions.keepSession");
  },
  get replyEnded() {
    return t("windows.sessions.replyEnded");
  },
  get commandFailed() {
    return t("windows.sessions.commandFailed");
  },
  get newSession() {
    return t("windows.sessions.newSession");
  },
  get browseFolder() {
    return t("windows.sessions.browseFolder");
  },
  get noRecentFolders() {
    return t("windows.sessions.noRecentFolders");
  },
  get firstPrompt() {
    return t("windows.sessions.firstPrompt");
  },
  get runInBackground() {
    return t("windows.sessions.runInBackground");
  },
  get runInTerminal() {
    return t("windows.sessions.runInTerminal");
  },
  get backgroundOffHelp() {
    return t("windows.sessions.backgroundOffHelp");
  },
  get startSession() {
    return t("windows.sessions.startSession");
  },
  get folderNotTrusted() {
    return t("windows.sessions.folderNotTrusted");
  },
  get agentPicker() {
    return t("windows.sessions.agentPicker");
  },
  get modelPicker() {
    return t("windows.sessions.modelPicker");
  },
  get defaultModel() {
    return t("windows.sessions.defaultModel");
  },
  get codexNeedsPrompt() {
    return t("windows.sessions.codexNeedsPrompt");
  },
  get codexNotStarted() {
    return t("windows.sessions.codexNotStarted");
  },
  get codexFailed() {
    return t("windows.sessions.codexFailed");
  },
  /** The agents' names, as Settings words them. */
  get claudeCode() {
    return t("windows.settings.claudeCode");
  },
  get codex() {
    return t("windows.settings.codex");
  },
  get openFailed() {
    return t("notchClaudeSessions.openFailed");
  },
  get back() {
    return t("radialMenu.backButton");
  },
} as const;
