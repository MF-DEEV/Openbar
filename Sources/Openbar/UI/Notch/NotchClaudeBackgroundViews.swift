// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import SwiftUI

/// Says a session runs in the background, in words, on its row.
struct NotchClaudeBackgroundMark: View {
    let text: NotchClaudeSessionStrings

    var body: some View {
        Text(text.backgroundMark)
            .font(.system(size: 9.5, weight: .semibold))
            .foregroundStyle(.white.opacity(0.72))
            .lineLimit(1)
            .fixedSize()
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(.white.opacity(0.1), in: Capsule())
            .help(text.backgroundHelp)
    }
}

/// What can be done with a background session: read its output, stop it,
/// remove it. Stopping and removing ask first, in place.
struct NotchClaudeBackgroundBar: View {
    let session: ClaudeSession
    let text: NotchClaudeSessionStrings
    @Binding var showingLogs: Bool
    @ObservedObject private var service = ClaudeSessionService.shared
    @ObservedObject private var notch = NotchService.shared
    @State private var asking: Action?
    @State private var working = false
    @State private var problem: String?

    private enum Action { case stop, remove, removeAnyway }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            if let asking { confirmation(asking) } else { buttons }
            if let problem {
                Text(problem)
                    .font(.system(size: 10.5))
                    .foregroundStyle(Color(red: 1.0, green: 0.55, blue: 0.5))
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var buttons: some View {
        HStack(spacing: 5) {
            NotchClaudeBackgroundMark(text: text)
            Spacer(minLength: 4)
            button(text.logs, symbol: "text.alignleft", active: showingLogs) {
                showingLogs.toggle()
                // Logs read in the enlarged page.
                if showingLogs { notch.setSessionsEnlarged(true) }
            }
            if NotchClaudeSessionsSupport.canStop(session) {
                button(text.stopSession, symbol: "stop.fill") { problem = nil; asking = .stop }
            }
            if NotchClaudeSessionsSupport.canRemove(session) {
                button(text.removeSession, symbol: "trash") { problem = nil; asking = .remove }
            }
        }
    }

    private func confirmation(_ action: Action) -> some View {
        HStack(spacing: 6) {
            Text(action == .stop ? text.stopConfirm : action == .remove ? text.removeConfirm : text.worktreeUnsaved)
                .font(.system(size: 11))
                .foregroundStyle(.white.opacity(0.85))
                .lineLimit(2)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 4)
            button(text.keepSession, symbol: nil) { asking = nil }
            button(action == .stop ? text.stopSession : action == .remove ? text.removeSession : text.removeAnyway,
                   symbol: action == .stop ? "stop.fill" : "trash", destructive: true) { run(action) }
                .disabled(working)
        }
    }

    private func run(_ action: Action) {
        working = true
        let finish: (String?) -> Void = { failure in
            working = false
            asking = nil
            problem = failure
        }
        let removed: (WorktreeRemoval) -> Void = { outcome in
            switch outcome {
            case .done: finish(nil)
            case .failed(let said): finish(said)
            // Its worktree holds work found nowhere else: ask again, in words that say so.
            case .unsaved:
                working = false
                asking = .removeAnyway
            }
        }
        switch action {
        case .stop: service.stopBackground(session, completion: finish)
        case .remove: service.removeBackground(session, completion: removed)
        case .removeAnyway: service.removeBackground(session, anyway: true, completion: removed)
        }
    }

    private func button(_ title: String, symbol: String?, active: Bool = false, destructive: Bool = false,
                        action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 4) {
                if let symbol { Image(systemName: symbol).font(.system(size: 10, weight: .semibold)) }
                Text(title).font(.system(size: 11.5, weight: .medium)).lineLimit(1)
            }
            .foregroundStyle(destructive ? Color(red: 1.0, green: 0.55, blue: 0.5) : .white.opacity(active ? 1 : 0.85))
            .padding(.horizontal, 8)
            .frame(height: 26)
            .background(.white.opacity(active ? 0.16 : 0.07), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 8))
    }
}

/// What can be done with the worktree a session works in: open its folder,
/// copy its branch, and remove one the island made once the session is done
/// with it. Claude Code's own go with `claude rm`.
struct NotchClaudeWorktreeBar: View {
    let session: ClaudeSession
    let text: NotchClaudeSessionStrings
    @ObservedObject private var service = ClaudeSessionService.shared
    @State private var branch: String?
    @State private var copied = false
    @State private var asking: Ask?
    @State private var working = false
    @State private var problem: String?

