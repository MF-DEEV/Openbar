// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// Shared look & feel: brand colors, card styling and the brand mark.
enum Theme {
    /// Near-black background behind the brand mark. Neutral greys into black, no
    /// colour cast, with just a hint of depth so the badge does not read as flat.
    static let spaceGradient = LinearGradient(
        colors: [Color(white: 0.10),
                 Color(white: 0.04),
                 Color.black],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
    )
}

/// The Graphite palette: the app's accent is the ink of the helmet mark, near
/// white on dark surfaces and near black on light ones, instead of whatever
/// accent the user picked in System Settings. Both colors resolve against the
/// appearance of the view that draws them, so a light panel and the black
/// island on screen at the same time each get the right one.
enum BrandPalette {
    static let accentDark = NSColor(srgbRed: 0.949, green: 0.949, blue: 0.953, alpha: 1)   // #F2F2F3
    static let accentLight = NSColor(srgbRed: 0.094, green: 0.094, blue: 0.102, alpha: 1)  // #18181A
    static let onAccentDark = NSColor(srgbRed: 0.067, green: 0.067, blue: 0.075, alpha: 1) // #111113
    static let onAccentLight = NSColor.white

    static let accent = NSColor(name: "OpenbarAccent") { appearance in
        isDark(appearance) ? accentDark : accentLight
    }

    /// Text and glyphs drawn on a solid accent fill.
    static let onAccent = NSColor(name: "OpenbarOnAccent") { appearance in
        isDark(appearance) ? onAccentDark : onAccentLight
    }

    /// Tint for the system's own controls: switches, prominent buttons,
    /// checkboxes, sliders. AppKit always draws a white switch knob and white
    /// prominent-button text, which vanish on the near-white dark accent, so
    /// in the dark these take a graphite grey instead.
    static let controlTintDark = NSColor(srgbRed: 0.557, green: 0.557, blue: 0.576, alpha: 1) // #8E8E93
    static let controlTint = NSColor(name: "OpenbarControlTint") { appearance in
        isDark(appearance) ? controlTintDark : accentLight
    }

    static func isDark(_ appearance: NSAppearance) -> Bool {
        let dark: [NSAppearance.Name] = [.darkAqua, .vibrantDark,
                                         .accessibilityHighContrastDarkAqua,
                                         .accessibilityHighContrastVibrantDark]
        let light: [NSAppearance.Name] = [.aqua, .vibrantLight,
                                          .accessibilityHighContrastAqua,
                                          .accessibilityHighContrastVibrantLight]
        guard let match = appearance.bestMatch(from: dark + light) else { return false }
        return dark.contains(match)
    }
}

extension Color {
    /// The app's accent. Use it in place of `Color.accentColor`, which follows
    /// the accent chosen in System Settings.
    static let brandAccent = Color(nsColor: BrandPalette.accent)
    /// Text and symbols drawn on a solid `brandAccent` fill.
    static let brandOnAccent = Color(nsColor: BrandPalette.onAccent)
    /// See `BrandPalette.controlTint`.
    static let brandControlTint = Color(nsColor: BrandPalette.controlTint)
}

extension View {
    /// Gives the system controls in a window the Graphite tint. Applied at
    /// each window's root, since `.tint` only reaches the views below it.
    func brandControlTint() -> some View {
        tint(Color.brandControlTint)
    }
}

enum PanelMetricColor {
    static func green(for scheme: ColorScheme) -> Color {
        scheme == .light ? Color(red: 0.00, green: 0.44, blue: 0.18) : .green
    }

    static func cyan(for scheme: ColorScheme) -> Color {
        scheme == .light ? Color(red: 0.00, green: 0.43, blue: 0.54) : .cyan
    }

    static func mint(for scheme: ColorScheme) -> Color {
        scheme == .light ? Color(red: 0.00, green: 0.44, blue: 0.40) : .mint
    }

    static func yellow(for scheme: ColorScheme) -> Color {
        scheme == .light ? Color(red: 0.56, green: 0.36, blue: 0.00) : .yellow
    }

    static func red(for scheme: ColorScheme) -> Color {
        scheme == .light ? Color(red: 0.68, green: 0.08, blue: 0.10) : .red
    }

    static func orange(for scheme: ColorScheme) -> Color {
        scheme == .light ? Color(red: 0.68, green: 0.30, blue: 0.00) : .orange
    }

    static func pink(for scheme: ColorScheme) -> Color {
        scheme == .light ? Color(red: 0.68, green: 0.06, blue: 0.34) : .pink
    }
}

private struct NotchPresentationKey: EnvironmentKey {
    static let defaultValue = false
}

