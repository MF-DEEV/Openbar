// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV and the Openbar contributors

import SwiftUI

/// What the queue's Allow all let through (#281): every ask it answered,
/// with its session, agent, tool and whole command, newest first and grouped
/// by click. Asks allowed one by one aren't listed.
struct SessionBatchApprovalsSettings: View {
    @ObservedObject private var l10n = L10n.shared
    @ObservedObject private var log = BatchApprovalLog.shared

    private var text: NotchClaudeSessionStrings { FeatureStrings.notchClaudeSessions(l10n.language) }

    /// The asks of one click; the list is newest first already.
    struct Batch: Identifiable {
        let id: String
        let at: Date
        var entries: [BatchApproval]
    }

    static func batches(_ entries: [BatchApproval]) -> [Batch] {
        var out: [Batch] = []
        for entry in entries {
            if let last = out.indices.last, out[last].id == entry.batch {
                out[last].entries.append(entry)
            } else {
                out.append(Batch(id: entry.batch, at: entry.date, entries: [entry]))
            }
        }
        return out
    }

    var body: some View {
        let batches = Self.batches(log.entries)
        VStack(alignment: .leading, spacing: 8) {
            SettingsRow(symbol: "checklist.checked", title: text.batchApprovals, caption: text.batchApprovalsHint) {
                if !batches.isEmpty {
                    Button(text.batchApprovalsClear) { log.clear() }
                }
            }
            if batches.isEmpty {
                Text(text.batchApprovalsEmpty)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.leading, settingsRowTextInset)
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0, pinnedViews: [.sectionHeaders]) {
                        ForEach(Array(batches.enumerated()), id: \.offset) { index, batch in
                            Section {
                                ForEach(Array(batch.entries.enumerated()), id: \.offset) { _, entry in
                                    row(entry)
                                }
                            } header: {
                                header(batch, first: index == 0)
                            }
                        }
                    }
                }
                .frame(maxHeight: 320)
                .background(RoundedRectangle(cornerRadius: 8, style: .continuous).fill(Color.primary.opacity(0.04)))
                .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).strokeBorder(Color.primary.opacity(0.08)))
                .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                .padding(.leading, settingsRowTextInset)
                .accessibilityElement(children: .contain)
                .accessibilityLabel(text.batchApprovals)
            }
        }
    }

    private func header(_ batch: Batch, first: Bool) -> some View {
        VStack(spacing: 0) {
            if !first { Divider() }
            HStack {
                Text(batch.at.formatted(date: .abbreviated, time: .shortened))
                    .font(.caption.weight(.semibold))
                Spacer(minLength: 8)
                Text(text.batchApprovalsCount(batch.entries.count))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, 12)
            .padding(.top, 8)
            .padding(.bottom, 4)
        }
        .background(.bar)
    }

    private func row(_ entry: BatchApproval) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Group {
                if let provider = entry.provider {
                    NotchSessionAgentGlyph(provider: provider, size: 11, working: false)
                } else {
                    Image(systemName: "terminal").font(.system(size: 11))
                }
            }
            .frame(width: 18, height: 18)
            .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(who(entry))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
                Text(entry.tool)
                    .font(.caption.weight(.semibold))
                if let detail = entry.shownDetail { code(detail) }
                if let paths = entry.paths, !paths.isEmpty { code(paths.joined(separator: "\n")) }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .accessibilityElement(children: .combine)
    }

    private func code(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 11, design: .monospaced))
            .textSelection(.enabled)
            .fixedSize(horizontal: false, vertical: true)
            .lineLimit(12)
            .padding(.horizontal, 6)
            .padding(.vertical, 4)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: 4, style: .continuous).fill(Color.primary.opacity(0.06)))
    }

    private func who(_ entry: BatchApproval) -> String {
        let agent = entry.provider?.displayName ?? entry.agent
        var parts = [agent, entry.session]
        if entry.project != entry.session { parts.append(entry.project) }
        return parts.filter { !$0.isEmpty }.joined(separator: " · ")
    }
}
