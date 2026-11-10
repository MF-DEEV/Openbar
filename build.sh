#!/bin/zsh
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Vorssaint

# Builds Openbar, assembles the .app bundle, signs it and (with --install)
# installs it into /Applications.
#
# The bundle is staged in a temporary directory outside ~/Documents: folders synced
# by File Provider gain xattrs (com.apple.provenance etc.) that invalidate codesign.
set -euo pipefail
cd "$(dirname "$0")"

# The icon catalog and the bundle are staged in temp dirs; sweep both however
# the script ends.
ICON_TMP=""
STAGE_TMP=""

cleanup() {
    [[ -n "$ICON_TMP" ]] && rm -rf "$ICON_TMP"
    [[ -n "$STAGE_TMP" ]] && rm -rf "$STAGE_TMP"
    return 0
}
trap cleanup EXIT
# zsh runs the EXIT trap when the script is hung up, but not when it is
# interrupted or terminated; route those through exit so a Ctrl-C partway
# into the build sweeps like any other ending.
trap 'exit 1' INT TERM HUP

# Flags: --dev builds the local-only "Openbar (Developer)" variant (its own
# bundle id, so it coexists with the official app); --install puts it in /Applications.
DEV=0
INSTALL=0
TEST=0
TEST_ARGS=()
for arg in "$@"; do
    case "$arg" in
        --dev)     DEV=1 ;;
        --install) INSTALL=1 ;;
        --test)    TEST=1 ;;
        --test-suite=*) TEST=1; TEST_ARGS+=("--suite=${arg#*=}") ;;
        --list-tests) TEST=1; TEST_ARGS+=(--list) ;;
    esac
done

if (( DEV )); then
    APP_NAME="Openbar (Developer)"
    EXECUTABLE="OpenbarDeveloper"
    APP_BUNDLE_ID="com.openbar.app.dev"
    BUILD_VARIANT_FLAGS=(-D OPENBAR_DEVELOPMENT)
    APP_OPTIMIZATION_FLAGS=(-Onone)
    BUILD_CONFIGURATION="debug"
else
    APP_NAME="Openbar"
    EXECUTABLE="Openbar"
    APP_BUNDLE_ID="com.openbar.app"
    BUILD_VARIANT_FLAGS=()
    APP_OPTIMIZATION_FLAGS=(-O)
    BUILD_CONFIGURATION="release"
fi
FAN_HELPER_ID="$APP_BUNDLE_ID.fan-control"
# Now Playing is read through /usr/bin/perl loading this library; see
# Sources/NowPlayingAdapter. Staged under Contents/Frameworks, signed on its own.
NOW_PLAYING_ADAPTER_ID="$APP_BUNDLE_ID.now-playing"
NOW_PLAYING_ADAPTER="libOpenbarNowPlaying.dylib"
TARGET="arm64-apple-macosx14.0"
ENTITLEMENTS="Resources/Openbar.entitlements"
LEGACY_IDENTITY="Openbar Signing"

developer_id_identity() {
    security find-identity -v -p codesigning 2>/dev/null \
        | grep 'Developer ID Application' \
        | head -1 \
        | sed -E 's/.*"(.*)".*/\1/' || true
}

# A find-identity listing also names certificates codesign then rejects (an
# expired one fails the build with errSecInternalComponent), and -v excludes
# every self-signed one; ask codesign itself with a throwaway copy of /bin/echo.
legacy_identity_installed() {
    local probe signed=1
    # A locked keychain still lists its identities but cannot sign with them,
    # and this one is locked after every reboot; unlock it before asking.
    security unlock-keychain -p openbar-signing \
        "$HOME/Library/Keychains/openbar-signing.keychain-db" 2>/dev/null || true
    probe="$(mktemp)"
    cp /bin/echo "$probe"
    /usr/bin/codesign --force --strip-disallowed-xattrs --sign "$LEGACY_IDENTITY" "$probe" \
        >/dev/null 2>&1 && signed=0
    rm -f "$probe"
    return $signed
}

# Any build that lands in /Applications needs a stable signature, not just the
# Developer one: macOS ties Accessibility and Screen Recording grants to the
# exact binary hash, so an ad-hoc rebuild orphans them while System Settings
# keeps showing them as granted, and no new prompt ever appears. A plain
# --install strands them under the released bundle id, on the app the user
# actually relies on. When no identity is installed, create the stable local one
# up front instead of falling through to ad-hoc — setup-signing.sh is free,
# offline and idempotent. Gating on the install rather than the variant keeps
# this off CI, which never passes --install.
if (( DEV || INSTALL )) && [[ -z "$(developer_id_identity)" ]] \
    && ! legacy_identity_installed; then
    echo "▸ No signing identity installed; creating the stable local one…"
    if ! ./Tools/setup-signing.sh; then
        echo "  ⚠ Tools/setup-signing.sh failed; signing ad-hoc instead." >&2
        echo "    Accessibility and Screen Recording grants will not survive rebuilds:" >&2
        echo "    System Settings will show them as granted while the app is not trusted." >&2
        echo "    After fixing the identity, clear the stale grant once with:" >&2
        echo "      tccutil reset Accessibility $APP_BUNDLE_ID" >&2
    fi
fi

codesign_with_timestamp_retry() {
    local attempt
    for attempt in 1 2 3; do
        if /usr/bin/codesign "$@"; then
            return 0
        fi
        if (( attempt < 3 )); then
            echo "  Developer ID signing failed; retrying ($((attempt + 1))/3)"
            sleep "$attempt"
        fi
    done
    return 1
}

write_swift_output_file_map() {
    local output_file="$1"
    local object_dir="$2"
    shift 2
    local source artifact

    {
        print -r -- "{"
        print -r -- "  \"\": {"
        print -r -- "    \"swift-dependencies\": \"$object_dir/master.swiftdeps\""
        print -r -- "  }"
        for source in "$@"; do
            artifact="${source//\//__}"
            artifact="${artifact%.swift}"
            print -r -- ","
            print -r -- "  \"$source\": {"
            print -r -- "    \"object\": \"$object_dir/$artifact.o\","
            print -r -- "    \"swift-dependencies\": \"$object_dir/$artifact.swiftdeps\""
            print -r -- "  }"
        done
        print -r -- "}"
    } > "$output_file"
}