extension EnvironmentValues {
    var notchPresentation: Bool {
        get { self[NotchPresentationKey.self] }
        set { self[NotchPresentationKey.self] = newValue }
    }
}

enum PanelSurface {
    /// Whether the menu popover hosts the panel across its whole balloon, so the
    /// panel's own surface can reach the arrow (#1030). Only macOS 26 lays the
    /// content out that way. On macOS 15, `hasFullSizeContent` publishes the
    /// full-size safe area but leaves the view at its content size in the frame's
    /// lower-left corner: the popover grows by that safe area, the panel sits off
    /// center, and a band of system material shows along the top and right edges.
    static var popoverHostsFullSizeContent: Bool {
        if #available(macOS 26.0, *) { return true }
        return false
    }

    static func baseFill(for scheme: ColorScheme) -> Color {
        scheme == .light ? Color.white.opacity(0.68) : Color.black.opacity(0.42)
    }

    static func cardFill(for scheme: ColorScheme) -> Color {
        scheme == .light ? Color.white.opacity(0.38) : Color.white.opacity(0.075)
    }

    static func controlFill(for scheme: ColorScheme) -> Color {
        scheme == .light ? Color.black.opacity(0.055) : Color.white.opacity(0.085)
    }

    /// Raised contrast is asked for by someone who cannot see a hairline at a
    /// tenth of an opacity, so the outlines that separate one card from the
    /// next are the ones that answer. A panel is rebuilt every time it opens,
    /// which is when a change to this setting shows.
    static func border(for scheme: ColorScheme) -> Color {
        let raised = NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast
        return scheme == .light
            ? Color.black.opacity(raised ? 0.24 : 0.09)
            : Color.white.opacity(raised ? 0.28 : 0.11)
    }

    /// A control that sits ON a glass surface rather than in it: the system's
    /// own round toggles read as physical because they are lighter than what
    /// is behind them and carry their own shadow.
    static func raisedFill(for scheme: ColorScheme) -> Color {
        scheme == .light ? Color.white.opacity(0.88) : Color.white.opacity(0.14)
    }

    static func raisedBorder(for scheme: ColorScheme) -> Color {
        let raised = NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast
        return scheme == .light
            ? Color.black.opacity(raised ? 0.22 : 0.07)
            : Color.white.opacity(raised ? 0.32 : 0.16)
    }

    static func raisedShadow(for scheme: ColorScheme) -> Color {
        scheme == .light ? Color.black.opacity(0.14) : Color.black.opacity(0.38)
    }

    /// The lit edge of a glass surface: bright where the light comes from,
    /// gone by the bottom. Without it a translucent panel reads as paper.
    static func rimHighlight(for scheme: ColorScheme) -> LinearGradient {
        LinearGradient(colors: [Color.white.opacity(scheme == .light ? 0.95 : 0.30),
                                Color.white.opacity(scheme == .light ? 0.12 : 0.04)],
                       startPoint: .top,
                       endPoint: .bottom)
    }
}

func sectionTitle(_ text: String) -> some View {
    Text(text.uppercased())
        .font(.system(size: 10, weight: .semibold))
        .kerning(0.5)
        .foregroundStyle(.secondary)
}

extension View {
    /// The rounded card background used by every panel section. A card
    /// holding a list of rows is not padded: each row brings its own insets,
    /// so hover highlights and separators can reach the card's edges.
    func panelCard(interactive: Bool = true, padded: Bool = true) -> some View {
        modifier(PanelCardModifier(interactive: interactive, padded: padded))
    }

    /// A restrained glass base for the menu panel: still translucent, but with a
    /// stable tint so text and controls do not depend too much on the wallpaper.
    /// It reaches the popover's arrow; see PanelGlassSurface.
    func panelGlassSurface() -> some View {
        background(PanelGlassSurface())
    }
}

private struct PanelCardModifier: ViewModifier {
    var interactive: Bool
    var padded = true
    @Environment(\.colorScheme) private var colorScheme
    @Environment(\.notchPresentation) private var notchPresentation

    func body(content: Content) -> some View {
        if notchPresentation {
            content.padding(padded ? 12 : 0).modifier(NotchControlSurface(cornerRadius: 18, interactive: interactive))
        } else {
        content
            .padding(padded ? 10 : 0)
            .background(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(PanelSurface.cardFill(for: colorScheme))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .strokeBorder(PanelSurface.border(for: colorScheme), lineWidth: 0.7)
            )
        }
    }
}

private struct PanelGlassSurface: View {
    @Environment(\.colorScheme) private var colorScheme
    @Environment(\.notchPresentation) private var notchPresentation
    @Environment(\.notchGlassSurface) private var notchGlassSurface
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @AppStorage(DefaultsKey.liquidGlassEnabled) private var liquidGlassEnabled = false

