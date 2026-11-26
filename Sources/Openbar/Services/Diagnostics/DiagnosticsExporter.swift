// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import AppKit
import Foundation

/// Gathers what a diagnostics file holds and writes it as one zip (#230).
/// Everything is read on this Mac; nothing is sent anywhere.
enum DiagnosticsExporter {
    enum Failure: Error {
        case write(String)
    }

    private static let work = DispatchQueue(label: "com.openbar.diagnostics-export", qos: .userInitiated)

    /// Collects the chosen parts and writes the zip to `url`; calls back on
    /// the main thread.
    static func export(parts: Set<DiagnosticsPart>, to url: URL, completion: @escaping (Result<URL, Failure>) -> Void) {
        let snapshot = MainSnapshot.take()
        let home = FileManager.default.homeDirectoryForCurrentUser
        DiagnosticsLog.info("diagnostics", "export started: \(parts.map(\.rawValue).sorted().joined(separator: ","))")
        work.async {
            let input = collect(snapshot: snapshot, home: home, parts: parts)
            let now = Date()
            let files = DiagnosticsReport.files(parts: parts, input: input, platform: "macOS", now: now, home: home.path)
            let result: Result<URL, Failure>
            do {
                try write(files, to: url)
                result = .success(url)
                DiagnosticsLog.info("diagnostics", "export saved (\(files.count) files)")
            } catch let failure as Failure {
                result = .failure(failure)
            } catch {
                result = .failure(.write(error.localizedDescription))
            }
            DispatchQueue.main.async { completion(result) }
        }
    }

    // MARK: Writing

    /// Writes the files into a folder named `openbar-diagnostics` and zips
    /// that folder, so the zip opens to one folder on every system.
    static func write(_ files: [(path: String, data: Data)], to url: URL) throws {
        let manager = FileManager.default
        let scratch = manager.temporaryDirectory.appending(path: "openbar-diagnostics-\(UUID().uuidString)")
        let root = scratch.appending(path: DiagnosticsReport.folder)
        defer { try? manager.removeItem(at: scratch) }
        do {
            try manager.createDirectory(at: root, withIntermediateDirectories: true)
            for file in files {
                let target = root.appending(path: file.path)
                try manager.createDirectory(at: target.deletingLastPathComponent(), withIntermediateDirectories: true)
                try file.data.write(to: target, options: .atomic)
            }
        } catch {
            throw Failure.write(error.localizedDescription)
        }
        let partial = url.deletingLastPathComponent().appending(path: ".\(url.lastPathComponent).partial")
        try? manager.removeItem(at: partial)
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/ditto")
        process.arguments = ["-c", "-k", "--norsrc", "--noextattr", "--keepParent", root.path, partial.path]
        let errors = Pipe()
        process.standardError = errors
        process.standardOutput = FileHandle.nullDevice
        do { try process.run() } catch { throw Failure.write(error.localizedDescription) }
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            try? manager.removeItem(at: partial)
            let reason = String(decoding: errors.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
            throw Failure.write(reason.isEmpty ? "ditto \(process.terminationStatus)" : reason)
        }
        do {
            if manager.fileExists(atPath: url.path) { _ = try manager.replaceItemAt(url, withItemAt: partial) }
            else { try manager.moveItem(at: partial, to: url) }
        } catch {
            try? manager.removeItem(at: partial)
            throw Failure.write(error.localizedDescription)
        }
    }

    // MARK: Collecting

    /// What only the main thread may read.
    struct MainSnapshot {
        var displays: [DiagnosticsSystem.Display]
        var channel: String
        var language: String
        var accounts: [ClaudeAccount]
        var accountHooks: [String: ClaudeAccountHooks]
        var codexStatus: ClaudeSessionService.CodexStatus?
        var bridges: [ClaudeBridgeStatus]?
        var sessions: [ClaudeSession]
        var remoteStates: [String: RemoteHostState]