finalize_installed_bundle_after_child() {
    local bundle="$1"
    local helper="$bundle/Contents/Library/LaunchServices/$FAN_HELPER_ID"
    local adapter="$bundle/Contents/Frameworks/$NOW_PLAYING_ADAPTER"
    local devid
    devid="$(developer_id_identity)"

    echo "▸ Finalizing installed signature…"
    sleep 3
    if [[ -n "$devid" ]]; then
        [[ -f "$helper" ]] && codesign_with_timestamp_retry --force --strip-disallowed-xattrs \
            --options runtime --timestamp --identifier "$FAN_HELPER_ID" --sign "$devid" "$helper"
        [[ -f "$adapter" ]] && codesign_with_timestamp_retry --force --strip-disallowed-xattrs \
            --options runtime --timestamp --identifier "$NOW_PLAYING_ADAPTER_ID" --sign "$devid" "$adapter"
        codesign_with_timestamp_retry --force --strip-disallowed-xattrs --options runtime --timestamp \
            --entitlements "$ENTITLEMENTS" --sign "$devid" "$bundle"
    elif legacy_identity_installed; then
        [[ -f "$helper" ]] && /usr/bin/codesign --force --strip-disallowed-xattrs \
            --identifier "$FAN_HELPER_ID" --sign "$LEGACY_IDENTITY" "$helper"
        [[ -f "$adapter" ]] && /usr/bin/codesign --force --strip-disallowed-xattrs \
            --identifier "$NOW_PLAYING_ADAPTER_ID" --sign "$LEGACY_IDENTITY" "$adapter"
        /usr/bin/codesign --force --strip-disallowed-xattrs --sign "$LEGACY_IDENTITY" "$bundle"
    else
        [[ -f "$helper" ]] && /usr/bin/codesign --force --strip-disallowed-xattrs \
            --identifier "$FAN_HELPER_ID" --sign - "$helper"
        [[ -f "$adapter" ]] && /usr/bin/codesign --force --strip-disallowed-xattrs \
            --identifier "$NOW_PLAYING_ADAPTER_ID" --sign - "$adapter"
        /usr/bin/codesign --force --strip-disallowed-xattrs --sign - "$bundle"
    fi
    [[ -f "$helper" ]] && /usr/bin/codesign --verify --strict "$helper"
    [[ -f "$adapter" ]] && /usr/bin/codesign --verify --strict "$adapter"
    /usr/bin/codesign --verify --deep --strict "$bundle"
    echo "✓ Signature ready: $bundle"
}

if (( INSTALL && ! TEST )) && [[ "${OPENBAR_INSTALL_CHILD:-0}" != "1" ]]; then
    OPENBAR_INSTALL_CHILD=1 "$0" "$@"
    child_status=$?
    if (( child_status != 0 )); then
        exit "$child_status"
    fi
    finalize_installed_bundle_after_child "/Applications/$APP_NAME.app"
    exit 0
fi

# Prefer the macOS 26 SDK when present: the 27 SDK turns SwiftUI property wrappers
# into macros (SwiftUIMacros plugin) that the Command Line Tools cannot load yet.
PINNED_SDK="/Library/Developer/CommandLineTools/SDKs/MacOSX26.sdk"
if [[ -n "${DEVELOPER_DIR:-}" ]]; then
    SDK="$(xcrun --show-sdk-path)"
elif [[ -d "$PINNED_SDK" ]]; then
    SDK="$PINNED_SDK"
else
    SDK="$(xcrun --show-sdk-path)"
fi
SDK_COMPAT_FLAGS=()
VM_STATISTICS_COMPAT_FLAGS=(-I Sources/VMStatisticsCompat)
HID_EVENT_SYSTEM_FLAGS=(-I Sources/HIDEventSystem)
if [[ "$SDK" == "$PINNED_SDK" ]]; then
    # Swift 6.4 can read the SDK 26 interfaces when given their compiler version.
    SDK_COMPAT_FLAGS=(-Xfrontend -interface-compiler-version -Xfrontend 6.3.2)
fi

# The defaults migrations under test need a real UserDefaults suite, and every
# suite leaves an empty plist in ~/Library/Preferences. The tests already clear
# the domains, but cfprefsd writes the emptied file back out around the time the
# process that owned it exits, so only a caller that outlives the run can remove
# them. `PreferenceNamespaceTests` scans every compiled Swift test file against
# these namespaces, which keeps this sweep complete without a second list.
discard_test_preferences() {
    local preferences="${1:-$HOME/Library/Preferences}" name attempt
    local survivors=0 quiet_passes=0
    # cfprefsd can recreate an emptied domain after the first removal. Require
    # two quiet checks, but keep a hard limit so persistent failures still fail CI.
    for attempt in {1..10}; do
        for name in "vorss.tests." "com.openbar.tests."; do
            rm -f "$preferences"/$name*.plist(N)
        done
        rm -f "$preferences/metrics-tests.plist"
        sleep 0.2
        survivors=$(find "$preferences" -maxdepth 1 \
            \( -name "vorss.tests.*.plist" -o -name "com.openbar.tests.*.plist" \
               -o -name "metrics-tests.plist" \) 2>/dev/null | wc -l | tr -d ' ')
        if [[ "$survivors" == "0" ]]; then
            quiet_passes=$((quiet_passes + 1))
            if (( quiet_passes == 2 )); then return 0; fi
        else
            quiet_passes=0
        fi
    done
    echo "✗ test preferences did not settle in $preferences ($survivors remaining)" >&2
    return 1
}

