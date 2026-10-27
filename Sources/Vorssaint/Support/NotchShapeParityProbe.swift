// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

#if VORSSAINT_DEVELOPMENT
    import AppKit
    import SwiftUI

    /// Drive the actual Settings sliders and reset action into a real island window.
    /// Only an isolated preference domain is written; no desktop input is posted.
    enum NotchShapeParityProbe {
        private final class Model: ObservableObject {
            let domain = "com.vorssaint.tests.shape-ui.\(UUID().uuidString)"
            let defaults: UserDefaults
            let screen: CGRect
            weak var island: NotchWindowHost?

            init(screen: CGRect) {
                self.screen = screen
                defaults = UserDefaults(suiteName: domain)!
                defaults.register(defaults: Defaults.registeredDefaults)
                defaults.set(NotchSilhouette.notch.rawValue, forKey: DefaultsKey.notchSilhouette)
            }

            func number(_ key: String) -> Binding<Double> {
                Binding(
                    get: { self.defaults.double(forKey: key) },
                    set: {
                        self.objectWillChange.send()
                        self.defaults.set($0, forKey: key)
                        self.sync()
                    })
            }
            var silhouette: Binding<String> {
                Binding(
                    get: { self.defaults.string(forKey: DefaultsKey.notchSilhouette) ?? "notch" },
                    set: {
                        self.objectWillChange.send()
                        self.defaults.set($0, forKey: DefaultsKey.notchSilhouette)
                        self.sync()
                    })
            }
            func geometry(physical: Bool = false) -> NotchGeometry {
                NotchGeometry(
                    screen: screen, safeAreaTop: physical ? 32 : 0, cameraWidth: physical ? 180 : 0,
                    silhouette: NotchSilhouette(rawValue: silhouette.wrappedValue) ?? .notch,
                    capsuleFit: NotchCapsuleFit.current(in: defaults),
                    shapeFit: NotchShapeFit.current(in: defaults))
            }
            func sync() {
                let value = geometry()
                island?.present(
                    size: value.restingSize(showsContent: false), geometry: value, animated: false)
            }
        }

        private struct Controls: View {
            @ObservedObject var model: Model
            var body: some View {
                NotchShapeSettings(
                    silhouette: model.silhouette,
                    simulatedWidth: model.number(DefaultsKey.notchSimulatedWidth),
                    simulatedHeight: model.number(DefaultsKey.notchSimulatedHeight),
                    shapeRoundness: model.number(DefaultsKey.notchShapeRoundness),
                    shoulderWidth: model.number(DefaultsKey.notchShoulderWidth),
                    shoulderHeight: model.number(DefaultsKey.notchShoulderHeight),
                    capsuleFitWidth: model.number(DefaultsKey.notchCapsuleFitWidth),
                    capsuleFitHeight: model.number(DefaultsKey.notchCapsuleFitHeight),
                    capsuleFitDrop: model.number(DefaultsKey.notchCapsuleFitDrop)
                )
                .padding(20)
            }
        }

        private static func descendants(_ view: NSView) -> [NSView] {
            [view] + view.subviews.flatMap(descendants)
        }
        private static func buttons(_ element: AnyObject, label: String) -> [AnyObject] {
            var found: [AnyObject] = []
            if element.accessibilityRole?() == .button && element.accessibilityLabel?() == label {
                found.append(element)
            }
            for child in element.accessibilityChildren?() ?? [] {
                found += buttons(child as AnyObject, label: label)
            }
            return found
        }

        static func runAndExit() -> Never {
            NSApplication.shared.setActivationPolicy(.prohibited)
            guard let screen = NSScreen.main else {
                print("SHAPE PARITY FAILED: no display")
                exit(1)
            }
            let model = Model(screen: screen.frame)
            let initial = model.geometry()
            let island = NotchWindowHost(
                content: AnyView(Color.clear), geometry: initial,
                size: initial.restingSize(showsContent: false), background: { _ in AnyView(Color.black) })
            island.panel.alphaValue = 0
            island.panel.ignoresMouseEvents = true
            island.panel.orderFrontRegardless()
            model.island = island
            let host = NSHostingView(rootView: Controls(model: model))
            let window = NSWindow(
                contentRect: CGRect(x: 0, y: 0, width: 660, height: 760),
                styleMask: [.borderless], backing: .buffered, defer: false)
            window.isReleasedWhenClosed = false
            window.contentView = host
            window.alphaValue = 0
            window.orderFrontRegardless()
            let attribute = "AXEnhancedUserInterface"
            let getter = NSSelectorFromString("accessibilityAttributeValue:")
            let setter = NSSelectorFromString("accessibilitySetValue:forAttribute:")
            let previous =
                NSApp.perform(getter, with: attribute)?.takeUnretainedValue() as? NSNumber ?? false
            NSApp.perform(setter, with: true, with: attribute)
            func settle() {
                host.layoutSubtreeIfNeeded()
                RunLoop.main.run(until: Date().addingTimeInterval(0.08))
                host.layoutSubtreeIfNeeded()
            }
            settle()
            // AppKit creates the SwiftUI accessibility tree lazily; prime it before inspecting native controls.
            _ = buttons(host, label: FeatureStrings.notchShape(L10n.shared.language).reset)
            settle()
            var failures: [String] = []
            let text = FeatureStrings.notch(L10n.shared.language)
            let shape = FeatureStrings.notchShape(L10n.shared.language)
            var changes = 0
            func change(_ label: String, key: String, value: Double, range: ClosedRange<Double>) {
                let sliders = descendants(host).compactMap { $0 as? NSSlider }
                guard let slider = sliders.first(where: { $0.accessibilityLabel() == label }) else {
                    failures.append(
                        "missing native slider: \(label); found \(sliders.map { $0.accessibilityLabel() ?? "nil" })"
                    )
                    return
                }
                let fraction = (value - range.lowerBound) / (range.upperBound - range.lowerBound)
                slider.doubleValue = slider.minValue + fraction * (slider.maxValue - slider.minValue)
                slider.sendAction(slider.action, to: slider.target)
                settle()
                if abs(model.defaults.double(forKey: key) - value) > 0.1 {
                    failures.append("slider did not persist \(label)")
                }
                changes += 1
            }
            change(
                text.width, key: DefaultsKey.notchSimulatedWidth, value: 400,
                range: NotchShapeFit.widthRange)
            change(
                text.height, key: DefaultsKey.notchSimulatedHeight, value: 64,
                range: NotchShapeFit.heightRange)
            change(shape.shoulderWidth, key: DefaultsKey.notchShoulderWidth, value: 300, range: 0...300)
            change(shape.shoulderHeight, key: DefaultsKey.notchShoulderHeight, value: 300, range: 0...300)
            change(shape.corners, key: DefaultsKey.notchShapeRoundness, value: 200, range: 0...200)
            if island.targetSize != CGSize(width: 400, height: 64) {
                failures.append("native island did not adopt the chosen dimensions")
            }
            if model.geometry(physical: true).shapeFit != .standard {
                failures.append("custom sliders changed physical-camera geometry")
            }
            if let bitmap = host.bitmapImageRepForCachingDisplay(in: host.bounds) {
                host.cacheDisplay(in: host.bounds, to: bitmap)
                try? bitmap.representation(using: .png, properties: [:])?.write(
                    to: URL(fileURLWithPath: "/tmp/vorssaint-shape-parity.png"))
            }
            let resets = buttons(host, label: shape.reset)
            let pressed = resets.first?.accessibilityPerformPress?() ?? false
            settle()
            if !pressed || NotchShapeFit.current(in: model.defaults) != .standard
                || NotchCapsuleFit.current(in: model.defaults) != .zero
            {
                failures.append("the rendered Reset shape button did not restore shape preferences")
            }
            if island.targetSize != initial.restingSize(showsContent: false) {
                failures.append("reset did not restore the native island's automatic size")
            }
            let capsuleButtons = buttons(host, label: text.capsuleShape)
            let selectedCapsule = capsuleButtons.first?.accessibilityPerformPress?() ?? false
            settle()
            if !selectedCapsule || !model.geometry().floats {
                failures.append("the capsule preview did not select the capsule")
            }
            change(
                text.capsuleFit + ", " + text.fromTop, key: DefaultsKey.notchCapsuleFitDrop, value: 20,
                range: NotchCapsuleFit.dropRange)
            if abs(island.panel.frame.maxY - (screen.frame.maxY - 20)) > 0.5 {
                failures.append("the native capsule did not adopt its chosen top offset")
            }
            island.close()
            window.contentView = nil
            window.close()
            model.defaults.removePersistentDomain(forName: model.domain)
            NSApp.perform(setter, with: previous, with: attribute)
            print(
                "SHAPE PARITY \(failures.isEmpty ? "OK" : "FAILED") sliders=\(changes) reset=\(pressed) capsule=\(selectedCapsule)"
            )
            for failure in failures { print(failure) }
            exit(failures.isEmpty ? 0 : 1)
        }
    }
#endif