        static func take() -> MainSnapshot {
            let service = ClaudeSessionService.shared
            let screens = NSScreen.screens
            let displays = screens.enumerated().map { index, screen in
                let number = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber
                return DiagnosticsSystem.Display(
                    width: Int(screen.frame.width), height: Int(screen.frame.height),
                    scale: Double(screen.backingScaleFactor), main: index == 0,
                    builtIn: number.map { CGDisplayIsBuiltin(CGDirectDisplayID($0.uint32Value)) != 0 },
                    notch: screen.safeAreaInsets.top > 0)
            }
            return MainSnapshot(displays: displays, channel: FeedbackDiagnostics.current().updateChannel,
                                language: L10n.shared.language.rawValue, accounts: service.accounts,
                                accountHooks: service.accountHooks, codexStatus: service.codexStatus,
                                bridges: service.bridges, sessions: service.sessions,
                                remoteStates: service.remoteStates)
        }
    }

    static func collect(snapshot: MainSnapshot, home: URL, parts: Set<DiagnosticsPart>) -> DiagnosticsInput {
        let defaults = UserDefaults.standard
        let path = ClaudeCLI.shell(home: home).environment["PATH"]
        return DiagnosticsInput(
            system: system(snapshot),
            features: features(defaults),
            agents: agents(snapshot, home: home, path: path, defaults: defaults),
            events: parts.contains(.events) ? DiagnosticsJournal.shared.recent() : [],
            hosts: parts.contains(.hosts) ? hosts(snapshot, home: home, path: path, defaults: defaults)
                : DiagnosticsHosts(terminals: [], editors: [], inUse: [:], claudeAccounts: [], codexAccounts: [], remoteHosts: []),
            logs: parts.contains(.logs) ? logs() : [])
    }

    static func system(_ snapshot: MainSnapshot) -> DiagnosticsSystem {
        let version = ProcessInfo.processInfo.operatingSystemVersion
        var cpu = sysctl("machdep.cpu.brand_string") ?? "unknown"
        if let machine = sysctl("hw.machine") { cpu += " (\(machine))" }
        var translated: Int32 = 0
        var size = MemoryLayout<Int32>.size
        if sysctlbyname("sysctl.proc_translated", &translated, &size, nil, 0) == 0, translated == 1 { cpu += ", Rosetta" }
        return DiagnosticsSystem(
            app: .init(name: AppInfo.name, version: AppInfo.version,
                       build: AppInfo.build,
                       commit: AppInfo.buildCommit, channel: snapshot.channel, developerBuild: AppInfo.isDeveloperBuild),
            os: .init(name: "macOS", version: "\(version.majorVersion).\(version.minorVersion).\(version.patchVersion)",
                      build: sysctl("kern.osversion"), kernel: sysctl("kern.osrelease")),
            cpu: cpu, model: sysctl("hw.model"), language: snapshot.language, displays: snapshot.displays)
    }

    static func features(_ defaults: UserDefaults) -> [DiagnosticsFeature] {
        var list = AppFeature.allCases.map { feature in
            let available = feature.isAvailable(in: defaults)
            let engaged = feature.enabledKeys.isEmpty || feature.enabledKeys.contains { defaults.bool(forKey: $0) }
            return DiagnosticsFeature(id: feature.rawValue, group: "app", on: available && engaged)
        }
        let island = defaults.bool(forKey: DefaultsKey.notchEnabled)
        list.append(DiagnosticsFeature(id: "island", group: "island", on: island))
        let hidden = Set((defaults.string(forKey: DefaultsKey.notchHiddenModules) ?? "").split(separator: ",").map(String.init))
        let switched: [NotchModule: String] = [
            .timer: DefaultsKey.notchTimerEnabled, .camera: DefaultsKey.notchCameraEnabled,
            .calendar: DefaultsKey.notchCalendarEnabled, .notifications: DefaultsKey.notchNotificationsEnabled,
            .agents: DefaultsKey.notchAgentsEnabled, .watch: DefaultsKey.notchWatchEnabled,
            .sessions: DefaultsKey.notchSessionsEnabled,
        ]
        for module in NotchModule.allCases {
            let on = island && !hidden.contains(module.rawValue) && switched[module].map { defaults.bool(forKey: $0) } ?? true
            list.append(DiagnosticsFeature(id: module.rawValue, group: "island", on: on))
        }
        let sessions: [(String, String)] = [
            ("previews", DefaultsKey.notchSessionsContentPreview), ("liveActivity", DefaultsKey.notchSessionsLiveActivity),
            ("autoExpand", DefaultsKey.notchSessionsAutoExpand), ("buddies", DefaultsKey.notchSessionsBuddies),
            ("limits", DefaultsKey.notchSessionsLimits), ("awayCard", DefaultsKey.notchSessionsAwayCard),
            ("voiceReplies", DefaultsKey.notchSessionsVoiceReplies), ("waitingSound", DefaultsKey.notchSessionsWaitingSound),
            ("remoteHosts", DefaultsKey.notchSessionsRemoteHostsEnabled),
        ]
        list += sessions.map { DiagnosticsFeature(id: $0.0, group: "sessions", on: defaults.bool(forKey: $0.1)) }
        let shown = NotchClaudeSessionsSupport.shownAgents(in: defaults)
        list += ClaudeSessionProvider.agents.map {
            DiagnosticsFeature(id: "agent.\($0.rawValue)", group: "sessions", on: shown.contains($0))
        }
        return list
    }