# Standalone automated tests: pure contracts plus isolated disk, subprocess,
# keyboard-data and media fixtures. No application windows or device capture.
if (( TEST )); then
    python3 Tests/generate_sources.py
    TEST_OBJECT_DIR="build/objects/tests"
    mkdir -p "$TEST_OBJECT_DIR"
    TEST_SOURCES=(
        Sources/Openbar/UI/BrandPalette.swift
        Sources/Openbar/Services/Media/MediaSupport.swift
        Sources/Openbar/Core/QuitProtectionSupport.swift
        Sources/Openbar/Core/QuitProtectionStrings.swift
        Sources/Openbar/Core/Defaults.swift
        Sources/Openbar/Core/NotchStrings.swift
        Sources/Openbar/Core/NotchTourStrings.swift
        Sources/Openbar/Core/NotchEditorStrings.swift
        Sources/Openbar/UI/Settings/NotchSettingsTabRow.swift
        Sources/Openbar/Core/NotchActivityStrings.swift
        Sources/Openbar/Core/NotchVoiceStrings.swift
        Sources/Openbar/Services/LevelDB.swift
        Sources/Openbar/Services/Discord/DiscordStorage.swift
        Sources/Openbar/Services/Discord/DiscordKeybinds.swift
        Sources/Openbar/Services/Discord/DiscordVoiceSupport.swift
        Sources/Openbar/Services/Notch/NotchTimerSupport.swift
        Sources/Openbar/Services/Notch/NotchTimerAlert.swift
        Sources/Openbar/UI/Notch/NotchIslandAlert.swift
        Sources/Openbar/Services/Notch/NotchAccessorySupport.swift
        Sources/Openbar/Services/QuickTools/CameraPreviewSupport.swift
        Sources/Openbar/Core/NotchMusicExtrasStrings.swift
        Sources/Openbar/Core/NotchClipboardSizeStrings.swift
        Sources/Openbar/Services/Notch/NotchLyricsSupport.swift
        Sources/Openbar/Services/Notch/NotchQueueSupport.swift
        Sources/Openbar/Services/Notch/NotchPreferredPlayer.swift
        Sources/Openbar/Core/NotchFilesStrings.swift
        Sources/Openbar/Core/NotchWatchStrings.swift
        Sources/Openbar/Services/Notch/NotchWatchSupport.swift
        Sources/Openbar/Core/NotchMascotStrings.swift
        Sources/Openbar/Services/Notch/NotchMascotSupport.swift
        Sources/Openbar/Services/CommandBar/CommandBarDropletMotion.swift
        Sources/Openbar/Services/Notch/NotchFileToolsSupport.swift
        Sources/Openbar/Services/Notch/NotchDownloadSupport.swift
        Sources/Openbar/Services/Notch/NotchDownloadProgressObserver.swift
        Sources/Openbar/Services/Notch/NotchDownloadFolders.swift
        Sources/Openbar/Services/Notch/NotchChromiumDownloads.swift
        Sources/Openbar/Core/NotchCalendarStrings.swift
        Sources/Openbar/Core/NotchNotificationStrings.swift
        Sources/Openbar/Core/NotchGestureStrings.swift
        Sources/Openbar/Core/NotchAgentStrings.swift
        Sources/Openbar/Core/CodexAccountStrings.swift
        Sources/Openbar/Services/Notch/NotchAgentSupport.swift
        Sources/Openbar/Core/NotchClaudeSessionStrings.swift
        Sources/Openbar/Services/Notch/NotchClaudeSessionsSupport.swift
        Sources/Openbar/Services/ClaudeSessions/VoiceReplySupport.swift
        Sources/Openbar/Core/NotchVoiceReplyStrings.swift
        Sources/Openbar/Services/Notch/NotchSessionLimitsSupport.swift
        Sources/Openbar/Services/Notch/NotchSessionAttention.swift
        Sources/Openbar/Services/Notch/NotchSessionWaitingSound.swift
        Sources/Openbar/Services/ClaudeSessions/ClaudeSessionModels.swift
        Sources/Openbar/Services/ClaudeSessions/ClaudeFileDiff.swift
        Sources/Openbar/Services/ClaudeSessions/ClaudeSessionEngine.swift
        Sources/Openbar/Services/ClaudeSessions/ClaudeSessionRegistryReader.swift
        Sources/Openbar/Services/ClaudeSessions/ClaudeTranscriptReader.swift
        Sources/Openbar/Services/ClaudeSessions/ClaudeSessionHosts.swift
        Sources/Openbar/Services/ClaudeSessions/ClaudeTerminalHosts.swift
        Sources/Openbar/Services/ClaudeSessions/ClaudeBackgroundSessions.swift
        Sources/Openbar/Services/ClaudeSessions/SessionWorktrees.swift
        Sources/Openbar/Services/ClaudeSessions/ClaudeImageAttachments.swift
        Sources/Openbar/Services/ClaudeSessions/ClaudeChatImages.swift
        Sources/Openbar/Services/ClaudeSessions/ClaudeHookMessage.swift
        Sources/Openbar/Services/ClaudeSessions/ClaudePlanModes.swift
        Sources/Openbar/Services/ClaudeSessions/ClaudeHookServer.swift
        Sources/Openbar/Services/ClaudeSessions/ClaudeHookInstaller.swift
        Sources/Openbar/Services/ClaudeSessions/RemoteHosts.swift
        Sources/Openbar/Services/ClaudeSessions/RemoteHostLink.swift
        Sources/Openbar/Core/RemoteHostStrings.swift
        Sources/Openbar/Services/ClaudeSessions/ClawdCostume.swift
        Sources/Openbar/Services/ClaudeSessions/ClaudeAgentColors.swift
        Sources/Openbar/Services/ClaudeSessions/CodexSessionState.swift
        Sources/Openbar/Services/ClaudeSessions/CodexSessionClient.swift
        Sources/Openbar/Services/ClaudeSessions/AgentHookMessage.swift
        Sources/Openbar/Services/ClaudeSessions/AgentHookSessions.swift
        Sources/Openbar/Services/ClaudeSessions/AgentHookInstaller.swift
        Sources/Openbar/Services/ClaudeSessions/AgentOpenCodePlugin.swift
        Sources/Openbar/Core/LegacyIdentity.swift
        Sources/Openbar/Core/NotchLockScreenStrings.swift
        Sources/Openbar/Services/Notch/NotchLockScreenSupport.swift
        Sources/Openbar/Services/AgentUsage/AgentUsageModels.swift
        Sources/Openbar/Services/AgentUsage/AgentPricing.swift
        Sources/Openbar/Services/AgentUsage/AgentLogObject.swift
        Sources/Openbar/Services/AgentUsage/AgentLogParser.swift
        Sources/Openbar/Services/AgentUsage/Harness/AgentHarnessReader.swift
        Sources/Openbar/Services/AgentUsage/Harness/ClaudeReader.swift
        Sources/Openbar/Services/AgentUsage/Harness/CodexReader.swift
        Sources/Openbar/Services/AgentUsage/Harness/OpenCodeReader.swift
        Sources/Openbar/Services/AgentUsage/Harness/CopilotReader.swift
        Sources/Openbar/Services/AgentUsage/AgentUsageSummary.swift
        Sources/Openbar/Services/AgentUsage/AgentUsageStore.swift
        Sources/Openbar/Services/AgentUsage/AgentUsageArchive.swift
        Sources/Openbar/Services/AgentUsage/AgentClaudeAppUsage.swift
        Sources/Openbar/Services/AgentUsage/AgentCLIConversation.swift
        Sources/Openbar/Services/AgentUsage/AgentClaudeServer.swift
        Sources/Openbar/Services/AgentUsage/AgentCodexServer.swift
        Sources/Openbar/Services/AgentUsage/CodexAccountLimits.swift
        Sources/Openbar/Services/AgentUsage/CodexProfileLimitsService.swift
        Sources/Openbar/Services/AgentUsage/AgentOpenCodeReader.swift
        Sources/Openbar/Services/Notch/NotchGestureSupport.swift
        Sources/Openbar/Services/Notch/NotchSectionPaging.swift
        Sources/Openbar/Services/Notch/NotchSliderEditing.swift
        Sources/Openbar/Services/Notch/NotchNotificationSupport.swift
        Sources/Openbar/Services/Discord/DiscordReplySupport.swift
        Sources/Openbar/Core/DiscordReplyStrings.swift
        Sources/Openbar/Services/Notch/NotchNotificationReaderCore.swift
        Sources/Openbar/Services/Notch/NotchCalendarSupport.swift
        Sources/Openbar/Services/Notch/NotchKeepAwakeSupport.swift
        Sources/Openbar/Services/Notch/NotchSupport.swift
        Sources/Openbar/Services/Notch/NotchAudioLevelSupport.swift
        Sources/Openbar/Services/Notch/NotchVolumeKeyGate.swift
        Sources/Openbar/Services/Notch/NotchMusicSupport.swift
        Sources/Openbar/UI/Notch/NotchDecorativeClock.swift
        Sources/Openbar/Services/Notch/NotchBrowserArtworkSupport.swift
        Sources/Openbar/UI/Notch/NotchEqualizerBars.swift
        Sources/Openbar/UI/Notch/NotchAgentMarkPath.swift
        Sources/Openbar/UI/Notch/NotchHookAgentMarks.swift
        Sources/Openbar/UI/Notch/NotchScrollEdgeFade.swift
        Sources/Openbar/UI/Notch/NotchAgentAnimationView.swift
        Sources/Openbar/UI/Notch/NotchClawdSprite.swift
        Sources/Openbar/UI/Notch/NotchClawdView.swift
        Sources/Openbar/UI/WindowVisibilityReader.swift
        Sources/Openbar/Services/Notch/NotchMusicAutomationSupport.swift
        Sources/Openbar/Services/Notch/NotchMusicAutomation.swift
        Sources/Openbar/Services/Notch/NotchPlaybackSource.swift
        Sources/Openbar/Services/Notch/NotchPlaybackCommand.swift
        Sources/Openbar/Services/Notch/NotchMusicCommandWriter.swift
        Sources/Openbar/Core/FeatureCatalog.swift
        Sources/Openbar/Core/FeaturePresets.swift
        Sources/Openbar/Core/FeatureHubStrings.swift
        Sources/Openbar/Core/ShortcutSettingsStrings.swift
        Sources/Openbar/Core/SettingsBackupSupport.swift
        Sources/Openbar/Core/BackupStrings.swift
        Sources/Openbar/Core/SnippetStrings.swift
        Sources/Openbar/Core/AlertSoundStrings.swift
        Sources/Openbar/Core/BrightnessStrings.swift
        Sources/Openbar/Core/MediaImageStrings.swift
        Sources/Openbar/Core/QuickToggleStrings.swift
        Sources/Openbar/Core/ScreenshotStrings.swift
        Sources/Openbar/Core/RecentCaptureStrings.swift
        Sources/Openbar/Core/RecorderStrings.swift
        Sources/Openbar/Core/RecorderShareStrings.swift
        Sources/Openbar/Core/CameraPreviewStrings.swift
        Sources/Openbar/Core/WallpaperStrings.swift
        Sources/Openbar/Services/Wallpaper/WallpaperSupport.swift
        Sources/Openbar/Core/ScratchpadStrings.swift
        Sources/Openbar/Core/FinderRenameStrings.swift
        Sources/Openbar/Core/CommandBarStrings.swift
        Sources/Openbar/Core/FeedbackStrings.swift
        Sources/Openbar/Core/RadialMenuStrings.swift
        Sources/Openbar/Core/MenuBarAppearanceStrings.swift
        Sources/Openbar/Core/AppAppearance.swift
        Sources/Openbar/Core/AppearanceStrings.swift
        Sources/Openbar/Core/GeneralSettingsStrings.swift
        Sources/Openbar/Core/SettingsPageStrings.swift
        Sources/Openbar/Core/BatteryTimeStrings.swift
        Sources/Openbar/Core/CPUCoreStrings.swift
        Sources/Openbar/Core/MonitorLayoutStrings.swift
        Sources/Openbar/Core/KeepAwakeStrings.swift
        Sources/Openbar/Core/BluetoothSleepStrings.swift
        Sources/Openbar/Core/PermissionGuideStrings.swift
        Sources/Openbar/Core/FanControlStrings.swift
        Sources/Openbar/Core/ConnectedDevicesStrings.swift
        Sources/Openbar/Services/FanControl/FanControlSupport.swift
        Sources/Openbar/Services/FanControl/FanControlResumeSupport.swift
        Sources/Openbar/Services/Snippets/TextSnippetSupport.swift
        Sources/Openbar/Services/RadialMenu/RadialMenuSupport.swift
        Sources/Openbar/Services/QuickTools/ScratchpadSupport.swift
        Sources/Openbar/Services/QuickTools/ScratchpadStore.swift
        Sources/Openbar/Services/KillProcess/KillProcessSupport.swift
        Sources/Openbar/Services/Recorder/RecorderSupport.swift
        Sources/Openbar/Services/Recorder/RecorderSampleTiming.swift
        Sources/Openbar/Services/Recorder/RecorderWriter.swift
        Sources/Openbar/Services/Recorder/RecorderCaptureEngine.swift
        Sources/Openbar/Core/RecorderExportStrings.swift
        Sources/Openbar/Services/Recorder/RecorderComposer.swift
        Sources/Openbar/Services/Recorder/RecorderComposerPlan.swift
        Sources/Openbar/Services/Recorder/RecorderCursorSprite.swift
        Sources/Openbar/Services/Recorder/RecorderTextRenderer.swift
        Sources/Openbar/Services/Recorder/RecorderImageRenderer.swift
        Sources/Openbar/Services/Recorder/RecorderExporter.swift
        Sources/Openbar/Services/Recorder/RecorderGIFClipboard.swift
        Sources/Openbar/Services/Recorder/RecorderGIFWriter.swift
        Sources/Openbar/Services/Recorder/RecorderGIFWorkspace.swift
        Sources/Openbar/Services/Recorder/RecorderComposition.swift
        Sources/Openbar/Services/Recorder/RecordingSharingSupport.swift
        Sources/Openbar/Services/PrivateFileStore.swift
        Sources/Openbar/Services/Recorder/RecorderTakeStore.swift
        Sources/Openbar/Services/Recorder/RecorderPresetImageStore.swift
        Sources/Openbar/Services/Recorder/RecorderMotion.swift
        Sources/Openbar/Services/Recorder/RecorderPointerTrack.swift
        Sources/Openbar/Services/Recorder/RecorderTypingTrack.swift
        Sources/Openbar/Services/Recorder/RecorderTimeline.swift
        Sources/Openbar/Services/Recorder/RecorderTextOverlay.swift
        Sources/Openbar/Services/Recorder/RecorderImageOverlay.swift
        Sources/Openbar/Services/Recorder/RecorderBlurRegion.swift
        Sources/Openbar/Services/Recorder/RecorderEditDocument.swift
        Sources/Openbar/Core/AppInfo.swift
        Sources/Openbar/Core/GlobalShortcut.swift
        Sources/Openbar/Core/SymbolicHotKeys.swift
        Sources/Openbar/Services/SystemShortcutTakeoverSupport.swift
        Sources/Openbar/Core/Localization.swift
        Sources/Openbar/Core/Localizations/Strings+*.swift
        Sources/Openbar/Core/FeatureStrings.swift
        Sources/Openbar/Core/KillProcessStrings.swift
        Sources/Openbar/Core/PortManagerStrings.swift
        Sources/Openbar/Core/WhatsAppDownloadStrings.swift
        Sources/Openbar/Core/WhatsAppOrganizerStrings.swift
        Sources/Openbar/Core/ReleaseNotes.swift
        Sources/Openbar/Core/ReleaseDigest.swift
        Sources/Openbar/Core/ReleaseChangeAliases.swift
        Sources/Openbar/Core/WhatsNewStrings.swift
        Sources/Openbar/Core/URLCleaning.swift
        Sources/Openbar/Services/GeneralPasteboardAccess.swift
        Sources/Openbar/Services/Clipboard/ClipboardHistoryWrite.swift
        Sources/Openbar/Services/Audio/AirPlayRouteManager.swift
        Sources/Openbar/Services/Audio/MixerRoutingSupport.swift
        Sources/Openbar/Services/Audio/MusicLaunchSupport.swift
        Sources/Openbar/Services/Bluetooth/BluetoothSleepSupport.swift
        Sources/Openbar/UI/MenuPanel/MixerPercentNativeTextField.swift
        Sources/Openbar/UI/MenuPanel/MixerAppDragSource.swift
        Sources/Openbar/Services/Audio/BoostLimiter.swift
        Sources/Openbar/Services/Audio/MixerRender.swift
        Sources/Openbar/Services/Audio/PreciseVolumeRollerSupport.swift
        Sources/Openbar/Services/DockPreview/DockPreviewSupport.swift
        Sources/Openbar/Services/DockPreview/DockAutohideHold.swift
        Sources/Openbar/Services/SpacesOrder/SpacesOrderHold.swift
        Sources/Openbar/Services/Homebrew/HomebrewSupport.swift
        Sources/Openbar/Services/Homebrew/HomebrewEnvironment.swift
        Sources/Openbar/Services/AppUpdates/AppUpdatesSupport.swift
        Sources/Openbar/Services/AppUpdates/AppUpdateFeedSupport.swift
        Sources/Openbar/Core/AppUpdateStrings.swift
        Sources/Openbar/Core/DiskImageInstallerStrings.swift
        Sources/Openbar/Services/DiskImageInstaller/DiskImageInstallerSupport.swift
        Sources/Openbar/UI/NonModalAlert.swift
        Sources/Openbar/Services/Clipboard/ClipboardHistorySupport.swift
        Sources/Openbar/Core/ColorValue.swift
        Sources/Openbar/Services/Clipboard/ClipboardAutoClearSupport.swift
        Sources/Openbar/Services/AutoQuit/AutoQuitSupport.swift
        Sources/Openbar/Services/Shelf/ShelfSupport.swift
        Sources/Openbar/Services/Shelf/ShelfFilePromiseTransfer.swift
        Sources/Openbar/Core/ShelfPromiseDeliveryStrings.swift
        Sources/Openbar/Services/Finder/FinderRenameSupport.swift
        Sources/Openbar/Services/Update/UpdateInstallerSupport.swift
        Sources/Openbar/Services/Update/UpdateServiceSupport.swift
        Sources/Openbar/Services/InstalledApps.swift
        Sources/Openbar/Services/LaunchAtLoginSupport.swift
        Sources/Openbar/UI/Settings/SettingsSearchSupport.swift
        Sources/Openbar/UI/Settings/SettingsSidebarSupport.swift
        Sources/Openbar/UI/Settings/FeatureVisibilitySupport.swift
        Sources/Openbar/UI/Onboarding/WhatsNewSupport.swift
        Sources/Openbar/UI/Settings/SettingsWindow.swift
        Sources/Openbar/Core/SettingsNavigationStrings.swift
        Sources/Openbar/App/MenuBarSpacingSupport.swift
        Sources/Openbar/App/MenuBarAllowanceSupport.swift
        Sources/Openbar/App/ReopenRequestSupport.swift
        Sources/Openbar/App/StatusItemAnchorSupport.swift
        Sources/Openbar/Services/DockClick/DockClickSupport.swift
        Sources/Openbar/Services/Finder/CutPasteProgressSupport.swift
        Sources/Openbar/Services/Finder/CutPastePrivilegeSupport.swift
        Sources/Openbar/Services/Finder/FinderPasteImageSupport.swift
        Sources/Openbar/Services/MiddleClick/MiddleClickSupport.swift
        Sources/Openbar/Services/MouseNavigation/MouseNavigationSupport.swift
        Sources/Openbar/Services/MouseNavigation/MouseNavigationKeys.swift
        Sources/Openbar/Services/MouseButtons/MouseButtonShortcutSupport.swift
        Sources/Openbar/Services/MouseButtons/MouseSpacesGestureSupport.swift
        Sources/Openbar/Services/MouseClickDebounce/MouseClickDebounceSupport.swift
        Sources/Openbar/Services/MouseExceptions/MouseAppExceptionSupport.swift
        Sources/Openbar/Services/MouseExceptions/MouseAppExceptions.swift
        Sources/Openbar/Services/WindowServerSupport.swift
        Sources/Openbar/Services/WindowMaximizerSupport.swift
        Sources/Openbar/Core/MouseButtonStrings.swift
        Sources/Openbar/Core/MouseClickDebounceStrings.swift
        Sources/Openbar/Core/MouseExceptionStrings.swift
        Sources/Openbar/Core/ClipboardIgnoredAppsStrings.swift
        Sources/Openbar/Core/ClipboardRetentionStrings.swift
        Sources/Openbar/Core/WindowLayoutIgnoredAppsStrings.swift
        Sources/Openbar/Services/WindowLayout/WindowLayoutIgnoredApps.swift
        Sources/Openbar/Core/WindowPreviewExclusionStrings.swift
        Sources/Openbar/Core/WindowMaximizerExclusionStrings.swift
        Sources/Openbar/Core/DiskExclusionStrings.swift
        Sources/Openbar/Core/SwitcherAppRulesStrings.swift
        Sources/Openbar/Services/QuickTools/QuickToolsSupport.swift
        Sources/Openbar/Services/CommandBar/CommandBarSupport.swift
        Sources/Openbar/Services/CommandBar/CommandBarPreferences.swift
        Sources/Openbar/Services/CommandBar/CommandBarMath.swift
        Sources/Openbar/Services/CommandBar/CommandBarUnits.swift
        Sources/Openbar/Services/CommandBar/CommandBarColors.swift
        Sources/Openbar/Services/CommandBar/CommandBarEmoji.swift
        Sources/Openbar/Services/CommandBar/CommandBarLinks.swift
        Sources/Openbar/Services/CommandBar/CommandBarDates.swift
        Sources/Openbar/Services/CommandBar/CommandBarRowShortcuts.swift
        Sources/Openbar/Services/CommandBar/CommandBarSystemSettingsSupport.swift
        Sources/Openbar/Services/CommandBar/CommandBarFileSearchSupport.swift
        Sources/Openbar/Services/CommandBar/CommandBarQueryMemory.swift
        Sources/Openbar/Services/SpotlightNamesSupport.swift
        Sources/Openbar/Services/QuickTools/MicMuteSupport.swift
        Sources/Openbar/Services/QuickTools/QuickTogglesSupport.swift
        Sources/Openbar/Services/QuickTools/ScreenshotCapturePolicy.swift
        Sources/Openbar/Services/QuickTools/ScreenshotSupport.swift
        Sources/Openbar/Services/QuickTools/ScreenshotCanvasSupport.swift
        Sources/Openbar/UI/Settings/ScreenCaptureToolPicker.swift
        Sources/Openbar/UI/Settings/MonitorToken.swift
        Sources/Openbar/Services/QuickTools/ScreenshotRenderer.swift
        Sources/Openbar/Services/QuickTools/ScreenshotImageLayerRendering.swift
        Sources/Openbar/Services/QuickTools/RecentCaptureStore.swift
        Sources/Openbar/Services/QuickTools/ScreenshotSharingSupport.swift
        Sources/Openbar/Services/QuickTools/WindowActivationPolicy.swift
        Sources/Openbar/Services/KeyboardDebounce/KeyboardDebounceSupport.swift
        Sources/Openbar/Services/SuperKey/SuperKeySupport.swift
        Sources/Openbar/Services/SuperKey/SuperKeyMappingGuard.swift
        Sources/Openbar/Core/SuperKeyStrings.swift
        Sources/Openbar/Core/InputSourceSelection.swift
        Sources/Openbar/Services/SessionActivity.swift
        Sources/Openbar/Services/SessionActivitySupport.swift
        Sources/Openbar/Services/EventTimestamp.swift
        Sources/Openbar/Services/OwnKeyEvent.swift
        Sources/Openbar/Services/ScrollWheelSupport.swift
        Sources/Openbar/Services/HorizontalWheelScrolling.swift
        Sources/Openbar/Services/SmoothScrollSupport.swift
        Sources/Openbar/Services/MouseAcceleration/MouseAccelerationSupport.swift
        Sources/Openbar/Services/FocusFollowsMouse/FocusFollowsMouseSupport.swift
        Sources/Openbar/Services/AssistiveKeyboard.swift
        Sources/Openbar/Services/Switcher/SwitcherModels.swift
        Sources/Openbar/Services/Switcher/WindowServerCaptureQueue.swift
        Sources/Openbar/Services/Switcher/SwitcherSupport.swift
        Sources/Openbar/Services/Switcher/SpaceHopSupport.swift
        Sources/Openbar/Services/Switcher/WindowUseOrder.swift
        Sources/Openbar/Services/Metrics/MetricFormat.swift
        Sources/Openbar/Services/Metrics/VMStatisticsDecoder.swift
        Sources/Openbar/Services/KeepAwakeAutomationSupport.swift
        Sources/Openbar/Services/SudoersSupport.swift
        Sources/Openbar/Services/Metrics/BatteryTimeSupport.swift
        Sources/Openbar/Services/Metrics/BatteryPowerSupport.swift
        Sources/Openbar/Services/BoundedProcessRunner.swift
        Sources/Openbar/Services/DetachedProcess.swift
        Sources/Openbar/Services/ShellSupport.swift
        Sources/Openbar/Services/PortManager/PortManagerSupport.swift
        Sources/Openbar/Services/Metrics/NetworkProcessSupport.swift
        Sources/Openbar/Services/Metrics/NetworkSampler.swift
        Sources/Openbar/Services/Metrics/NetworkAddressService.swift
        Sources/Openbar/Services/Metrics/SpeedTest.swift
        Sources/Openbar/Services/Metrics/PeripheralBatterySampler.swift
        Sources/Openbar/Services/Metrics/PeripheralBatterySupport.swift
        Sources/Openbar/Services/Metrics/DiskSupport.swift
        Sources/Openbar/Services/Metrics/MonitorSamplingPolicy.swift
        Sources/Openbar/Services/Metrics/USBDeviceSampler.swift
        Sources/Openbar/Services/Metrics/CPUCoreSampler.swift
        Sources/Openbar/Services/Metrics/MaxCapacityProbe.swift
        Sources/Openbar/Services/Metrics/TemperatureSensorSelector.swift
        Sources/Openbar/Services/Metrics/SustainedAlertGate.swift
        Sources/Openbar/Services/WindowLayout/WindowLayoutSupport.swift
        Sources/Openbar/Services/WindowLayout/WindowGestureSupport.swift
        Sources/Openbar/Core/WindowDirectionalStrings.swift
        Sources/Openbar/Core/PointerDisplayStrings.swift
        Sources/Openbar/Core/GraphScaleStrings.swift
        Sources/Openbar/Core/NotchLowBatteryStrings.swift
        Sources/Openbar/Services/CleaningMode/CleaningUnlockCounter.swift
        Sources/Openbar/Services/CleaningMode/CleaningMouseReleaseGate.swift
        Sources/Openbar/Services/Display/ExtraBrightnessSupport.swift
        Sources/Openbar/Services/Display/BrightnessSupport.swift
        Sources/Openbar/Services/Display/LidDimmingSupport.swift
        Sources/Openbar/Services/Cleaner/CleanerSupport.swift
        Sources/Openbar/Services/Cleaner/CleanerPolicy.swift
        Sources/Openbar/Services/Cleaner/CleanerSchedule.swift
        Sources/Openbar/Services/Uninstall/UninstallerSupport.swift
        Sources/Openbar/Services/ManagedDownloads/WhatsAppDownloadSupport.swift
        Sources/Openbar/Core/SecureInputSupport.swift
        Tests/*.swift
        build/generated-tests/*.swift
    )
    TEST_OUTPUT_FILE_MAP="$TEST_OBJECT_DIR/output-file-map.json"
    write_swift_output_file_map "$TEST_OUTPUT_FILE_MAP" "$TEST_OBJECT_DIR" "${TEST_SOURCES[@]}"
    echo "▸ Building & running tests against $(basename "$SDK")…"
    swiftc -Onone -incremental -enable-batch-mode -j "$(sysctl -n hw.logicalcpu)" \
        -module-name OpenbarTests -output-file-map "$TEST_OUTPUT_FILE_MAP" \
        -target "$TARGET" -sdk "$SDK" "${SDK_COMPAT_FLAGS[@]}" \
        "${VM_STATISTICS_COMPAT_FLAGS[@]}" "${TEST_SOURCES[@]}" -o build/metrics-tests
    test_status=0
    ./build/metrics-tests "${TEST_ARGS[@]}" || test_status=$?
    if (( ${#TEST_ARGS} == 0 )); then
        ./Tests/PreferenceCleanupTests.sh || test_status=1
        ./Tests/UninstallSpacesTests.sh || test_status=1
        ./Tests/DeveloperInstallIsolationTests.sh || test_status=1
    fi
    discard_test_preferences || test_status=1
    exit $test_status
fi

echo "▸ Compiling ($BUILD_CONFIGURATION) against $(basename "$SDK")…"
APP_SOURCES=(Sources/Openbar/**/*.swift)
if (( ! DEV )); then
    # A release starts from an empty build directory, so nothing an earlier
    # build left behind (an old icon catalog, a staged bundle) can reach it.
    # Only the compiler's incremental records stay: they rebuild whatever
    # changed since, and a fresh checkout has none.
    find build -mindepth 1 -maxdepth 1 ! -name objects -exec rm -rf {} + 2>/dev/null || true