    var body: some View {
        // AppKit hands the hosted panel a safe area for the popover's border and
        // arrow, so a surface that stopped at the panel would leave the tip in the
        // plain system material. The panel content keeps that inset and never sits
        // under the arrow; only this background bleeds into it. The popover clips
        // it to its own balloon, so the surface is a plain rectangle: rounding would
        // expose the system material at the corners, while stroking would duplicate
        // the outline AppKit already draws. Where the popover still insets its
        // content, the panel is a card inside the balloon instead; see
        // PanelSurface.popoverHostsFullSizeContent.
        if notchPresentation {
            Rectangle().fill(notchGlassSurface ? Color.clear : .black)
        } else if PanelSurface.popoverHostsFullSizeContent {
            surface.ignoresSafeArea()
        } else {
            insetSurface
        }
    }

    @ViewBuilder
    private var surface: some View {
#if compiler(>=6.2)
        if #available(macOS 26.0, *), liquidGlassEnabled, !reduceTransparency {
            Rectangle()
                .fill(Color.clear)
                .glassEffect(.regular, in: Rectangle())
                .overlay(
                    Rectangle()
                        .fill(PanelSurface.baseFill(for: colorScheme).opacity(colorScheme == .light ? 0.35 : 0.45))
                )
        } else {
            standardSurface
        }
#else
        standardSurface
#endif
    }

    @ViewBuilder
    private var standardSurface: some View {
        Rectangle()
            .fill(.regularMaterial)
            .overlay(Rectangle().fill(PanelSurface.baseFill(for: colorScheme)))
    }

    /// The panel as a card inside a popover that insets its content (before
    /// macOS 26): the balloon's rounding never reaches it there, so it carries
    /// its own rounding and rim. Liquid Glass needs macOS 26, so this is always
    /// the standard material.
    private var insetSurface: some View {
        let shape = RoundedRectangle(cornerRadius: 18, style: .continuous)
        return shape
            .fill(.regularMaterial)
            .overlay(shape.fill(PanelSurface.baseFill(for: colorScheme)))
            .overlay(shape.strokeBorder(PanelSurface.border(for: colorScheme), lineWidth: 0.8))
    }
}

func appDelegate() -> AppDelegate? {
    NSApp.delegate as? AppDelegate
}

/// The official mark (Resources/Brand/logo.png, trimmed at build time),
/// tintable for light or dark surfaces.
///
/// `width` sizes a slot of `width × width / slotAspect`, the footprint the
/// earlier wide mark filled, so layouts built around it keep their spacing.
/// The helmet fits that slot by its height; pass `height` for a taller slot.
struct BrandMark: View {
    var width: CGFloat
    var height: CGFloat? = nil
    var tint: Color = .white

    static let slotAspect: CGFloat = 1.97

    private static let mark: NSImage? = {
        guard let url = Bundle.main.url(forResource: "BrandMark", withExtension: "png") else { return nil }
        return NSImage(contentsOf: url)
    }()

    var body: some View {
        if let mark = Self.mark {
            Image(nsImage: mark)
                .renderingMode(.template)
                .interpolation(.high)
                .antialiased(true)
                .resizable()
                .aspectRatio(contentMode: .fit)
                .foregroundStyle(tint)
                .frame(width: width, height: height ?? width / Self.slotAspect)
        } else {
            Image(systemName: "circle.fill")
                .resizable()
                .aspectRatio(contentMode: .fit)
                .foregroundStyle(tint)
                .frame(width: width * 0.5)
        }
    }
}

struct DiscordMark: View {
    var width: CGFloat

    private static let mark: NSImage? = {
        guard let url = Bundle.main.url(forResource: "discord-symbol",
                                        withExtension: "svg",
                                        subdirectory: "Images") else { return nil }
        return NSImage(contentsOf: url)
    }()

    var body: some View {
        if let mark = Self.mark {
            Image(nsImage: mark)
                .resizable()
                .aspectRatio(contentMode: .fit)
                .frame(width: width)
        }
    }
}

/// Squircle badge with the white helmet on black, as in the app icon — the
/// app's face in the About tab and onboarding.
struct BrandBadge: View {
    var size: CGFloat

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.26, style: .continuous)
                .fill(Color.black)
            RoundedRectangle(cornerRadius: size * 0.26, style: .continuous)
                .strokeBorder(Color.white.opacity(0.14), lineWidth: max(0.5, size / 128))
            // The icon's proportions: the helmet stands at ~70% of the badge.
            BrandMark(width: size * 0.7, height: size * 0.7)
        }
        .frame(width: size, height: size)
    }
}