    private enum Ask { case remove, unsaved }

    private var canRemove: Bool {
        SessionWorktrees.isOwn(session.cwd) && !session.phase.isActive && !session.phase.isWaiting
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            if let asking { confirmation(asking) } else { buttons }
            if let problem {
                Text(problem)
                    .font(.system(size: 10.5))
                    .foregroundStyle(Color(red: 1.0, green: 0.55, blue: 0.5))
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .onAppear { service.worktreeBranch(session) { branch = $0 } }
    }

    private var buttons: some View {
        HStack(spacing: 5) {
            Label(session.worktree ?? "", systemImage: "arrow.triangle.branch")
                .labelStyle(NotchClaudeSessionInlineLabel())
                .font(.system(size: 10.5, weight: .medium))
                .foregroundStyle(.white.opacity(0.6))
                .lineLimit(1)
                .truncationMode(.middle)
                .help(text.worktreeMark)
            Spacer(minLength: 4)
            chip(text.openWorktree, symbol: "folder") {
                guard let folder = SessionWorktrees.worktreeFolder(session.cwd) else { return }
                NSWorkspace.shared.open(URL(fileURLWithPath: folder, isDirectory: true))
            }
            if let branch {
                chip(copied ? text.branchCopied : text.copyBranch, symbol: copied ? "checkmark" : "doc.on.doc") {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(branch, forType: .string)
                    copied = true
                    DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { copied = false }
                }
                .help(branch)
            }
            if canRemove {
                chip(text.removeWorktree, symbol: "trash") { problem = nil; asking = .remove }
            }
        }
    }

    private func confirmation(_ ask: Ask) -> some View {
        HStack(spacing: 6) {
            Text(ask == .remove ? text.removeWorktreeConfirm : text.worktreeUnsaved)
                .font(.system(size: 11))
                .foregroundStyle(.white.opacity(0.85))
                .lineLimit(2)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 4)
            chip(text.keepSession, symbol: nil) { asking = nil }
            chip(ask == .remove ? text.removeWorktree : text.removeAnyway, symbol: "trash", destructive: true) {
                remove(anyway: ask == .unsaved)
            }
            .disabled(working)
        }
    }

    private func remove(anyway: Bool) {
        working = true
        service.removeWorktree(session, anyway: anyway) { outcome in
            working = false
            switch outcome {
            case .done: asking = nil
            case .unsaved: asking = .unsaved
            case .failed(let said):
                asking = nil
                problem = said
            }
        }
    }

    private func chip(_ title: String, symbol: String?, destructive: Bool = false,
                      action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 4) {
                if let symbol {
                    Image(systemName: symbol)
                        .font(.system(size: 10, weight: .semibold))
                        .contentTransition(.symbolEffect(.replace))
                }
                Text(title).font(.system(size: 11.5, weight: .medium)).lineLimit(1)
            }
            .foregroundStyle(destructive ? Color(red: 1.0, green: 0.55, blue: 0.5) : .white.opacity(0.85))
            .padding(.horizontal, 8)
            .frame(height: 26)
            .background(.white.opacity(0.07), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 8))
    }
}

/// A background session's recent output, read again while it shows.
struct NotchClaudeBackgroundLogsView: View {
    let session: ClaudeSession
    let text: NotchClaudeSessionStrings
    @ObservedObject private var service = ClaudeSessionService.shared
    @State private var logs: String?
    @State private var loaded = false

    private let timer = Timer.publish(every: 5, on: .main, in: .common).autoconnect()

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(text.logsTitle)
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.secondary)
            ScrollViewReader { reader in
                ScrollView {
                    Group {
                        if let logs, !logs.isEmpty {
                            Text(logs)
                                .font(.system(size: 11, design: .monospaced))
                                .foregroundStyle(.white.opacity(0.88))
                                .textSelection(.enabled)
                        } else if loaded {
                            Text(logs == nil ? text.logsGone : text.logsEmpty)
                                .font(.system(size: 12))
                                .foregroundStyle(.secondary)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(10)
                    Color.clear.frame(height: 1).id("end")
                }
                .background(.white.opacity(0.05), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                .onChange(of: logs) { _, _ in reader.scrollTo("end", anchor: .bottom) }
            }
        }
        .onAppear(perform: load)
        .onReceive(timer) { _ in if session.background?.ended != true { load() } }
    }

    private func load() {
        service.backgroundLogs(session) { screen in
            logs = screen
            loaded = true
        }
    }
}