fi
APP_OBJECT_DIR="build/objects/$EXECUTABLE"
mkdir -p build "$APP_OBJECT_DIR"
APP_OUTPUT_FILE_MAP="$APP_OBJECT_DIR/output-file-map.json"
write_swift_output_file_map "$APP_OUTPUT_FILE_MAP" "$APP_OBJECT_DIR" "${APP_SOURCES[@]}"
# Without -j the driver compiles one file at a time, and without batch mode
# each file's compiler parses the whole module again: a clean release took a
# quarter of an hour. Batches share that work and run on every core, and the
# optimization stays per file, as before.
swiftc "${APP_OPTIMIZATION_FLAGS[@]}" -incremental -enable-batch-mode -j "$(sysctl -n hw.logicalcpu)" \
    -output-file-map "$APP_OUTPUT_FILE_MAP" \
    -target "$TARGET" -sdk "$SDK" "${SDK_COMPAT_FLAGS[@]}" "${VM_STATISTICS_COMPAT_FLAGS[@]}" "${HID_EVENT_SYSTEM_FLAGS[@]}" \
    "${BUILD_VARIANT_FLAGS[@]}" \
    "${APP_SOURCES[@]}" -o "build/$EXECUTABLE"

echo "▸ Compiling protected fan helper…"
swiftc -O -target "$TARGET" -sdk "$SDK" "${SDK_COMPAT_FLAGS[@]}" "${BUILD_VARIANT_FLAGS[@]}" \
    Sources/Openbar/Services/FanControl/FanControlSupport.swift \
    Sources/Openbar/Services/FanControl/FanControlXPC.swift \
    Sources/Openbar/Services/SystemMonitor/SMCClient.swift \
    Sources/Openbar/Services/Metrics/TemperatureSensorSelector.swift \
    Sources/Openbar/Services/FanControl/FanControlHardware.swift \
    Sources/FanControlHelper/main.swift \
    -o "build/$FAN_HELPER_ID"
