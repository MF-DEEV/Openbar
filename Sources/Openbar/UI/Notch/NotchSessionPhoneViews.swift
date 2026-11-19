// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import SwiftUI

/// What was answered from the phone for a session (#214), newest first, so
/// its page tells which answers came from Telegram.
struct NotchSessionPhoneAnswerList: View {
    let session: ClaudeSession
    var large = false
    static let shown = 5

    var body: some View {
        let entries = Array(session.phoneAnswers.reversed())
        VStack(alignment: .leading, spacing: large ? 6 : 4) {
            Text(PhoneApprovalStrings.answeredTitle.uppercased())
                .font(.system(size: large ? 10.5 : 9, weight: .semibold))
                .tracking(0.6)
                .foregroundStyle(.white.opacity(0.45))
            ForEach(entries.prefix(Self.shown)) { entry in
                HStack(spacing: 6) {
                    Image(systemName: "iphone")
                        .font(.system(size: large ? 10 : 8.5, weight: .semibold))
                        .foregroundStyle(session.provider.tint.opacity(0.8))
                        .frame(width: large ? 14 : 12)
                    Text(entry.tool)
                        .font(.system(size: large ? 13 : 11, weight: .medium))
                        .foregroundStyle(.white.opacity(0.85))
                    if let summary = entry.summary {
                        Text(summary)
                            .font(.system(size: large ? 12 : 10.5, design: .monospaced))
                            .foregroundStyle(.white.opacity(0.65))
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                    Spacer(minLength: 4)
                    Text(PhoneApprovalStrings.verdict(entry.verdict))
                        .font(.system(size: large ? 11 : 9.5, weight: .medium))
                        .foregroundStyle(entry.verdict.refused ? Color.orange.opacity(0.9) : Color.secondary)
                    Text(entry.at, style: .time)
                        .font(.system(size: large ? 12 : 10))
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
                .accessibilityElement(children: .combine)
            }
        }
    }
}
