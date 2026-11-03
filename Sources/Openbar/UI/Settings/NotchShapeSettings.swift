// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// Portable shape controls. Physical-camera calibration remains in NotchSettings.
struct NotchShapeSettings: View {
    @Binding var silhouette: String
    @Binding var simulatedWidth: Double
    @Binding var simulatedHeight: Double
    @Binding var shapeRoundness: Double
    @Binding var shoulderWidth: Double
    @Binding var shoulderHeight: Double
    @Binding var capsuleFitWidth: Double
    @Binding var capsuleFitHeight: Double
    @Binding var capsuleFitDrop: Double
    var showDisplayHint = false
    var showAllDisplays: () -> Void = {}
    @ObservedObject private var l10n = L10n.shared
    private var text: NotchStrings { FeatureStrings.notch(l10n.language) }
    private var shapeText: NotchShapeStrings { FeatureStrings.notchShape(l10n.language) }

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            // Only a display without a camera can float the island.
            Group {
                SettingsCard(title: text.withoutNotch) {
                    HStack(alignment: .top, spacing: 12) {
                        shapePreview(.notch)
                        shapePreview(.capsule)
                    }
                    Text(shapeText.hint).font(.caption).foregroundStyle(.secondary)
                    // An island kept on a camera, or away with the lid closed,
                    // shows neither shape, so the card says where to change that.
                    if showDisplayHint {
                        Text(text.withoutNotchUnreached).font(.caption).foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                        Button(text.showOnAllDisplays, action: showAllDisplays)
                    }
                }
                if NotchSilhouette(rawValue: silhouette) ?? .capsule == .notch {
                    SettingsCard(title: shapeText.fit) {
                        Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 10) {
                            shapeSlider(
                                text.width, value: $simulatedWidth, range: NotchShapeFit.widthRange, step: 5,
                                fallback: 135)
                            shapeSlider(
                                text.height, value: $simulatedHeight, range: NotchShapeFit.heightRange, step: 1,
                                fallback: 24)
                            shapeSlider(
                                shapeText.shoulderWidth, value: $shoulderWidth, range: 0...300, step: 5,
                                fallback: 100, percent: true)
                            shapeSlider(
                                shapeText.shoulderHeight, value: $shoulderHeight, range: 0...300, step: 5,
                                fallback: 100, percent: true)
                        }
                    }
                }
                if NotchSilhouette(rawValue: silhouette) ?? .capsule == .capsule {
                    SettingsCard(title: text.capsuleFit) {
                        Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 10) {
                            // Even widths keep the capsule centred on whole points.
                            fitSlider(
                                text.width, card: text.capsuleFit, value: $capsuleFitWidth,
                                range: NotchCapsuleFit.widthRange, step: 2)
                            fitSlider(
                                text.height, card: text.capsuleFit, value: $capsuleFitHeight,
                                range: NotchCapsuleFit.heightRange, step: 1)
                            fitSlider(
                                text.fromTop, card: text.capsuleFit, value: $capsuleFitDrop,
                                range: NotchCapsuleFit.dropRange, step: 1)
                        }
                        Text(text.capsuleFitHint).font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
            SettingsCard {
                Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 10) {
                    shapeSlider(
                        shapeText.corners, value: $shapeRoundness, range: 0...200, step: 5, fallback: 100,
                        percent: true)
                }
                Button(shapeText.reset) {
                    simulatedWidth = 0
                    simulatedHeight = 0
                    shapeRoundness = 100
                    shoulderWidth = 100
                    shoulderHeight = 100
                    capsuleFitWidth = 0
                    capsuleFitHeight = 0
                    capsuleFitDrop = 0
                }
                Text(shapeText.hint).font(.caption).foregroundStyle(.secondary)
            }
        }
    }

    private var shapeFit: NotchShapeFit {
        NotchShapeFit(
            width: simulatedWidth, height: simulatedHeight, roundness: shapeRoundness,
            shoulderWidth: shoulderWidth, shoulderHeight: shoulderHeight)
    }

    private func shapePreview(_ silhouette: NotchSilhouette) -> some View {
        let geometry = NotchGeometry(
            screen: CGRect(x: 0, y: 0, width: 1920, height: 1080),
            safeAreaTop: 0, cameraWidth: 0, silhouette: silhouette,
            capsuleFit: NotchCapsuleFit(
                width: capsuleFitWidth, height: capsuleFitHeight, drop: capsuleFitDrop),
            shapeFit: shapeFit)
        let size = geometry.restingSize(showsContent: false)
        let selected = (NotchSilhouette(rawValue: self.silhouette) ?? .capsule) == silhouette
        return Button {
            self.silhouette = silhouette.rawValue
        } label: {
            VStack(spacing: 8) {
                GeometryReader { proxy in
                    let scale = min(1, max(0, proxy.size.width - 16) / max(1, size.width))
                    NotchShape.island(height: size.height, geometry: geometry).fill(.black)
                        .frame(width: size.width, height: size.height)
                        .scaleEffect(scale, anchor: .top)
                        .position(x: proxy.size.width / 2, y: size.height / 2 + geometry.floatingDrop)
                }
                .frame(height: 86)
                .background(.primary.opacity(0.04), in: RoundedRectangle(cornerRadius: 10))
                Text(silhouette == .notch ? text.notchShape : text.capsuleShape).font(.caption)
            }
            .padding(8)
            .background(
                selected ? Color.accentColor.opacity(0.08) : .clear,
                in: RoundedRectangle(cornerRadius: 12)
            )
            .overlay {
                RoundedRectangle(cornerRadius: 12).strokeBorder(
                    selected ? Color.accentColor.opacity(0.7) : .clear)
            }
        }
        .buttonStyle(.plain)
        .accessibilityLabel(silhouette == .notch ? text.notchShape : text.capsuleShape)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    private func shapeSlider(
        _ title: String, value: Binding<Double>, range: ClosedRange<Double>, step: Double,
        fallback: Double, percent: Bool = false
    ) -> some View {
        let bounded = Binding(
            get: {
                NotchSize.clamped(
                    value.wrappedValue == 0 && range.lowerBound > 0 ? fallback : value.wrappedValue,
                    to: range, fallback: fallback)
            }, set: { value.wrappedValue = NotchSize.clamped($0, to: range, fallback: fallback) })
        return GridRow {
            Text(title).fixedSize().accessibilityHidden(true)
            Slider(value: bounded, in: range, step: step) { Text(title) }.labelsHidden()
            Text(
                value.wrappedValue == 0 && range.lowerBound > 0
                    ? text.automatic
                    : "\(Int(bounded.wrappedValue))" + (percent ? "%" : "")
            )
            .monospacedDigit().foregroundStyle(.secondary).frame(minWidth: 44)
            .fixedSize(horizontal: true, vertical: false)
        }
    }

    private func fitSlider(
        _ title: String, card: String, value: Binding<Double>, range: ClosedRange<Double>,
        step: Double
    ) -> some View {
        let bounded = Binding(
            get: { NotchSize.clamped(value.wrappedValue, to: range, fallback: 0) },
            set: { value.wrappedValue = NotchSize.clamped($0, to: range, fallback: 0) })
        let formatted = bounded.wrappedValue.formatted(
            .number.sign(strategy: .always(includingZero: false))
                .precision(.fractionLength(0...1)).locale(Locale(identifier: l10n.language.rawValue)))
        return GridRow {
            Text(title).fixedSize().accessibilityHidden(true)
            // Other cards have sliders with the same names.
            Slider(value: bounded, in: range, step: step) { Text("\(card), \(title)") }.labelsHidden()
                .accessibilityValue(formatted)
            Text(formatted).monospacedDigit().foregroundStyle(.secondary).frame(width: 38)
        }
    }
}