"build/$FAN_HELPER_ID" --selftest

echo "▸ Compiling Now Playing adapter…"
swiftc -O -target "$TARGET" -sdk "$SDK" "${SDK_COMPAT_FLAGS[@]}" -emit-library \
    -module-name OpenbarNowPlaying \
    Sources/NowPlayingAdapter/NowPlayingAdapter.swift \
    Sources/NowPlayingAdapter/NowPlayingQueue.swift \
    Sources/NowPlayingAdapter/NowPlayingSelection.swift \
    Sources/Openbar/Services/Notch/NotchPlaybackSource.swift \
    Sources/Openbar/Services/Notch/NotchPlaybackCommand.swift \
    -o "build/$NOW_PLAYING_ADAPTER"

echo "▸ Generating app icon…"
swift Tools/MakeIcon.swift build/AppIcon.iconset
xattr -c -r build/AppIcon.iconset build/AppIcon.icns build/MenuBarIcon.png build/MenuBarIcon@2x.png build/BrandMark.png 2>/dev/null || true
ACTOOL_BIN="$(xcrun --find actool 2>/dev/null || true)"
ICON_TMP="$(mktemp -d)"
ADAPTIVE_SKIP=""
if [[ -z "$ACTOOL_BIN" ]]; then
    ADAPTIVE_SKIP="actool not found (adaptive icons need Xcode 26+)"