    static func agents(_ snapshot: MainSnapshot, home: URL, path: String?, defaults: UserDefaults) -> [DiagnosticsAgent] {
        let socket = ClaudeSessionService.socketPath(home: home)
        let bundle = ClaudeSessionService.bundleID
        let records = HookRecords.all(in: defaults)
        let providers = ClaudeSessionProvider.agents
        var commands: [ClaudeSessionProvider: String] = [:]
        for provider in providers {
            switch provider {
            case .claude: commands[provider] = ClaudeCLI.locate(path: path, home: home)
            case .codex: commands[provider] = CodexCLI.locate(path: path, home: home)
            default: commands[provider] = AgentCLI.locate(provider, path: path, home: home)
            }
        }
        let versions = probeVersions(commands, environment: ClaudeCLI.shell(home: home).environment)

        return providers.map { provider in
            let version = versions[provider]
            switch provider {
            case .claude:
                let command = ClaudeHookInstaller.command(socket: socket, bundle: bundle)
                let accounts = snapshot.accounts.isEmpty ? ClaudeAccounts.all(home: home) : snapshot.accounts
                let hooks = accounts.map { account in
                    let base = ClaudeSessionService.hookState(account.settingsURL, command: command, bundle: bundle)
                    let current = HookRecords.fingerprint(of: try? ClaudeHookInstaller.contents(account.settingsURL),
                                                          marker: ClaudeHookInstaller.marker(bundle: bundle))
                    let state = HookRecords.derive(HookRecords.Base(base),
                                                   record: records[HookRecords.claudeID(account.settingsURL)], current: current)
                    return DiagnosticsAgent.Hooks(account: account.hidden ? "\(account.name) (hidden)" : account.name,
                                                  state: state.rawValue, configPath: account.settingsURL.path)
                }
                let installed = commands[.claude] != nil
                    || FileManager.default.fileExists(atPath: home.appending(path: ".claude").path)
                return DiagnosticsAgent(id: provider.rawValue, name: provider.displayName, installed: installed,
                                        version: version, hooks: hooks)
            case .codex:
                let status: String = switch snapshot.codexStatus {
                case .connected?: "connected"
                case .notRunning?: "notRunning"
                case .notFound?: "notFound"
                case nil: "unchecked"
                }
                let installed = commands[.codex] != nil
                    || CodexSessionClient.home().map { FileManager.default.fileExists(atPath: $0.path) } == true
                return DiagnosticsAgent(id: provider.rawValue, name: provider.displayName, installed: installed,
                                        version: version, status: status, hooks: [])
            default:
                let state = AgentHookInstaller.diagnosticsState(provider, home: home, socket: socket, bundle: bundle,
                                                                records: records)
                let file = AgentHookInstaller.hookFile(provider, home: home, bundle: bundle)
                return DiagnosticsAgent(id: provider.rawValue, name: provider.displayName,
                                        installed: AgentHookInstaller.isPresent(provider, home: home, path: path),
                                        version: version ?? appVersion(provider, home: home),
                                        hooks: [.init(state: state.rawValue, configPath: file?.url.path)])
            }
        }
    }

    /// Antigravity is an app first: its bundle says its version when no `agy` does.
    static func appVersion(_ provider: ClaudeSessionProvider, home: URL) -> String? {
        guard provider == .antigravity else { return nil }
        for folder in ["/Applications", home.appending(path: "Applications").path] {
            for app in ["Antigravity.app", "Antigravity IDE.app"] {
                if let version = Bundle(path: "\(folder)/\(app)")?.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String {
                    return version
                }
            }
        }
        return nil
    }

