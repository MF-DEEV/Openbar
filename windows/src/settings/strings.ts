// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import { t, tf } from "../i18n";

// The settings window's words, in English for now. Where the Mac has the
// same setting its wording is kept (NotchStrings, NotchEditorStrings,
// NotchClaudeSessionStrings); the rest is Windows' own.

export const T = {
  // NotchMascotStrings; "your Mac" reads "your PC".
  mascot: {
    get title() {
      return t("notchMascot.title");
    },
    get hint() {
      return t("notchMascot.hint");
    },
    get switchCaption() {
      return t("windows.settings.mascot.switchCaption");
    },
    get livesInIsland() {
      return t("notchMascot.livesInIsland");
    },
    get turnOnIsland() {
      return t("windows.settings.mascot.turnOnIsland");
    },
    get appearance() {
      return t("notchMascot.appearance");
    },
    get behavior() {
      return t("notchEditor.behavior");
    },
    get style() {
      return t("notchMascot.style");
    },
    styles: {
      get minimal() {
        return t("notchMascot.minimal");
      },
      get robot() {
        return t("notchMascot.robot");
      },
    },
    get shape() {
      return t("notchMascot.shape");
    },
    shapes: {
      get ball() {
        return t("notchMascot.ball");
      },
      get egg() {
        return t("notchMascot.egg");
      },
      get squircle() {
        return t("notchMascot.squircle");
      },
      get pill() {
        return t("notchMascot.pill");
      },
    },
    get color() {
      return t("notchMascot.color");
    },
    palettes: {
      get pearl() {
        return t("notchMascot.pearl");
      },
      get lemon() {
        return t("notchMascot.lemon");
      },
      get peach() {
        return t("notchMascot.peach");
      },
      get rose() {
        return t("notchMascot.rose");
      },
      get lilac() {
        return t("notchMascot.lilac");
      },
      get sky() {
        return t("notchMascot.sky");
      },
      get mint() {
        return t("notchMascot.mint");
      },
    },
    get side() {
      return t("windows.settings.mascot.side");
    },
    sides: {
      get left() {
        return t("notch.quickAccessLeft");
      },
      get right() {
        return t("notch.quickAccessRight");
      },
    },
    get visits() {
      return t("notchMascot.visits");
    },
    get visitsHint() {
      return t("notchMascot.visitsHint");
    },
    get frequency() {
      return t("notchMascot.frequency");
    },
    frequencies: {
      get rare() {
        return t("notchMascot.rare");
      },
      get normal() {
        return t("notchMascot.normal");
      },
      get frequent() {
        return t("notchMascot.frequent");
      },
    },
    get reactions() {
      return t("notchMascot.reactions");
    },
    get reactionsHint() {
      return t("notchMascot.reactionsHint");
    },
    get hidesWhenIdle() {
      return t("notchMascot.hidesWhenIdle");
    },
    get hidesWhenIdleHint() {
      return t("notchMascot.hidesWhenIdleHint");
    },
    get preview() {
      return t("notchMascot.preview");
    },
    get previewHint() {
      return t("notchMascot.previewHint");
    },
    get petTip() {
      return t("notchMascot.petTip");
    },
    get sayHi() {
      return t("notchMascot.sayHi");
    },
    get sayHiHint() {
      return t("notchMascot.sayHiHint");
    },
    get momentsTitle() {
      return t("notchMascot.momentsTitle");
    },
    moments: {
      get visit() {
        return t("notchMascot.visitMoment");
      },
      get timerStarted() {
        return t("notchMascot.timerStarted");
      },
      get timeIsUp() {
        return t("notchActivities.finished");
      },
      get music() {
        return t("radialMenu.mediaNowPlaying");
      },
      get agents() {
        return t("notchAgents.title");
      },
      get eventStarts() {
        return t("windows.settings.mascot.moments.eventStarts");
      },
      get downloadFinished() {
        return t("windows.settings.mascot.moments.downloadFinished");
      },
      get downloadFailed() {
        return t("notchMascot.downloadFailed");
      },
      get screenshot() {
        return t("screenshot.editorTitle");
      },
      get micMuted() {
        return t("notchVoice.muted");
      },
      get keepAwake() {
        return t("s.keepAwakeTitle");
      },
      get charging() {
        return t("notch.charging");
      },
      get lowBattery() {
        return t("notch.lowBattery");
      },
      get unlocked() {
        return t("windows.settings.mascot.moments.unlocked");
      },
    },
  },
  nav: {
    get general() {
      return t("s.tabGeneral");
    },
    get layout() {
      return t("notchEditor.layout");
    },
    get content() {
      return t("notchEditor.content");
    },
    get activity() {
      return t("notchEditor.activity");
    },
    get behavior() {
      return t("notchEditor.behavior");
    },
    get mascot() {
      return t("notchMascot.title");
    },
    get island() {
      return t("notch.title");
    },
  },
  get brand() {
    return t("windows.settings.brand");
  },
  general: {
    get island() {
      return t("notch.title");
    },
    get islandCaption() {
      return t("windows.settings.general.islandCaption");
    },
    get login() {
      return t("windows.settings.general.login");
    },
    get loginCaption() {
      return t("windows.settings.general.loginCaption");
    },
  },
  layout: {
    get size() {
      return t("windows.settings.layout.size");
    },
    get compact() {
      return t("notch.compact");
    },
    get spacious() {
      return t("notch.spacious");
    },
    get custom() {
      return t("notch.custom");
    },
    get sizeCaption() {
      return t("windows.settings.layout.sizeCaption");
    },
    get width() {
      return t("notch.width");
    },
    get maxHeight() {
      return t("notch.maximumHeight");
    },
    get customCaption() {
      return t("notch.sizeHint");
    },
    get notchFit() {
      return t("notch.cameraFit");
    },
    get notchFitCaption() {
      return t("windows.settings.layout.notchFitCaption");
    },
    get corners() {
      return t("notchShape.corners");
    },
    get cornersCaption() {
      return t("windows.settings.layout.cornersCaption");
    },
    get notchPreview() {
      return t("windows.settings.layout.notchPreview");
    },
    get notchPreviewCaption() {
      return t("windows.settings.layout.notchPreviewCaption");
    },
    get shoulderWidth() {
      return t("windows.settings.layout.shoulderWidth");
    },
    get shoulderWidthCaption() {
      return t("windows.settings.layout.shoulderWidthCaption");
    },
    get shoulderHeight() {
      return t("windows.settings.layout.shoulderHeight");
    },
    get shoulderHeightCaption() {
      return t("windows.settings.layout.shoulderHeightCaption");
    },
    get resetShape() {
      return t("windows.settings.layout.resetShape");
    },
    get shape() {
      return t("notchMascot.shape");
    },
    get shapeCaption() {
      return t("windows.settings.layout.shapeCaption");
    },
    get notch() {
      return t("notch.notchShape");
    },
    get capsule() {
      return t("notch.capsuleShape");
    },
    get capsulePreview() {
      return t("windows.settings.layout.capsulePreview");
    },
    get capsulePreviewCaption() {
      return t("windows.settings.layout.capsulePreviewCaption");
    },
    get capsuleFit() {
      return t("notch.capsuleFit");
    },
    get capsuleFitCaption() {
      return t("windows.settings.layout.capsuleFitCaption");
    },
    get height() {
      return t("notch.height");
    },
    get fromTop() {
      return t("notch.fromTop");
    },
    get outline() {
      return t("notch.showOutline");
    },
    get outlineCaption() {
      return t("windows.settings.layout.outlineCaption");
    },
  },
  content: {
    get idleContent() {
      return t("notch.idleContent");
    },
    get idleContentCaption() {
      return t("windows.settings.content.idleContentCaption");
    },
    get idleNone() {
      return t("notch.idleNone");
    },
    get idleBattery() {
      return t("notch.battery");
    },
    get idleMusic() {
      return t("notch.music");
    },
    get idleAgents() {
      return t("notchAgents.title");
    },
    get downloadFolders() {
      return t("windows.settings.content.downloadFolders");
    },
    get downloadFoldersCaption() {
      return t("windows.settings.content.downloadFoldersCaption");
    },
    get quickAccess() {
      return t("windows.settings.content.quickAccess");
    },
    get quickPreview() {
      return t("windows.settings.content.quickPreview");
    },
    get quickPreviewCaption() {
      return t("windows.settings.content.quickPreviewCaption");
    },
    get quickCapacity() {
      return t("windows.settings.content.quickCapacity");
    },
    get addShortcut() {
      return t("commandBar.linkAddButton");
    },
    get removeShortcut() {
      return t("s.shortcutClear");
    },
    get defaultShortcuts() {
      return t("windows.settings.content.defaultShortcuts");
    },
    get defaultShortcutsCaption() {
      return t("windows.settings.content.defaultShortcutsCaption");
    },
    get restoreDefaults() {
      return t("windows.settings.content.restoreDefaults");
    },
    get shortcutAction() {
      return t("radialMenu.actionLabel");
    },
    get shortcutSide() {
      return t("notchEditor.position");
    },
    get sideLeft() {
      return t("notch.quickAccessLeft");
    },
    get sideRight() {
      return t("notch.quickAccessRight");
    },
    get sideBottom() {
      return t("notchEditor.bottom");
    },
    get leftShortcut() {
      return t("windows.settings.content.leftShortcut");
    },
    get rightShortcut() {
      return t("windows.settings.content.rightShortcut");
    },
    get shortcutCaption() {
      return t("windows.settings.content.shortcutCaption");
    },
    get lead() {
      return t("windows.settings.content.lead");
    },
    get sections() {
      return t("notchEditor.sections");
    },
    get show() {
      return t("notchWatch.inIsland");
    },
    get moveUp() {
      return t("s.audioPriorityMoveUp");
    },
    get moveDown() {
      return t("s.audioPriorityMoveDown");
    },
    get position() {
      return t("notchEditor.position");
    },
    positionCaption: (n: number, of: number) => tf("windows.settings.content.positionCaption", n, of),
    get comingSoon() {
      return t("windows.settings.content.comingSoon");
    },
    get noOptions() {
      return t("notchEditor.noOptions");
    },
    get showPlayingMusic() {
      return t("notch.playingMusic");
    },
    get showPlayingMusicCaption() {
      return t("windows.settings.content.showPlayingMusicCaption");
    },
    get lyricsOnline() {
      return t("notchMusicExtras.online");
    },
    get lyricsOnlineCaption() {
      return t("windows.settings.content.lyricsOnlineCaption");
    },
    get hideTimerCountdown() {
      return t("notchActivities.hideTimerCountdown");
    },
    get hideTimerCountdownCaption() {
      return t("windows.settings.content.hideTimerCountdownCaption");
    },
    get timerSound() {
      return t("notchActivities.soundEnabled");
    },
    get timerSoundCaption() {
      return t("windows.settings.content.timerSoundCaption");
    },
    get watchSound() {
      return t("notchWatch.sound");
    },
    get watchSoundCaption() {
      return t("windows.settings.content.watchSoundCaption");
    },
    get colorFormat() {
      return t("windows.settings.content.colorFormat");
    },
    get colorFormatCaption() {
      return t("windows.settings.content.colorFormatCaption");
    },
    get bareHex() {
      return t("windows.settings.content.bareHex");
    },
    get quickToggles() {
      return t("quickToggles.pageTitle");
    },
    get quickTogglesCaption() {
      return t("windows.settings.content.quickTogglesCaption");
    },
    get ejectExcluded() {
      return t("windows.settings.content.ejectExcluded");
    },
    get ejectExcludedCaption() {
      return t("windows.settings.content.ejectExcludedCaption");
    },
    get toolsGridCaption() {
      return t("windows.settings.content.toolsGridCaption");
    },
    get textSize() {
      return t("scratchpad.textSize");
    },
    get textSizeCaption() {
      return t("windows.settings.content.textSizeCaption");
    },
    get retention() {
      return t("scratchpad.retentionTitle");
    },
    get retentionCaption() {
      return t("scratchpad.retentionCaption");
    },
    get retentionNever() {
      return t("s.batteryNever");
    },
    get retentionDay() {
      return t("scratchpad.retentionDay");
    },
    get retentionWeek() {
      return t("scratchpad.retentionWeek");
    },
    get retentionMonth() {
      return t("scratchpad.retentionMonth");
    },
    get dragReveal() {
      return t("notch.dragReveal");
    },
    get dragRevealCaption() {
      return t("windows.settings.content.dragRevealCaption");
    },
    get shelfCloseAfterDrop() {
      return t("s.shelfCloseAfterDrop");
    },
    get shelfCloseAfterDropCaption() {
      return t("windows.settings.content.shelfCloseAfterDropCaption");
    },
    get shelfRemoveAfterDrop() {
      return t("s.shelfRemoveAfterDrop");
    },
    get shelfRemoveAfterDropCaption() {
      return t("s.shelfRemoveAfterDropCaption");
    },
    get notificationsCaption() {
      return t("windows.settings.content.notificationsCaption");
    },
  },
  system: {
    get interval() {
      return t("s.monitorIntervalLabel");
    },
    get intervalCaption() {
      return t("windows.settings.system.intervalCaption");
    },
    get interval1() {
      return t("s.monitorInterval1");
    },
    get interval2() {
      return t("s.monitorInterval2");
    },
    get interval5() {
      return t("s.monitorInterval5");
    },
    get speedUnit() {
      return t("monitorLayout.networkSpeedUnit");
    },
    metrics: {
      get cpu() {
        return t("s.cpuLabel");
      },
      get gpu() {
        return t("s.gpuLabel");
      },
      get memory() {
        return t("s.memorySection");
      },
      get network() {
        return t("s.monitorShowNetwork");
      },
      get disk() {
        return t("windows.settings.system.metrics.disk");
      },
      get power() {
        return t("s.monitorShowPowerLabel");
      },
      get connectedDevices() {
        return t("connectedDevices.title");
      },
    },
    captions: {
      get cpu() {
        return t("windows.settings.system.captions.cpu");
      },
      get gpu() {
        return t("windows.settings.system.captions.gpu");
      },
      get memory() {
        return t("windows.settings.system.captions.memory");
      },
      get network() {
        return t("hub.descMonitorNetwork");
      },
      get disk() {
        return t("hub.descMonitorDisk");
      },
      get power() {
        return t("hub.descMonitorPower");
      },
      get connectedDevices() {
        return t("windows.settings.system.captions.connectedDevices");
      },
    },
  },
  summaries: {
    get controls() {
      return t("notchEditor.controlsSummary");
    },
    get mixer() {
      return t("notchEditor.mixerSummary");
    },
    get music() {
      return t("notchEditor.musicSummary");
    },
    get clipboard() {
      return t("notchEditor.clipboardSummary");
    },
    get captures() {
      return t("notchEditor.capturesSummary");
    },
    get files() {
      return t("notchEditor.filesSummary");
    },
    get system() {
      return t("notchEditor.systemSummary");
    },
    get tools() {
      return t("notchEditor.toolsSummary");
    },
    get calendar() {
      return t("notchEditor.calendarSummary");
    },
    get notifications() {
      return t("windows.settings.summaries.notifications");
    },
    get timer() {
      return t("notchEditor.timerSummary");
    },
    get camera() {
      return t("notchEditor.cameraSummary");
    },
    get downloads() {
      return t("notchEditor.downloadsSummary");
    },
    get scratchpad() {
      return t("notchEditor.scratchpadSummary");
    },
    get agents() {
      return t("windows.settings.summaries.agents");
    },
    get sessions() {
      return t("notchClaudeSessions.summary");
    },
    get watch() {
      return t("notchEditor.watchSummary");
    },
  },
  activity: {
    get downloadProgress() {
      return t("windows.settings.activity.downloadProgress");
    },
    get downloadProgressCaption() {
      return t("windows.settings.activity.downloadProgressCaption");
    },
    get indicators() {
      return t("notchEditor.feedback");
    },
    get indicatorsLead() {
      return t("windows.settings.activity.indicatorsLead");
    },
    get timerCountdown() {
      return t("windows.settings.activity.timerCountdown");
    },
    get timerCountdownCaption() {
      return t("windows.settings.activity.timerCountdownCaption");
    },
    get trackChange() {
      return t("notch.newTrack");
    },
    get trackChangeCaption() {
      return t("windows.settings.activity.trackChangeCaption");
    },
    get more() {
      return t("windows.settings.activity.more");
    },
    get keepAwake() {
      return t("notchActivities.keepAwakeActivity");
    },
    get keepAwakeCaption() {
      return t("windows.settings.activity.keepAwakeCaption");
    },
    get volume() {
      return t("notch.volume");
    },
    get volumeCaption() {
      return t("windows.settings.activity.volumeCaption");
    },
    get agents() {
      return t("windows.settings.activity.agents");
    },
    get agentsCaption() {
      return t("windows.settings.activity.agentsCaption");
    },
  },
  behavior: {
    get opening() {
      return t("notchEditor.opening");
    },
    get openingCaption() {
      return t("windows.settings.behavior.openingCaption");
    },
    get click() {
      return t("notchEditor.clickOpen");
    },
    get hoverPreview() {
      return t("notchEditor.hoverPreview");
    },
    get hoverExpand() {
      return t("notchEditor.hoverExpand");
    },
    get hiddenUntilHover() {
      return t("notchEditor.hiddenUntilHover");
    },
    get showActivity() {
      return t("notchEditor.hiddenShowsActivity");
    },
    get showActivityCaption() {
      return t("notchEditor.hiddenShowsActivityHint");
    },
    get activation() {
      return t("notchEditor.activationTime");
    },
    get activationCaption() {
      return t("notchEditor.activationTimeHint");
    },
    get closing() {
      return t("notchEditor.closeTime");
    },
    get closingCaption() {
      return t("notchEditor.closeTimeHint");
    },
    get shortcut() {
      return t("s.globalHotkeySection");
    },
    get shortcutCaption() {
      return t("windows.settings.behavior.shortcutCaption");
    },
    get shortcutTaken() {
      return t("windows.settings.behavior.shortcutTaken");
    },
    get shortcutChange() {
      return t("windows.settings.behavior.shortcutChange");
    },
    get shortcutPressKeys() {
      return t("windows.settings.behavior.shortcutPressKeys");
    },
    get shortcutRecording() {
      return t("windows.settings.behavior.shortcutRecording");
    },
    get shortcutInvalid() {
      return t("windows.settings.behavior.shortcutInvalid");
    },
    get reopening() {
      return t("notchEditor.reopening");
    },
    get reopeningCaption() {
      return t("windows.settings.behavior.reopeningCaption");
    },
    get lastPage() {
      return t("notchEditor.lastPage");
    },
    get explore() {
      return t("notch.sectionsTitle");
    },
    get display() {
      return t("notch.display");
    },
    get hideFullscreen() {
      return t("windows.settings.behavior.hideFullscreen");
    },
    get hideFullscreenCaption() {
      return t("windows.settings.behavior.hideFullscreenCaption");
    },
    get where() {
      return t("s.switcherScreenPlacementLabel");
    },
    get whereCaption() {
      return t("windows.settings.behavior.whereCaption");
    },
    get displayPrimary() {
      return t("notch.mainDisplay");
    },
    get displayCursor() {
      return t("notch.followPointer");
    },
    get privacy() {
      return t("notchEditor.privacy");
    },
    get captures() {
      return t("notch.showInCaptures");
    },
    get capturesCaption() {
      return t("windows.settings.behavior.capturesCaption");
    },
  },
  sessions: {
    get lead() {
      return t("windows.settings.sessions.lead");
    },
    get claudeOnCaption() {
      return t("windows.settings.sessions.claudeOnCaption");
    },
    get claudeOffCaption() {
      return t("windows.settings.sessions.claudeOffCaption");
    },
    get hooksOn() {
      return t("s.homebrewInstalled");
    },
    get hooksOff() {
      return t("s.homebrewNotInstalledBadge");
    },
    get hooksStale() {
      return t("windows.settings.sessions.hooksStale");
    },
    get hooksProblem() {
      return t("windows.settings.sessions.hooksProblem");
    },
    get install() {
      return t("notchClaudeSessions.installHooks");
    },
    get update() {
      return t("windows.settings.sessions.update");
    },
    get remove() {
      return t("notchClaudeSessions.removeHooks");
    },
    get bridge() {
      return t("windows.settings.sessions.bridge");
    },
    get bridgeCaption() {
      return t("windows.settings.sessions.bridgeCaption");
    },
    get bridgeNoEditors() {
      return t("windows.settings.sessions.bridgeNoEditors");
    },
    get bridgeOn() {
      return t("s.homebrewInstalled");
    },
    get bridgeOff() {
      return t("s.homebrewNotInstalledBadge");
    },
    get bridgeStale() {
      return t("windows.settings.sessions.bridgeStale");
    },
    get bridgeInstall() {
      return t("s.homebrewInstall");
    },
    get bridgeUpdate() {
      return t("s.homebrewUpgrade");
    },
    bridgeFailures: {
      get notFound() {
        return t("windows.settings.sessions.bridgeFailures.notFound");
      },
      get packaging() {
        return t("windows.settings.sessions.bridgeFailures.packaging");
      },
      get refused() {
        return t("windows.settings.sessions.bridgeFailures.refused");
      },
      get timedOut() {
        return t("windows.settings.sessions.bridgeFailures.timedOut");
      },
    } as Record<string, string>,
    get codexCaption() {
      return t("windows.settings.sessions.codexCaption");
    },
    get codexConnected() {
      return t("notchActivities.connected");
    },
    get codexNotRunning() {
      return t("notchClaudeSessions.codexNotRunning");
    },
    get codexNotFound() {
      return t("windows.settings.sessions.codexNotFound");
    },
    get previews() {
      return t("notchClaudeSessions.contentPreview");
    },
    get previewsCaption() {
      return t("notchClaudeSessions.contentPreviewHint");
    },
    get autoExpand() {
      return t("notchClaudeSessions.autoExpand");
    },
    get autoExpandCaption() {
      return t("notchClaudeSessions.autoExpandHint");
    },
    get live() {
      return t("notchClaudeSessions.liveActivity");
    },
    get liveCaption() {
      return t("windows.settings.sessions.liveCaption");
    },
    get buddies() {
      return t("notchClaudeSessions.buddies");
    },
    get buddiesCaption() {
      return t("windows.settings.sessions.buddiesCaption");
    },
  },
  calendar: {
    get permission() {
      return t("windows.settings.calendar.permission");
    },
    get access() {
      return t("windows.settings.calendar.access");
    },
    get granted() {
      return t("windows.settings.calendar.granted");
    },
    get notAsked() {
      return t("windows.settings.calendar.notAsked");
    },
    get denied() {
      return t("windows.settings.calendar.denied");
    },
    get unavailable() {
      return t("windows.settings.calendar.unavailable");
    },
    get unavailableCaption() {
      return t("windows.settings.calendar.unavailableCaption");
    },
    get allow() {
      return t("notchCalendar.allow");
    },
    get settings() {
      return t("windows.settings.calendar.settings");
    },
    get countdown() {
      return t("notchCalendar.countdown");
    },
    get countdownHint() {
      return t("notchCalendar.countdownHint");
    },
    get timeLeft() {
      return t("notchCalendar.timeLeft");
    },
    get timeLeftHint() {
      return t("notchCalendar.timeLeftHint");
    },
    get weekNumbers() {
      return t("notchCalendar.weekNumbers");
    },
    get calendars() {
      return t("notchCalendar.calendars");
    },
    get noCalendars() {
      return t("windows.settings.calendar.noCalendars");
    },
  },
  controls: {
    get music() {
      return t("radialMenu.mediaNowPlaying");
    },
    get volume() {
      return t("notch.volume");
    },
    get brightness() {
      return t("notch.brightness");
    },
    get keepAwake() {
      return t("s.keepAwakeTitle");
    },
    get microphone() {
      return t("s.micMuteName");
    },
    get mixer() {
      return t("s.mixerSection");
    },
    get timer() {
      return t("notchActivities.timer");
    },
    get calendar() {
      return t("notchCalendar.title");
    },
    get screenshot() {
      return t("screenshot.editorTitle");
    },
    get speedTest() {
      return t("s.monitorItemNetTest");
    },
    get scratchpad() {
      return t("scratchpad.pageTitle");
    },
    get levelsCaption() {
      return t("windows.settings.controls.levelsCaption");
    },
    get shortcuts() {
      return t("notch.controlShortcuts");
    },
    get shortcutsCaption() {
      return t("windows.settings.controls.shortcutsCaption");
    },
    get brightnessSliders() {
      return t("windows.settings.controls.brightnessSliders");
    },
    get brightnessSlidersCaption() {
      return t("windows.settings.controls.brightnessSlidersCaption");
    },
    get keepAwakeFor() {
      return t("windows.settings.controls.keepAwakeFor");
    },
    get keepAwakeForCaption() {
      return t("windows.settings.controls.keepAwakeForCaption");
    },
    get untilOff() {
      return t("windows.settings.controls.untilOff");
    },
    minutes: (n: number) =>
      n < 60
        ? tf("WhatsAppOrganizerStrings.localized.minutesFormat", n)
        : n === 60
          ? t("s.hour1")
          : tf("windows.settings.controls.minutes", n / 60),
    get hideInactive() {
      return t("mixer.hideInactiveApps");
    },
    get hideInactiveCaption() {
      return t("windows.settings.controls.hideInactiveCaption");
    },
    get hiddenApps() {
      return t("windows.settings.controls.hiddenApps");
    },
    hiddenAppsCaption: (n: number) =>
      n === 0
        ? t("windows.settings.controls.hiddenAppsCaption")
        : tf("windows.settings.controls.hiddenAppsCaption249e0b", n),
    get showAll() {
      return t("s.menuShowAll");
    },
    get mixerCaption() {
      return t("windows.settings.controls.mixerCaption");
    },
  },
  clipboard: {
    // ClipboardFeatureStrings, ClipboardRetentionStrings, ClipboardIgnoredAppsStrings,
    // NotchClipboardSizeStrings and NotchStrings on the Mac.
    get enable() {
      return t("clipboard.enable");
    },
    get enableCaption() {
      return t("clipboard.caption");
    },
    get includeImagesFiles() {
      return t("clipboard.includeImagesFiles");
    },
    get includeImagesFilesCaption() {
      return t("clipboard.includeImagesFilesCaption");
    },
    get skipSensitive() {
      return t("clipboard.skipSensitive");
    },
    get skipSensitiveCaption() {
      return t("clipboard.skipSensitiveCaption");
    },
    get limit() {
      return t("notchAgents.readoutLimit");
    },
    get limitUnlimited() {
      return t("clipboard.limitUnlimited");
    },
    get retention() {
      return t("clipboardRetention.title");
    },
    get retentionCaption() {
      return t("clipboardRetention.caption");
    },
    get forever() {
      return t("clipboardRetention.forever");
    },
    days: (n: number) => (n === 1 ? t("clipboardRetention.oneDay") : tf("clipboardRetention.daysFormat", n)),
    get ignoredApps() {
      return t("clipboardIgnoredApps.listTitle");
    },
    get ignoredAppsCaption() {
      return t("windows.settings.clipboard.ignoredAppsCaption");
    },
    get entrySize() {
      return t("notchClipboardSize.entrySize");
    },
    get entrySizeCaption() {
      return t("notchClipboardSize.hint");
    },
    get compact() {
      return t("notch.compact");
    },
    get comfortable() {
      return t("notchClipboardSize.comfortable");
    },
    get notice() {
      return t("notch.clipboardActivity");
    },
    get noticeCaption() {
      return t("windows.settings.clipboard.noticeCaption");
    },
  },
  agents: {
    get cardsKeys() {
      return t("windows.settings.agents.cardsKeys");
    },
    get cardShown() {
      return t("windows.settings.agents.cardShown");
    },
    get cardHidden() {
      return t("s.mixerHiddenCountLabel");
    },
  },
  get saveFailed() {
    return t("windows.settings.saveFailed");
  },
} as const;