else
    echo "▸ Compiling adaptive icon catalog…"
    # actool crashes on File Provider-synced paths, so compile a local copy.
    ditto "Resources/Brand/AppIcon.icon" "$ICON_TMP/AppIcon.icon"
    # Xcode 27 beta actool requires the --compile target directory to already exist.
    mkdir -p "$ICON_TMP/catalog"
    if "$ACTOOL_BIN" "$ICON_TMP/AppIcon.icon" \
            --compile "$ICON_TMP/catalog" \
            --app-icon AppIcon \
            --platform macosx \
            --target-device mac \
            --minimum-deployment-target 14.0 \
            --enable-on-demand-resources NO \
            --output-partial-info-plist "$ICON_TMP/partial-info.plist" \
            >"$ICON_TMP/actool.log" 2>&1 && [[ -s "$ICON_TMP/catalog/Assets.car" ]]; then
        mv "$ICON_TMP/catalog/Assets.car" build/Assets.car
    else
        ADAPTIVE_SKIP="actool could not compile the catalog"
    fi
fi
if [[ -n "$ADAPTIVE_SKIP" ]]; then
    cp "$ICON_TMP/actool.log" build/actool-failure.log 2>/dev/null || true
    echo "  adaptive icon skipped: $ADAPTIVE_SKIP (Dock falls back to AppIcon.icns)"