/// The Claude account a session runs under, while more than one is in use (#216).
struct NotchClaudeAccountBadge: View {
    let name: String
    var size: CGFloat = 9.5

    var body: some View {
        Label(name.count > 16 ? name.prefix(15) + "…" : name, systemImage: "person.crop.circle")
            .labelStyle(NotchClaudeSessionInlineLabel())
            .font(.system(size: size, weight: .semibold))
            .foregroundStyle(.white.opacity(0.78))
            .lineLimit(1)
            .fixedSize()
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(.white.opacity(0.1), in: Capsule())
            .help(ClaudeAccountStrings.sessionAccount(name))
            .accessibilityLabel(ClaudeAccountStrings.sessionAccount(name))
    }
}

/// The Claude app's Code tab or Cowork, beside the name of a session it
/// runs, so it never passes for one in a terminal (#246).
struct NotchClaudeOriginBadge: View {
    let origin: ClaudeSessionOrigin
    let text: NotchClaudeSessionStrings
    var size: CGFloat = 9.5

    var body: some View {
        Label(text.originBadge(origin), systemImage: origin == .cowork ? "folder" : "macwindow")
            .labelStyle(NotchClaudeSessionInlineLabel())
            .font(.system(size: size, weight: .semibold))
            .foregroundStyle(.white.opacity(0.78))
            .lineLimit(1)
            .fixedSize()
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(.white.opacity(0.1), in: Capsule())
            .help(text.originHelp(origin))
            .accessibilityLabel(text.originHelp(origin))
    }
}

/// The tmux or zellij a session's pane belongs to, beside its name (#235).
struct NotchMultiplexerBadge: View {
    let name: String
    let text: NotchClaudeSessionStrings
    var size: CGFloat = 9.5
    /// Open found no terminal attached to it.
    var detached = false

    var body: some View {
        Label(detached ? name + " · " + text.detachedMark : name, systemImage: "rectangle.split.2x1")
            .labelStyle(NotchClaudeSessionInlineLabel())
            .font(.system(size: size, weight: .semibold))
            .foregroundStyle(.white.opacity(0.78))
            .lineLimit(1)
            .fixedSize()
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(.white.opacity(0.1), in: Capsule())
            .help(detached ? text.detachedHelp(name) : text.inMultiplexer(name))
            .accessibilityLabel(detached ? text.detachedHelp(name) : text.inMultiplexer(name))
            .animation(.easeOut(duration: 0.15), value: detached)
    }
}

/// The editor whose panel runs an agent's session (Cline in VS Code, #241).
struct NotchEditorBadge: View {
    let agent: String
    let editor: String
    let text: NotchClaudeSessionStrings
    var size: CGFloat = 9.5

    var body: some View {
        Label(editor, systemImage: "chevron.left.forwardslash.chevron.right")
            .labelStyle(NotchClaudeSessionInlineLabel())
            .font(.system(size: size, weight: .semibold))
            .foregroundStyle(.white.opacity(0.78))
            .lineLimit(1)
            .fixedSize()
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(.white.opacity(0.1), in: Capsule())
            .help(text.agentInEditor(agent, editor))
            .accessibilityLabel(text.agentInEditor(agent, editor))
    }
}

/// The SSH host a session runs on, beside its name, so a remote session
/// never passes for one on this Mac (issue #189).
struct NotchRemoteHostBadge: View {
    let alias: String
    var size: CGFloat = 9.5

    var body: some View {
        // A long alias is cut, so the session's own name keeps its room.
        Label(alias.count > 22 ? alias.prefix(21) + "…" : alias, systemImage: "network")
            .labelStyle(NotchClaudeSessionInlineLabel())
            .font(.system(size: size, weight: .semibold))
            .foregroundStyle(.white.opacity(0.78))
            .lineLimit(1)
            .fixedSize()
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(.white.opacity(0.1), in: Capsule())
            .help("On " + alias + ", over SSH")
            .accessibilityLabel("On remote host " + alias)
    }
}
