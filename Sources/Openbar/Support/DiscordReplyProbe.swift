// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

#if OPENBAR_DEVELOPMENT
import AppKit
import SwiftUI

/// Development probes for replies to Discord notifications. Neither touches
/// the person's Discord: the preview draws the island alone, and sending goes
/// to the app `-OpenbarDiscordReplyBundleID` names, a stand-in page.
enum DiscordReplyProbe {
    /// `--discord-reply-preview [--sending|--failed]`: the held preview of a
    /// Discord message with its reply field open, on screen for a screenshot
    /// (the window number is printed) until the probe exits after 8 s.
    static func previewAndExit(arguments: [String]) -> Never {
        let app = NSApplication.shared
        app.setActivationPolicy(.prohibited)
        guard let screen = NSScreen.main else { print("DISCORD REPLY PREVIEW FAILED: no display"); exit(1) }
        let geometry = NotchGeometry(screen: screen.frame, safeAreaTop: screen.safeAreaInsets.top,
                                     cameraWidth: screen.safeAreaInsets.top > 0 ? 210 : 0)
        let item = NotchSystemNotification(
            id: UUID(),
            content: NotchNotificationContent(app: "Discord", title: "Alicja (#general, Friends)", subtitle: "",
                                              body: "Hej, wpadasz dzi\u{015B} na stream?"),
            received: Date(), canOpen: true)
        let notifications = NotchNotificationService.shared
        notifications.receiveForProbe(item, bundleIdentifier: DiscordReplyService.bundleIdentifier)
        notifications.setReplying(item.id)
        notifications.setReplyDraft(arguments.contains("--draft") ? "Jasne, b\u{0119}d\u{0119} o 20" : "", for: item.id)
        let notice = NotchNotice(event: .systemNotification, title: item.content.title, detail: item.content.body,
                                 symbol: "bell.fill", notification: item.content, notificationID: item.id)
        let size = geometry.notificationPreviewSize(
            contentHeight: notice.previewContentHeight(width: geometry.notificationPreviewContentWidth))
        let content = NotchNotificationPreviewView(notice: notice, content: item.content, service: NotchService.shared)
            .padding(.top, geometry.safeContentTop)
            .padding(.horizontal, (size.width - geometry.notificationPreviewContentWidth) / 2)
            .frame(width: size.width, height: size.height, alignment: .top)
            .background(Color.black)
            .environment(\.colorScheme, .dark)
        let window = NSWindow(contentRect: CGRect(x: screen.frame.midX - size.width / 2, y: screen.frame.midY,
                                                  width: size.width, height: size.height),
                              styleMask: [.borderless], backing: .buffered, defer: false)
        window.contentView = NSHostingView(rootView: content)
        window.level = .floating
        window.orderFrontRegardless()
        print("DISCORD REPLY PREVIEW WINDOW \(window.windowNumber) \(Int(size.width))x\(Int(size.height))")
        fflush(stdout)
        DispatchQueue.main.asyncAfter(deadline: .now() + 8) { exit(0) }
        app.run()
        exit(1)
    }

    /// `--discord-reply-send <title> <text>`: the whole send path, against
    /// the app `-OpenbarDiscordReplyBundleID` names.
    static func sendAndExit(title: String, text: String) -> Never {
        let app = NSApplication.shared
        app.setActivationPolicy(.prohibited)
        guard DiscordReplyService.bundleIdentifier != DiscordReplySupport.stableBundleIdentifier else {
            print("DISCORD REPLY FAILED: name a stand-in with -OpenbarDiscordReplyBundleID; the probe never drives Discord")
            exit(1)
        }
        DiscordReplyService.send(title: title, text: text) { failure in
            print(failure.map { "DISCORD REPLY FAILED: \($0.rawValue)" } ?? "DISCORD REPLY OK")
            exit(failure == nil ? 0 : 2)
        }
        app.run()
        exit(1)
    }
}
#endif