fi
echo "▸ Assembling and signing bundle…"
STAGE_TMP="$(mktemp -d)"
STAGE="$STAGE_TMP/$APP_NAME.app"
mkdir -p "$STAGE/Contents/MacOS" "$STAGE/Contents/Resources" \
    "$STAGE/Contents/Library/LaunchDaemons" "$STAGE/Contents/Library/LaunchServices"
cp "build/$EXECUTABLE" "$STAGE/Contents/MacOS/$EXECUTABLE"
cp "build/$FAN_HELPER_ID" "$STAGE/Contents/Library/LaunchServices/$FAN_HELPER_ID"
mkdir -p "$STAGE/Contents/Frameworks"
cp "build/$NOW_PLAYING_ADAPTER" "$STAGE/Contents/Frameworks/$NOW_PLAYING_ADAPTER"
cp Resources/now-playing.pl "$STAGE/Contents/Resources/now-playing.pl"
cp Resources/agent-prices.json "$STAGE/Contents/Resources/agent-prices.json"
cp Resources/remote-agent.sh "$STAGE/Contents/Resources/remote-agent.sh"
# The Antigravity extension the Sessions page types replies through. A copy
# also sits next to the bare binary, where `build/Openbar --selftest` looks.
ditto Tools/AntigravityBridge "$STAGE/Contents/Resources/AntigravityBridge"
rm -rf build/AntigravityBridge
ditto Tools/AntigravityBridge build/AntigravityBridge
cp Resources/com.openbar.app.fan-control.plist \
    "$STAGE/Contents/Library/LaunchDaemons/$FAN_HELPER_ID.plist"
