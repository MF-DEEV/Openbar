// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// The short screen before a diagnostics file is saved (#230): what goes in,
/// in plain words, with a switch for each part that may stay out. The rows
/// come from `DiagnosticsPart`, the list the file itself is written from.
struct DiagnosticsExportView: View {
    /// Shown when the file is saved for a bug report, which opens next.
    let forBugReport: Bool
    let close: () -> Void
    let saved: (URL) -> Void

    @ObservedObject private var l10n = L10n.shared
    @State private var chosen: Set<DiagnosticsPart> = Set(DiagnosticsPart.allCases)
    @State private var saving = false
    @State private var failure: String?

    private var text: DiagnosticsStrings { FeatureStrings.diagnostics(l10n.language) }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: "doc.zipper")
                    .font(.system(size: 20, weight: .semibold))
                    .foregroundStyle(Color.brandAccent)
                    .frame(width: 40, height: 40)
                    .background(Color.brandAccent.opacity(0.12), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                VStack(alignment: .leading, spacing: 3) {
                    Text(text.windowTitle).font(.title3.bold())
                    Text(text.intro)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }

            SettingsCard(title: text.alwaysTitle) {
                ForEach(DiagnosticsPart.allCases.filter { !$0.isOptional }, id: \.self) { part in
                    SettingsRow(symbol: symbol(part), title: title(part), caption: caption(part)) {
                        Image(systemName: "checkmark")
                            .font(.system(size: 11, weight: .bold))
                            .foregroundStyle(.secondary)
                            .accessibilityHidden(true)
                    }
                }
            }

            SettingsCard(title: text.optionalTitle) {
                ForEach(DiagnosticsPart.allCases.filter(\.isOptional), id: \.self) { part in
                    SettingsRow(symbol: symbol(part), title: title(part), caption: caption(part)) {
                        Toggle(title(part), isOn: binding(part))
                            .labelsHidden()
                            .toggleStyle(.switch)
                            .controlSize(.small)
                            .disabled(saving)
                    }
                }
            }

            HStack(alignment: .top, spacing: 8) {
                Image(systemName: "lock.shield")
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 4) {
                    Text(text.privacyNote)
                    Text(text.nothingSent)
                    if forBugReport { Text(text.reportHint) }
                }
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            }

            if let failure {
                Text(failure)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true)
            }

            HStack(spacing: 10) {
                if saving {
                    ProgressView().controlSize(.small)
                    Text(text.saving).font(.callout).foregroundStyle(.secondary)
                }
                Spacer()
                Button(text.cancel, action: close)
                    .keyboardShortcut(.cancelAction)
                Button(forBugReport ? text.saveAndReport : text.save, action: save)
                    .keyboardShortcut(.defaultAction)
                    .disabled(saving)
            }
        }
        .padding(20)
        .frame(width: 500)
        .brandControlTint()
    }

    private func binding(_ part: DiagnosticsPart) -> Binding<Bool> {
        Binding {
            chosen.contains(part)
        } set: { on in
            if on { chosen.insert(part) } else { chosen.remove(part) }
        }
    }

    private func symbol(_ part: DiagnosticsPart) -> String {
        switch part {
        case .system: return "desktopcomputer"
        case .features: return "switch.2"
        case .agents: return "terminal"
        case .events: return "list.bullet.rectangle"
        case .hosts: return "network"
        case .logs: return "doc.text"
        }
    }

    private func title(_ part: DiagnosticsPart) -> String {
        switch part {
        case .system: return text.systemTitle
        case .features: return text.featuresTitle
        case .agents: return text.agentsTitle
        case .events: return text.eventsTitle
        case .hosts: return text.hostsTitle
        case .logs: return text.logsTitle
        }
    }

    private func caption(_ part: DiagnosticsPart) -> String {
        switch part {
        case .system: return text.systemCaption
        case .features: return text.featuresCaption
        case .agents: return text.agentsCaption
        case .events: return String(format: text.eventsCaptionFormat, DiagnosticsJournal.capacity)
        case .hosts: return text.hostsCaption
        case .logs: return String(format: text.logsCaptionFormat, DiagnosticsLog.exportDays)
        }
    }

    private func save() {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = DiagnosticsReport.fileName()
        panel.allowedContentTypes = [.zip]
        panel.canCreateDirectories = true
        panel.directoryURL = FileManager.default.urls(for: .desktopDirectory, in: .userDomainMask).first
        NSApp.activate(ignoringOtherApps: true)
        let finish: (NSApplication.ModalResponse) -> Void = { response in
            guard response == .OK, let url = panel.url else { return }
            saving = true
            failure = nil
            DiagnosticsExporter.export(parts: chosen, to: url) { result in
                saving = false
                switch result {
                case .success(let url): saved(url)
                case .failure(.write(let reason)): failure = String(format: text.failedFormat, reason)
                }
            }
        }
        if let window = DiagnosticsExportWindow.shared.current {
            panel.beginSheetModal(for: window, completionHandler: finish)
        } else {
            finish(panel.runModal())
        }
    }
}

/// The window the export screen opens in, from About, Settings › Sessions
/// or a bug report. Main thread.
final class DiagnosticsExportWindow: NSObject, NSWindowDelegate {
    static let shared = DiagnosticsExportWindow()
    private var window: NSWindow?
    /// The window while it is open, for the save sheet.
    var current: NSWindow? { window?.isVisible == true ? window : nil }

    /// Shows the screen; `saved` runs once the file is written, after it is
    /// shown in Finder.
    func show(forBugReport: Bool = false, saved: ((URL) -> Void)? = nil) {
        let view = DiagnosticsExportView(forBugReport: forBugReport) { [weak self] in
            self?.window?.close()
        } saved: { [weak self] url in
            self?.window?.close()
            NSWorkspace.shared.activateFileViewerSelecting([url])
            saved?(url)
        }
        let host = NSHostingController(rootView: view)
        if let window {
            window.contentViewController = host
        } else {
            let window = NSWindow(contentViewController: host)
            window.styleMask = [.titled, .closable, .fullSizeContentView]
            // The screen names itself in its header; the bar stays empty.
            window.titleVisibility = .hidden
            window.titlebarAppearsTransparent = true
            window.isReleasedWhenClosed = false
            window.isRestorable = false
            window.delegate = self
            self.window = window
        }
        window?.title = FeatureStrings.diagnostics(L10n.shared.language).windowTitle
        // Sized before it is centered: SwiftUI would grow it from its corner afterwards.
        host.view.layoutSubtreeIfNeeded()
        window?.setContentSize(host.view.fittingSize)
        window?.center()
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
    }

    func windowWillClose(_ notification: Notification) {
        // The view is rebuilt on the next show, with every part switched on again.
        window?.contentViewController = nil
    }
}
