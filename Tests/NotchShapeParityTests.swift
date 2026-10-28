// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation
import SwiftUI

/// Exercise the real contour, geometry and preference export at the slider limits.
enum NotchShapeParityTests {
    static func run(_ suite: TestSuite) {
        let screen = CGRect(x: 0, y: 0, width: 1440, height: 900)
        let automatic = NotchGeometry(screen: screen, safeAreaTop: 0, cameraWidth: 0)
        suite.expect(
            automatic.shapeFit == .standard && automatic.cameraWidth == 135
                && automatic.cameraHeight == 24,
            "existing installs retain the automatic simulated-notch dimensions")
        let hostile = NotchShapeFit(
            width: .nan, height: .infinity, roundness: .nan,
            shoulderWidth: -.infinity, shoulderHeight: .infinity)
        suite.expect(
            hostile == .standard,
            "non-finite stored values fall back to automatic dimensions and standard curves")
        let clamped = NotchShapeFit(
            width: -1, height: 900, roundness: 900, shoulderWidth: -1, shoulderHeight: 900)
        suite.expect(
            clamped.width == 100 && clamped.height == 64 && clamped.roundness == 2
                && clamped.shoulderWidth == 0 && clamped.shoulderHeight == 3,
            "stored values are clamped to the same bounds as the sliders")
        let physical = NotchGeometry(screen: screen, safeAreaTop: 32, cameraWidth: 180)
        for width in [100.0, 400] {
            for height in [20.0, 64] {
                for roundness in [0.0, 100, 200] {
                    for side in [0.0, 100, 300] {
                        for rise in [0.0, 100, 300] {
                            let fit = NotchShapeFit(
                                width: width, height: height, roundness: roundness,
                                shoulderWidth: side, shoulderHeight: rise)
                            let camera = NotchGeometry(
                                screen: screen, safeAreaTop: 32, cameraWidth: 180, shapeFit: fit)
                            suite.expect(
                                camera == physical,
                                "shape customization cannot shrink or reshape a physical camera cutout")
                            let geometry = NotchGeometry(
                                screen: screen, safeAreaTop: 0, cameraWidth: 0, shapeFit: fit)
                            suite.expect(
                                geometry.cameraWidth == width && geometry.cameraHeight == height,
                                "a simulated notch takes its chosen width and height")
                            for size in [
                                geometry.collapsed, geometry.compactMusicGeometry.compactActivitySize,
                                geometry.expanded,
                            ] {
                                let path = NotchShape.island(height: size.height, geometry: geometry)
                                    .path(in: CGRect(origin: .zero, size: size))
                                suite.expect(
                                    path.boundingRect == CGRect(origin: .zero, size: size),
                                    "every extreme contour stays inside its window and keeps the top edge")
                                suite.expect(
                                    path.contains(CGPoint(x: size.width / 2, y: size.height / 2)),
                                    "the center remains visible and hittable at every curve limit")
                            }
                            let expanded = geometry.expanded
                            let path = NotchShape.island(height: expanded.height, geometry: geometry)
                                .path(in: CGRect(origin: .zero, size: expanded))
                            suite.expect(
                                path.contains(
                                    CGPoint(
                                        x: geometry.horizontalInset + 1,
                                        y: expanded.height - NotchLayout.bottomInset - 1)),
                                "expanded content clears stretched shoulders and rounded bottom corners")
                            let capture = NotchCaptureControlsLayout(
                                geometry: geometry, titleWidth: 110,
                                capturesAudio: false, toolCount: 5)
                            let tileRoom = capture.size.width - capture.toolInset * 2
                            suite.expect(
                                CGFloat(capture.toolColumns) * 76
                                    + CGFloat(capture.toolColumns - 1) * NotchCaptureControlsLayout.buttonSpacing
                                    <= tileRoom,
                                "capture tools fit within the custom shoulder inset")
                            let rows = (5 + capture.toolColumns - 1) / capture.toolColumns
                            suite.expect(
                                capture.size.height >= capture.headerTop + capture.headerHeight + 12
                                    + CGFloat(rows) * NotchLayout.shortcutHeight
                                    + CGFloat(rows - 1) * NotchCaptureControlsLayout.buttonSpacing + 16,
                                "capture controls reserve enough height for every wrapped tool row")
                            let withAudio = NotchCaptureControlsLayout(
                                geometry: geometry, titleWidth: 110, capturesAudio: true, toolCount: 5)
                            suite.expect(
                                withAudio.audioStacksVertically == (geometry.contentWidth < 320)
                                    && withAudio.size.height - capture.size.height
                                        == (withAudio.audioStacksVertically ? 72 : 40),
                                "narrow capture controls stack audio switches and reserve their height")
                            let notice = NotchNotice(
                                event: .battery, title: "Charging", detail: "80%",
                                symbol: "battery.100percent.bolt")
                            let wing = notice.wings(in: geometry).leading
                            suite.expect(
                                notice.inset(wing: wing, geometry: geometry) <= wing - 18 - notice.cameraGap,
                                "notice content retains room beside enlarged shoulders")
                            let capsule = NotchGeometry(
                                screen: screen, safeAreaTop: 0, cameraWidth: 0,
                                silhouette: .capsule, shapeFit: fit)
                            suite.expect(
                                capsule.cameraWidth == 135 && capsule.cameraHeight == 24,
                                "simulated-notch dimensions do not resize the separate capsule fit")
                            let capsulePath = NotchShape.island(height: 24, geometry: capsule)
                                .path(in: CGRect(x: 0, y: 0, width: 135, height: 24))
                            suite.expect(
                                capsulePath.boundingRect
                                    == NotchLayout.capsuleBody(in: CGRect(x: 0, y: 0, width: 135, height: 24), gap: 2),
                                "capsule rounding preserves its body and hit contour")
                        }
                    }
                }
            }
        }
        let domain = "com.vorssaint.tests.shape-parity"
        let defaults = UserDefaults(suiteName: domain)!
        defaults.removePersistentDomain(forName: domain)
        defer { defaults.removePersistentDomain(forName: domain) }
        suite.expect(
            NotchShapeFit.current(in: defaults) == .standard, "missing preferences preserve automatic fit"
        )
        let settings: [(String, Double)] = [
            (DefaultsKey.notchSimulatedWidth, 240), (DefaultsKey.notchSimulatedHeight, 48),
            (DefaultsKey.notchShapeRoundness, 175), (DefaultsKey.notchShoulderWidth, 250),
            (DefaultsKey.notchShoulderHeight, 150),
        ]
        for (key, value) in settings { defaults.set(value, forKey: key) }
        suite.expect(
            NotchShapeFit.current(in: defaults)
                == NotchShapeFit(
                    width: 240, height: 48,
                    roundness: 175, shoulderWidth: 250, shoulderHeight: 150),
            "shape preferences survive rereading")
        let payload = SettingsBackupSupport.payload(
            appVersion: "test", valueFor: defaults.object(forKey:))
        let exported = payload[SettingsBackupSupport.settingsKey] as? [String: Any] ?? [:]
        suite.expect(
            settings.allSatisfy { (exported[$0.0] as? NSNumber)?.doubleValue == $0.1 },
            "portable shape preferences participate in settings backup")
        for language in AppLanguage.allCases {
            let text = FeatureStrings.notchShape(language)
            suite.expect(
                !text.hint.isEmpty && !text.shoulderHeight.isEmpty && !text.playerVolume.isEmpty,
                "shape and player-volume controls are localized in every supported language")
        }
        let track = NotchNotice(event: .track, title: "Song title", detail: "Artist", symbol: "music.note")
        let noticeSizes: [(CGFloat, CGFloat, CGFloat, CGFloat)] = [
            (20, 12, 10, 9), (32, 22, 12, 11), (64, 22, 12, 11),
        ]
        for (height, artwork, title, detail) in noticeSizes {
            let metrics = NotchNoticeMetrics(stripHeight: height)
            suite.expect(
                metrics.artwork == artwork && metrics.titleFont == title && metrics.detailFont == detail,
                "compact media artwork and text follow the visible strip height")
            let titleWidth = (track.title as NSString).size(withAttributes: [
                .font: NSFont.monospacedDigitSystemFont(ofSize: title, weight: .medium)
            ]).width
            let expectedWing = min(
                240,
                max(
                    NotchNotice.minimumWing,
                    ceil(titleWidth + artwork + 8) + 16 + track.cameraGap))
            suite.expect(
                track.preferredWings(wrapsMessage: false, stripHeight: height).leading == expectedWing,
                "music notice reserves the same artwork and font sizes that it draws")
        }
    }
}