cp Resources/Info.plist "$STAGE/Contents/Info.plist"
cp CHANGELOG.md "$STAGE/Contents/Resources/CHANGELOG.md"
for lproj in Resources/*.lproj(N); do
    cp -R "$lproj" "$STAGE/Contents/Resources/"
done
if (( DEV )); then
    # A distinct identity so the Developer build installs and runs next to the
    # official app, with its own permissions, preferences and login item.
    /usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $APP_BUNDLE_ID" "$STAGE/Contents/Info.plist"
    /usr/libexec/PlistBuddy -c "Set :CFBundleName $APP_NAME" "$STAGE/Contents/Info.plist"
    /usr/libexec/PlistBuddy -c "Set :CFBundleDisplayName $APP_NAME" "$STAGE/Contents/Info.plist"
    /usr/libexec/PlistBuddy -c "Set :CFBundleExecutable $EXECUTABLE" "$STAGE/Contents/Info.plist"
    FAN_PLIST="$STAGE/Contents/Library/LaunchDaemons/$FAN_HELPER_ID.plist"
    /usr/libexec/PlistBuddy -c "Set :Label $FAN_HELPER_ID" "$FAN_PLIST"
    /usr/libexec/PlistBuddy -c "Set :BundleProgram Contents/Library/LaunchServices/$FAN_HELPER_ID" "$FAN_PLIST"
    /usr/libexec/PlistBuddy -c "Delete :MachServices:com.openbar.app.fan-control" "$FAN_PLIST"
    /usr/libexec/PlistBuddy -c "Add :MachServices:$FAN_HELPER_ID bool true" "$FAN_PLIST"
    # Stamp the source commit + build time so the running dev app shows (in About)
    # exactly which code it was compiled from. Lets you verify it matches HEAD before
    # testing, instead of unknowingly running a stale build. Dev-only; never shipped.
    SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
    [[ -n "$(git status --porcelain 2>/dev/null)" ]] && SHA="$SHA-dirty"
    /usr/libexec/PlistBuddy -c "Add :OpenbarBuildCommit string '$SHA · $(date '+%Y-%m-%d %H:%M')'" "$STAGE/Contents/Info.plist"
    echo "  stamped dev build: $SHA"
fi
FAN_HELPER_VERSION="$(
    export LC_ALL=C
    /usr/bin/shasum -a 256 \
        "$STAGE/Contents/Library/LaunchServices/$FAN_HELPER_ID" \
        "$STAGE/Contents/Library/LaunchDaemons/$FAN_HELPER_ID.plist" \
        | /usr/bin/awk '{print $1}' | /usr/bin/shasum -a 256 \
        | /usr/bin/awk '{print $1}'
)"
/usr/libexec/PlistBuddy -c "Add :OpenbarFanControlHelperVersion string '$FAN_HELPER_VERSION'" \
    "$STAGE/Contents/Info.plist"
printf 'APPL????' > "$STAGE/Contents/PkgInfo"
cp build/AppIcon.icns "$STAGE/Contents/Resources/AppIcon.icns"
cp build/MenuBarIcon.png build/MenuBarIcon@2x.png build/BrandMark.png "$STAGE/Contents/Resources/"
if [[ -f build/Assets.car ]]; then
    cp build/Assets.car "$STAGE/Contents/Resources/Assets.car"
fi
if [[ -d Resources/Gifs ]]; then
    mkdir -p "$STAGE/Contents/Resources/Gifs"
    cp Resources/Gifs/*.gif "$STAGE/Contents/Resources/Gifs/"
fi
if ! cmp -s Resources/Gifs/highlights-notch.gif "$STAGE/Contents/Resources/Gifs/highlights-notch.gif"; then
    echo "Dynamic Island tour GIF is missing or differs from the bundled copy" >&2
    exit 1
fi
if [[ -d Resources/Images ]]; then
    mkdir -p "$STAGE/Contents/Resources/Images"
    cp Resources/Images/* "$STAGE/Contents/Resources/Images/"
fi
xattr -c -r "$STAGE" 2>/dev/null || true

# Signing, in order of preference:
#   1. Developer ID Application — the real, Apple-issued identity used for
#      notarized releases. Signed with the hardened runtime (required for
#      notarization), the app's entitlements and a secure timestamp. Gives a
#      stable, team-based designated requirement, so permissions persist across
#      updates AND Gatekeeper shows no "unverified developer" warning.
#   2. "Openbar Signing" — the stable self-signed identity from
#      Tools/setup-signing.sh, so builds without a Developer ID still get a
#      constant designated requirement and keep their granted permissions.
#   3. Ad-hoc — fresh clone with no identity at all.
DEVID="$(developer_id_identity)"
codesign_app() {
    local target="$1"
    if [[ -n "$DEVID" ]]; then
        codesign_with_timestamp_retry --force --strip-disallowed-xattrs --options runtime --timestamp \
            --entitlements "$ENTITLEMENTS" --sign "$DEVID" "$target"
    elif legacy_identity_installed; then
        codesign --force --strip-disallowed-xattrs --sign "$LEGACY_IDENTITY" "$target"
    else
        codesign --force --strip-disallowed-xattrs --sign - "$target"
    fi
}

codesign_fan_helper() {
    local target="$1"
    if [[ -n "$DEVID" ]]; then
        codesign_with_timestamp_retry --force --strip-disallowed-xattrs --options runtime --timestamp \
            --identifier "$FAN_HELPER_ID" --sign "$DEVID" "$target"
    elif legacy_identity_installed; then
        codesign --force --strip-disallowed-xattrs --identifier "$FAN_HELPER_ID" \
            --sign "$LEGACY_IDENTITY" "$target"
    else
        codesign --force --strip-disallowed-xattrs --identifier "$FAN_HELPER_ID" --sign - "$target"
    fi
}

codesign_now_playing_adapter() {
    local target="$1"
    if [[ -n "$DEVID" ]]; then
        codesign_with_timestamp_retry --force --strip-disallowed-xattrs --options runtime --timestamp \
            --identifier "$NOW_PLAYING_ADAPTER_ID" --sign "$DEVID" "$target"
    elif legacy_identity_installed; then
        codesign --force --strip-disallowed-xattrs --identifier "$NOW_PLAYING_ADAPTER_ID" \
            --sign "$LEGACY_IDENTITY" "$target"
    else
        codesign --force --strip-disallowed-xattrs --identifier "$NOW_PLAYING_ADAPTER_ID" --sign - "$target"
    fi
}

sign_bundle() {
    local bundle="$1"
    local executable="$bundle/Contents/MacOS/$EXECUTABLE"
    local helper="$bundle/Contents/Library/LaunchServices/$FAN_HELPER_ID"
    local adapter="$bundle/Contents/Frameworks/$NOW_PLAYING_ADAPTER"

    if [[ -n "$DEVID" ]]; then
        echo "  signing with Developer ID (hardened runtime): $DEVID"
    elif legacy_identity_installed; then
        echo "  signing with legacy self-signed identity: $LEGACY_IDENTITY"
    else
        echo "  signing ad-hoc (no identity installed — run Tools/setup-signing.sh)"
    fi
    [[ -f "$helper" ]] && codesign_fan_helper "$helper"
    [[ -f "$adapter" ]] && codesign_now_playing_adapter "$adapter"
    codesign_app "$bundle"

    # If local filesystem metadata invalidates the first signature, sign once
    # more. The installed Developer bundle is signed again after the final copy.
    if ! codesign --verify --deep --strict "$bundle" >/dev/null 2>&1; then
        echo "  re-signing after filesystem metadata settled"
        xattr -c -r "$bundle" 2>/dev/null || true
        [[ -f "$helper" ]] && codesign_fan_helper "$helper"
        [[ -f "$adapter" ]] && codesign_now_playing_adapter "$adapter"
        codesign_app "$bundle"
    fi
    [[ -f "$executable" ]] && codesign --verify --strict "$executable"
    [[ -f "$helper" ]] && codesign --verify --strict "$helper"
    [[ -f "$adapter" ]] && codesign --verify --strict "$adapter"
    codesign --verify --deep --strict "$bundle"
}

sign_installed_bundle() {
    local bundle="$1"
    wait_for_install_metadata "$bundle"
    sign_bundle "$bundle"
}

sign_bundle "$STAGE"

process_is_running() {
    local proc="$1"
    if (( ${#proc} > 15 )); then
        pgrep -f "/Contents/MacOS/$proc" >/dev/null 2>&1
    else
        pgrep -x "$proc" >/dev/null 2>&1
    fi
}

stop_process() {
    local proc="$1"
    if (( ${#proc} > 15 )); then
        pkill -f "/Contents/MacOS/$proc" 2>/dev/null || true
    else
        pkill -x "$proc" 2>/dev/null || true
    fi
    for _ in {1..50}; do
        if ! process_is_running "$proc"; then
            return 0
        fi
        sleep 0.1
    done
    echo "✗ $proc is still running — quit it and retry" >&2
    return 1
}

wait_for_install_metadata() {
    local bundle="$1"
    local missing
    for _ in {1..50}; do
        missing=0
        while IFS= read -r file; do
            if ! xattr -p com.apple.provenance "$file" >/dev/null 2>&1; then
                missing=1
                break
            fi
        done < <(find "$bundle/Contents" -type f ! -path "*/_CodeSignature/*")
        if (( missing == 0 )); then
            return 0
        fi
        sleep 0.1
    done
}

# Installed development builds only need the copy in /Applications. Retaining
# another app in each checkout pollutes application search with stale builds.
if (( DEV )); then
    for old_bundle in "build/stage/$APP_NAME.app" "build/stage.noindex/$APP_NAME.app"; do
        if [[ -d "$old_bundle" ]]; then
            /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister \
                -u "$PWD/$old_bundle" >/dev/null 2>&1 || true
            rm -rf "$old_bundle"
        fi
    done
fi

if (( !DEV || !INSTALL )); then
    STAGE_DIRECTORY="build/stage"
    (( DEV )) && STAGE_DIRECTORY="build/stage.noindex"
    mkdir -p "$STAGE_DIRECTORY"
    BUILD_STAGE="$STAGE_DIRECTORY/$APP_NAME.app"
    rm -rf "$BUILD_STAGE"
    ditto --noextattr --noqtn "$STAGE" "$BUILD_STAGE"
    xattr -c -r "$BUILD_STAGE" 2>/dev/null || true
    if ! codesign --verify --deep --strict "$BUILD_STAGE" >/dev/null 2>&1; then
        if xattr -lr "$BUILD_STAGE" 2>/dev/null | grep -Eq 'com\.apple\.(FinderInfo|ResourceFork|provenance|fileprovider)'; then
            echo "  staging copy has local filesystem metadata; temp bundle was verified"
        else
            codesign --verify --deep --strict "$BUILD_STAGE"
        fi
    fi
    echo "✓ Bundle ready: $BUILD_STAGE"
fi

if (( INSTALL )); then
    echo "▸ Installing into /Applications…"
    stop_process "$EXECUTABLE"
    # Only a release install replaces the pre-rename apps from before 2.5. The
    # Developer variant must coexist with them just as it does with Openbar.
    if (( ! DEV )); then
        for legacy in "Vorss:Vorss" "Vorssaint Utils:VorssaintUtils"; do
            name="${legacy%%:*}"; proc="${legacy##*:}"
            if [[ -d "/Applications/$name.app" ]]; then
                stop_process "$proc"
                rm -rf "/Applications/$name.app"
                echo "  (legacy $name.app removed)"
            fi
        done
    fi
    INSTALL_DEST="/Applications/$APP_NAME.app"
    rm -rf "$INSTALL_DEST"
    ditto --noextattr --noqtn "$STAGE" "$INSTALL_DEST"
    sign_installed_bundle "$INSTALL_DEST"
    echo "✓ Installed: $INSTALL_DEST"
fi