    /// Runs each found command with `--version`, all at once, giving each
    /// three seconds.
    static func probeVersions(_ commands: [ClaudeSessionProvider: String],
                              environment: [String: String]) -> [ClaudeSessionProvider: String] {
        let group = DispatchGroup()
        let lock = NSLock()
        var versions: [ClaudeSessionProvider: String] = [:]
        for (provider, command) in commands {
            group.enter()
            DispatchQueue.global(qos: .userInitiated).async {
                defer { group.leave() }
                guard let output = ClaudeCLI.run(command, ["--version"], environment: environment, timeout: 3),
                      let version = DiagnosticsReport.version(in: String(decoding: output.out, as: UTF8.self) + "\n" + output.error)
                else { return }
                lock.lock()
                versions[provider] = version
                lock.unlock()
            }
        }
        group.wait()
        return versions
    }

    static func hosts(_ snapshot: MainSnapshot, home: URL, path: String?, defaults: UserDefaults) -> DiagnosticsHosts {
        let workspace = NSWorkspace.shared
        let terminalApps = [ClaudeSessionHosts.terminal, ClaudeTerminalApps.iterm, ClaudeTerminalApps.ghostty,
                            ClaudeTerminalApps.wezterm, ClaudeTerminalApps.kitty, ClaudeTerminalApps.warp,
                            ClaudeTerminalApps.alacritty]
        var terminals = terminalApps.filter { workspace.urlForApplication(withBundleIdentifier: $0) != nil }
            .map(ClaudeTerminalApps.name(of:))
        let folders = (path ?? "").split(separator: ":").map(String.init) + ["/opt/homebrew/bin", "/usr/local/bin"]
        for tool in ["tmux", "zellij", "screen"]
        where folders.contains(where: { FileManager.default.isExecutableFile(atPath: "\($0)/\(tool)") }) {
            terminals.append(tool)
        }
        let editors = (snapshot.bridges ?? ClaudeBridgeInstaller.editors().map { ide in
            let installed = ClaudeBridgeInstaller.isInstalled(ide, home: home)
            return ClaudeBridgeStatus(ide: ide, installed: installed,
                                      outdated: installed && ClaudeBridgeInstaller.isOutdated(ide, home: home))
        }).map { DiagnosticsHosts.Editor(name: $0.ide.name, bridge: $0.outdated ? "outdated" : $0.installed ? "installed" : "missing") }
        var inUse: [String: Int] = [:]
        for session in snapshot.sessions {
            inUse[session.host.displayName ?? "unknown", default: 0] += 1
        }
        let codex = CodexAccountProfile.decode(defaults.string(forKey: DefaultsKey.notchAgentsCodexProfiles) ?? "").map(\.name)
        let remote = RemoteHostPrefs.hosts(in: defaults).map { host in
            DiagnosticsHosts.Remote(label: host.alias,
                                    state: snapshot.remoteStates[host.alias].map(ClaudeSessionService.remoteKind) ?? "off")
        }
        let accounts = snapshot.accounts.map { $0.hidden ? "\($0.name) (hidden)" : $0.name }
        return DiagnosticsHosts(terminals: terminals, editors: editors, inUse: inUse, claudeAccounts: accounts,
                                codexAccounts: codex, remoteHosts: remote)
    }

    static func logs() -> [(name: String, text: String)] {
        DiagnosticsLog.shared.recentFiles().compactMap { url in
            guard let data = try? Data(contentsOf: url) else { return nil }
            return (url.lastPathComponent, String(decoding: data, as: UTF8.self))
        }
    }

    private static func sysctl(_ name: String) -> String? {
        var size = 0
        guard sysctlbyname(name, nil, &size, nil, 0) == 0, size > 0 else { return nil }
        var buffer = [CChar](repeating: 0, count: size)
        guard sysctlbyname(name, &buffer, &size, nil, 0) == 0 else { return nil }
        let text = String(cString: buffer).trimmingCharacters(in: .whitespaces)
        return text.isEmpty ? nil : text
    }
}
