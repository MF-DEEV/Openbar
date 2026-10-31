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

    private enum Action { case stop, remove }

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
            Text(action == .stop ? text.stopConfirm : text.removeConfirm)
                .font(.system(size: 11))
                .foregroundStyle(.white.opacity(0.85))
                .lineLimit(2)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 4)
            button(text.keepSession, symbol: nil) { asking = nil }
            button(action == .stop ? text.stopSession : text.removeSession, symbol: action == .stop ? "stop.fill" : "trash",
                   destructive: true) { run(action) }
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
        switch action {
        case .stop: service.stopBackground(session, completion: finish)
        case .remove: service.removeBackground(session, completion: finish)
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

/// Starts a session: which folder, optionally what to say first, and whether
/// it runs in the background or in a terminal.
struct NotchClaudeNewSessionView: View {
    let text: NotchClaudeSessionStrings
    @ObservedObject private var service = ClaudeSessionService.shared
    @State private var folders: [ClaudeRecentFolder] = []
    @State private var chosen: ClaudeRecentFolder?
    @State private var prompt = ""
    @State private var inBackground = true
    @State private var starting = false
    @State private var problem: String?
    @FocusState private var promptFocused: Bool

    private var background: Bool { inBackground && service.backgroundAvailable }

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            folderList
            TextField(text.firstPrompt, text: $prompt)
                .textFieldStyle(.plain)
                .font(.system(size: 12))
                .focused($promptFocused)
                .padding(.horizontal, 10)
                .frame(height: 30)
                .background(.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
                .accessibilityLabel(text.firstPrompt)
            if let problem {
                Text(problem)
                    .font(.system(size: 10.5))
                    .foregroundStyle(Color(red: 1.0, green: 0.55, blue: 0.5))
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
            }
            HStack(spacing: 6) {
                if service.backgroundAvailable {
                    mode(text.runInBackground, selected: inBackground) { inBackground = true }
                    mode(text.runInTerminal, selected: !inBackground) { inBackground = false }
                } else {
                    Text(text.backgroundOffHelp)
                        .font(.system(size: 10.5))
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
                Spacer(minLength: 4)
                startButton
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .notchBand(leading: {
            NotchBandBackButton(title: text.newSession) { service.composingSession = false }
        })
        .onAppear {
            service.recentFolders { found in
                folders = found
                if chosen == nil { chosen = found.first }
            }
        }
    }

    private var folderList: some View {
        ScrollView {
            LazyVStack(spacing: 3) {
                if folders.isEmpty {
                    Text(text.noRecentFolders)
                        .font(.system(size: 11.5))
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, minHeight: 28, alignment: .leading)
                        .padding(.horizontal, 9)
                }
                ForEach(folders) { folder in
                    Button { chosen = folder; problem = nil } label: {
                        HStack(spacing: 7) {
                            Image(systemName: chosen == folder ? "checkmark.circle.fill" : "folder")
                                .font(.system(size: 11, weight: .semibold))
                                .foregroundStyle(chosen == folder ? ClaudeSessionProvider.claude.tint : .white.opacity(0.5))
                                .frame(width: 14)
                            Text(folder.name)
                                .font(.system(size: 12, weight: .medium))
                                .foregroundStyle(.white.opacity(0.92))
                                .lineLimit(1)
                            Text(folder.path)
                                .font(.system(size: 10))
                                .foregroundStyle(.tertiary)
                                .lineLimit(1)
                                .truncationMode(.head)
                            Spacer(minLength: 0)
                        }
                        .padding(.horizontal, 9)
                        .frame(height: 28)
                        .background(.white.opacity(chosen == folder ? 0.1 : 0), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                        .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                    }
                    .buttonStyle(NotchButtonStyle(cornerRadius: 8, lifts: false))
                    .accessibilityLabel(folder.path)
                    .accessibilityAddTraits(chosen == folder ? .isSelected : [])
                }
                Button(action: browse) {
                    HStack(spacing: 7) {
                        Image(systemName: "folder.badge.plus").font(.system(size: 11, weight: .semibold)).frame(width: 14)
                        Text(text.browseFolder).font(.system(size: 12, weight: .medium))
                        Spacer(minLength: 0)
                    }
                    .foregroundStyle(.white.opacity(0.75))
                    .padding(.horizontal, 9)
                    .frame(height: 28)
                    .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                }
                .buttonStyle(NotchButtonStyle(cornerRadius: 8, lifts: false))
            }
        }
        .scrollIndicators(.automatic)
    }

    private func mode(_ title: String, selected: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 11.5, weight: .medium))
                .foregroundStyle(.white.opacity(selected ? 1 : 0.6))
                .padding(.horizontal, 10)
                .frame(height: 28)
                .background(.white.opacity(selected ? 0.16 : 0.05), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 8))
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    private var startButton: some View {
        Button(action: start) {
            Text(text.startSession)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(.white)
                .padding(.horizontal, 14)
                .frame(height: 28)
                .background(ClaudeSessionProvider.claude.tint.opacity(chosen == nil ? 0.3 : 0.85),
                            in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        }
        .buttonStyle(NotchButtonStyle(cornerRadius: 8))
        .disabled(chosen == nil || starting)
    }

    private func start() {
        guard let chosen else { return }
        starting = true
        problem = nil
        service.startSession(in: chosen.path, configHome: chosen.configHome, prompt: prompt, background: background) { failure in
            starting = false
            problem = failure
            if failure == nil { service.composingSession = false }
        }
    }

    /// A folder from anywhere, kept in the list for this visit.
    private func browse() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.level = .modalPanel
        NSApp.activate(ignoringOtherApps: true)
        guard panel.runModal() == .OK, let url = panel.url else { return }
        let path = url.standardizedFileURL.path
        let existing = folders.first { $0.path == path }
        let folder = existing ?? ClaudeRecentFolder(path: path, configHome: folders.first?.configHome, used: nil)
        if existing == nil { folders.insert(folder, at: 0) }
        chosen = folder
        problem = nil
    }
}
