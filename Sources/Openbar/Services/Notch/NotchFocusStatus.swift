// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV and the Openbar contributors

import AppKit
import ApplicationServices

/// Whether Focus or Do Not Disturb is on (#231). macOS gives no public
/// reading of it to an app like this: INFocusStatusCenter needs the
/// communication-notifications entitlement, and the Focus database sits
/// behind privacy protection. While a Focus is on, Control Center shows its
/// item in the menu bar, so that item tells, read through Accessibility.
enum NotchFocusStatus {
    static let menuExtraIdentifier = "com.apple.menuextra.focusmode"

    /// True while a Focus is on, false while none is; nil when it can't be
    /// read (no Accessibility, no Control Center), which holds nothing back.
    static func isOn() -> Bool? {
        guard AXIsProcessTrusted(),
              let center = NSRunningApplication.runningApplications(withBundleIdentifier: "com.apple.controlcenter").first
        else { return nil }
        let app = AXUIElementCreateApplication(center.processIdentifier)
        AXUIElementSetMessagingTimeout(app, 0.5)
        guard let bar = value(app, "AXExtrasMenuBar") else { return nil }
        // swiftlint:disable:next force_cast
        let items = value(bar as! AXUIElement, kAXChildrenAttribute) as? [AXUIElement] ?? []
        return items.contains { value($0, "AXIdentifier") as? String == menuExtraIdentifier }
    }

    private static func value(_ element: AXUIElement, _ attribute: String) -> AnyObject? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, attribute as CFString, &value) == .success else { return nil }
        return value
    }
}
