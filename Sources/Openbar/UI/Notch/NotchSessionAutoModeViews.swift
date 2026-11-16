// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import SwiftUI

/// The mark of a session that runs in its agent's no-ask mode (#234), beside
/// its name, so it is seen at a glance that the island does not hold it.
struct NotchSessionAutoBadge: View {
    let provider: ClaudeSessionProvider
    let text: NotchClaudeSessionStrings
    var size: CGFloat = 9.5

    var body: some View {
        Label(text.autoMark, systemImage: "forward.fill")
            .labelStyle(NotchClaudeSessionInlineLabel())
            .font(.system(size: size, weight: .semibold))
            .foregroundStyle(provider.tint)
            .lineLimit(1)
            .fixedSize()
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(provider.tint.opacity(0.16), in: Capsule())
            .help(text.autoMarkHelp(provider))
            .accessibilityLabel(text.autoMarkHelp(provider))
    }
}

/// What the island allowed for a session without asking (#234), newest
/// first, so the person can see afterwards what ran.
struct NotchSessionLetThroughList: View {
    let session: ClaudeSession
    let text: NotchClaudeSessionStrings
    static let shown = 5

    var body: some View {
        let entries = Array(session.letThrough.reversed())
        VStack(alignment: .leading, spacing: 4) {
            Text(text.letThroughTitle.uppercased())
                .font(.system(size: 9, weight: .semibold))
                .tracking(0.6)
                .foregroundStyle(.white.opacity(0.45))
            ForEach(entries.prefix(Self.shown)) { entry in
                HStack(spacing: 6) {
                    Image(systemName: "forward.fill")
                        .font(.system(size: 8, weight: .semibold))
                        .foregroundStyle(session.provider.tint.opacity(0.8))
                        .frame(width: 12)
                    Text(entry.tool)
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(.white.opacity(0.85))
                    if let summary = entry.summary {
                        Text(summary)
                            .font(.system(size: 10.5, design: .monospaced))
                            .foregroundStyle(.white.opacity(0.65))
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                    Spacer(minLength: 4)
                    if entry.reason == .alwaysProceed {
                        Text(text.letThroughAlways)
                            .font(.system(size: 9.5, weight: .medium))
                            .foregroundStyle(.secondary)
                    }
                    Text(entry.at, style: .time)
                        .font(.system(size: 10))
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
                .accessibilityElement(children: .combine)
            }
            if entries.count > Self.shown {
                Text(text.moreLetThrough(entries.count - Self.shown))
                    .font(.system(size: 10))
                    .foregroundStyle(.secondary)
            }
        }
    }
}
