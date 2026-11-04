// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import AppKit
import SwiftUI

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
