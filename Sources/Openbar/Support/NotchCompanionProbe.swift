// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Openbar

#if OPENBAR_DEVELOPMENT
import AppKit
import QuartzCore
import SwiftUI

/// `--companion-probe <folder>` draws the companion with the app's own rig,
/// sprites and Settings page into PNGs, offscreen, and quits. It reads no
/// preference of the person running it and writes none: the Settings page
/// gets a scratch suite that is only registered, never saved.
enum NotchCompanionProbe {
    static func isRequested(by arguments: [String]) -> Bool { arguments.contains("--companion-probe") }

    static func runAndExit() {
        let arguments = CommandLine.arguments
        let folder = arguments.firstIndex(of: "--companion-probe")
            .flatMap { arguments.dropFirst($0 + 1).first }
            .map { URL(fileURLWithPath: $0) } ?? FileManager.default.temporaryDirectory
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        _ = NSApplication.shared
        NSApp.setActivationPolicy(.prohibited)
        var failures: [String] = []
        sheet(to: folder.appendingPathComponent("companion-moods.png"), failures: &failures)
        looks(to: folder.appendingPathComponent("companion-looks.png"), failures: &failures)
        settings(to: folder.appendingPathComponent("companion-settings.png"), failures: &failures)
        if failures.isEmpty {
            print("companion probe: ok, \(folder.path)")
            exit(0)
        }
        failures.forEach { print("companion probe: \($0)") }
        exit(1)
    }

    private static let scale: CGFloat = 2

    /// Draws `layer` (flipped, as the island lays the companion out) on black.
    private static func png(_ layer: CALayer, size: CGSize, to url: URL, background: CGColor? = CGColor(gray: 0, alpha: 1)) -> Bool {
        let width = Int(size.width * scale), height = Int(size.height * scale)
        guard let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                                      space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return false }
        if let background {
            context.setFillColor(background)
            context.fill(CGRect(x: 0, y: 0, width: width, height: height))
        }
        context.translateBy(x: 0, y: CGFloat(height))
        context.scaleBy(x: scale, y: -scale)
        layer.render(in: context)
        guard let image = context.makeImage(),
              let destination = CGImageDestinationCreateWithURL(url as CFURL, "public.png" as CFString, 1, nil)
        else { return false }
        CGImageDestinationAddImage(destination, image, nil)
        return CGImageDestinationFinalize(destination)
    }

    private static func rig(_ look: NotchMascotLook, size: CGFloat, mood: NotchMascotMood, yaw: Int? = nil,
                            at center: CGPoint, in parent: CALayer) {
        let rig = NotchMascotRig()
        rig.reduceMotion = true
        rig.configure(look: look, size: size, contentsScale: scale)
        rig.setMood(mood, animated: false)
        if let yaw { rig.showYaw(yaw) }
        rig.root.position = CGPoint(x: center.x, y: center.y - NotchMascotGeometry.figureOffset(look) * size)
        parent.addSublayer(rig.root)
    }

    /// Every mood, and every turn of the head, in the standard look.
    private static func sheet(to url: URL, failures: inout [String]) {
        let cell: CGFloat = 130, size: CGFloat = 84
        let moods = NotchMascotMood.allCases
        let columns = 6
        let rows = Int(ceil(Double(moods.count + NotchMascotArt.yaws.count) / Double(columns)))
        let canvas = CALayer()
        canvas.frame = CGRect(x: 0, y: 0, width: cell * CGFloat(columns), height: cell * CGFloat(rows))
        canvas.isGeometryFlipped = true
        for (index, mood) in moods.enumerated() {
            rig(.standard, size: size, mood: mood,
                at: CGPoint(x: cell * (CGFloat(index % columns) + 0.5), y: cell * (CGFloat(index / columns) + 0.55)), in: canvas)
        }
        for yaw in NotchMascotArt.yaws.indices {
            let index = moods.count + yaw
            rig(.standard, size: size, mood: .idle, yaw: yaw,
                at: CGPoint(x: cell * (CGFloat(index % columns) + 0.5), y: cell * (CGFloat(index / columns) + 0.55)), in: canvas)
        }
        if !png(canvas, size: canvas.bounds.size, to: url) { failures.append("could not write \(url.lastPathComponent)") }
    }

    /// Every material in every color, and every head item.
    private static func looks(to url: URL, failures: inout [String]) {
        let cell: CGFloat = 92, size: CGFloat = 60
        let palettes = NotchMascotPalette.allCases, textures = NotchMascotTexture.allCases
        let items = NotchMascotItem.allCases
        let canvas = CALayer()
        canvas.frame = CGRect(x: 0, y: 0, width: cell * CGFloat(palettes.count),
                              height: cell * CGFloat(textures.count + 1))
        canvas.isGeometryFlipped = true
        for (row, texture) in textures.enumerated() {
            for (column, palette) in palettes.enumerated() {
                let item = items[(row + column) % items.count]
                rig(NotchMascotLook(texture: texture, palette: palette, item: item), size: size, mood: .idle,
                    at: CGPoint(x: cell * (CGFloat(column) + 0.5), y: cell * (CGFloat(row) + 0.58)), in: canvas)
            }
        }
        for (column, item) in items.enumerated() {
            rig(NotchMascotLook(texture: .plush, palette: palettes[(column * 3) % palettes.count], item: item),
                size: size, mood: .happy,
                at: CGPoint(x: cell * (CGFloat(column) + 0.5), y: cell * (CGFloat(textures.count) + 0.58)), in: canvas)
        }
        if !png(canvas, size: canvas.bounds.size, to: url) { failures.append("could not write \(url.lastPathComponent)") }
    }

    /// The Settings page as it opens, from a scratch suite, light and dark.
    private static func settings(to url: URL, failures: inout [String]) {
        let suite = UserDefaults(suiteName: "com.openbar.tests.companion-probe")!
        suite.register(defaults: Defaults.registeredDefaults.merging([
            DefaultsKey.notchEnabled: true,
            DefaultsKey.notchMascotEnabled: true,
            DefaultsKey.notchSilhouette: NotchSilhouette.notch.rawValue,
        ]) { $1 })
        for appearance in [NSAppearance.Name.aqua, .darkAqua] {
            let view = NSHostingView(rootView: NotchMascotSettings().defaultAppStorage(suite))
            view.appearance = NSAppearance(named: appearance)
            view.frame = CGRect(x: 0, y: 0, width: 720, height: 1180)
            let window = NSWindow(contentRect: view.frame, styleMask: [.borderless], backing: .buffered, defer: false)
            window.appearance = view.appearance
            window.contentView = view
            view.layoutSubtreeIfNeeded()
            RunLoop.main.run(until: Date().addingTimeInterval(1.2))
            view.layoutSubtreeIfNeeded()
            guard let layer = view.layer else { failures.append("the Settings page has no layer"); continue }
            let name = appearance == .darkAqua ? url.deletingPathExtension().lastPathComponent + "-dark.png" : url.lastPathComponent
            let background = appearance == .darkAqua ? CGColor(gray: 0.12, alpha: 1) : CGColor(gray: 0.96, alpha: 1)
            if !png(layer, size: view.frame.size, to: url.deletingLastPathComponent().appendingPathComponent(name),
                    background: background) {
                failures.append("could not write \(name)")
            }
        }
    }
}
#endif
