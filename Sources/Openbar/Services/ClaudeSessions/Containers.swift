// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// A container the person picked in Settings (issue #330, docs/containers.md).
/// It is named so that a rebuilt one is still the same: a devcontainer by its
/// project folder on this Mac, any other container by its name.
struct ContainerTarget: Codable, Equatable, Hashable, Identifiable {
    /// A devcontainer's folder on this Mac (its `devcontainer.local_folder` label).
    var folder: String?
    /// Any other container's name.
    var name: String?
    /// The container Set up last wrote its hooks into, to tell a rebuilt one.
    var setUp: String?

    var id: String { folder.map { "folder:" + $0 } ?? "name:" + (name ?? "") }

    /// The key its sessions and connection go by, next to SSH aliases.
    var alias: String { Self.prefix + id }

    static let prefix = "container:"

    var title: String { Self.title(id: id) }

    static func title(id: String) -> String {
        if id.hasPrefix("folder:") {
            let folder = String(id.dropFirst("folder:".count))
            let last = URL(fileURLWithPath: folder).lastPathComponent
            return last.isEmpty ? folder : last
        }
        return id.hasPrefix("name:") ? String(id.dropFirst("name:".count)) : id
    }

    /// Whether this is the container `info` describes.
    func matches(_ info: ContainerInfo) -> Bool {
        if let folder { return info.folder == folder }
        return name != nil && info.name == name
    }

    static func == (lhs: ContainerTarget, rhs: ContainerTarget) -> Bool { lhs.id == rhs.id }
    func hash(into hasher: inout Hasher) { hasher.combine(id) }

    /// A name or folder `docker` and the agent script can take: no control
    /// characters, and a name that never reads as an option.
    var isValid: Bool {
        if let folder { return folder.hasPrefix("/") && !folder.contains(where: \.isNewline) && folder.utf8.count < 4096 }
        guard let name, !name.isEmpty, name.utf8.count <= 255, name.first != "-" else { return false }
        return name.unicodeScalars.allSatisfy { $0.isASCII && (CharacterSet.alphanumerics.contains($0) || "._-".unicodeScalars.contains($0)) }
    }
}

/// What a sessions row or a host badge shows for a key that may be an SSH
/// alias or a container's.
enum RemoteHostName {
    static func isContainer(_ alias: String) -> Bool { alias.hasPrefix(ContainerTarget.prefix) }

    static func title(_ alias: String) -> String {
        isContainer(alias) ? ContainerTarget.title(id: String(alias.dropFirst(ContainerTarget.prefix.count))) : alias
    }

    static func symbol(_ alias: String) -> String { isContainer(alias) ? "shippingbox" : "network" }

    static func help(_ alias: String) -> String {
        isContainer(alias) ? "In the container " + title(alias) : "On " + alias + ", over SSH"
    }
}

/// Where the containers and the switch for them are kept.
enum ContainerPrefs {
    /// Off by default: no container is looked at until the person turns it on.
    static func isEnabled(in defaults: UserDefaults = .standard) -> Bool {
        defaults.bool(forKey: DefaultsKey.notchSessionsContainersEnabled)
    }

    static func targets(in defaults: UserDefaults = .standard) -> [ContainerTarget] {
        guard let text = defaults.string(forKey: DefaultsKey.notchSessionsContainers),
              let targets = try? JSONDecoder().decode([ContainerTarget].self, from: Data(text.utf8)) else { return [] }
        var seen: Set<String> = []
        return targets.filter { $0.isValid && seen.insert($0.id).inserted }
    }

    static func save(_ targets: [ContainerTarget], in defaults: UserDefaults = .standard) {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let data = (try? encoder.encode(targets)) ?? Data("[]".utf8)
        defaults.set(String(decoding: data, as: UTF8.self), forKey: DefaultsKey.notchSessionsContainers)
    }

    /// The containers to connect to: those picked, while the switch is on.
    static func active(in defaults: UserDefaults = .standard) -> [ContainerTarget] {
        isEnabled(in: defaults) ? targets(in: defaults) : []
    }

    /// Remembers which container was set up, or that none is any more.
    static func markSetUp(_ target: ContainerTarget, container: String?, in defaults: UserDefaults = .standard) {
        var targets = targets(in: defaults)
        guard let index = targets.firstIndex(of: target) else { return }
        targets[index].setUp = container
        save(targets, in: defaults)
    }
}

/// A running container, as `docker inspect` describes it.
struct ContainerInfo: Equatable, Identifiable {
    let id: String
    let name: String
    let image: String
    /// A devcontainer's project folder on this Mac.
    var folder: String?
    /// The user VS Code runs as in a devcontainer (`remoteUser`), which the
    /// island runs as too; nil uses the container's own.
    var user: String?
    /// Where the project folder is inside it.
    var workspace: String?

    var isDevcontainer: Bool { folder != nil }

    /// The target the person picks for it.
    var target: ContainerTarget {
        folder != nil ? ContainerTarget(folder: folder) : ContainerTarget(name: name)
    }

    var title: String { target.title }
}

/// The Docker command line, or Podman's, which takes the same commands. The
/// first one found is used; Docker Desktop, OrbStack, Colima and Rancher
/// Desktop all put a `docker` here.
struct ContainerEngine: Equatable {
    let binary: String

    var name: String { URL(fileURLWithPath: binary).lastPathComponent }

    static func candidates(home: URL = FileManager.default.homeDirectoryForCurrentUser) -> [String] {
        ["/usr/local/bin/docker", "/opt/homebrew/bin/docker", home.appending(path: ".orbstack/bin/docker").path,
         home.appending(path: ".docker/bin/docker").path, "/Applications/Docker.app/Contents/Resources/bin/docker",
         home.appending(path: ".rd/bin/docker").path, "/usr/bin/docker",
         "/opt/homebrew/bin/podman", "/usr/local/bin/podman", "/opt/podman/bin/podman", "/usr/bin/podman"]
    }

    static func find(home: URL = FileManager.default.homeDirectoryForCurrentUser) -> ContainerEngine? {
        candidates(home: home).first { FileManager.default.isExecutableFile(atPath: $0) }.map(ContainerEngine.init)
    }

    /// The app's own environment has a short PATH; the CLI's helpers (a
    /// credential helper, Colima's or OrbStack's) live next to it.
    var environment: [String: String] {
        var environment = ProcessInfo.processInfo.environment
        let folder = (binary as NSString).deletingLastPathComponent
        let path = [folder, "/usr/local/bin", "/opt/homebrew/bin", environment["PATH"] ?? "/usr/bin:/bin:/usr/sbin:/sbin"]
        environment["PATH"] = path.joined(separator: ":")
        return environment
    }

    /// The agent script's loader, for a container: `docker exec` starts no
    /// login shell, so `sh` reads the script straight away.
    static let loader = #"IFS= read -r s; eval "$(printf %s "$s" | base64 -d)""#

    /// `docker exec -i` into the container as `user`, running the loader.
    func exec(_ container: String, user: String?) -> RemoteCommand {
        var arguments = ["exec", "-i"]
        if let user, !user.isEmpty { arguments += ["-u", user] }
        arguments += ["--", container, "sh", "-c", Self.loader]
        return RemoteCommand(executable: binary, arguments: arguments, environment: environment)
    }

    // MARK: Listing

    /// The running containers, or why they can't be listed.
    func running(timeout: TimeInterval = 10) -> Result<[ContainerInfo], ContainerListFailure> {
        let ids = run(["ps", "-q", "--no-trunc"], timeout: timeout)
        guard let ids, ids.status == 0 else {
            return .failure(.engine(ids.map { Self.reason($0.errors) } ?? "\(name) didn’t answer"))
        }
        let list = String(decoding: ids.output, as: UTF8.self).split(whereSeparator: \.isNewline).map(String.init)
            .filter { !$0.isEmpty && $0.allSatisfy { $0.isHexDigit } }
        guard !list.isEmpty else { return .success([]) }
        guard let inspected = run(["inspect", "--type", "container"] + list, timeout: timeout), inspected.status == 0 else {
            return .failure(.engine("\(name) inspect failed"))
        }
        return .success(Self.parse(inspected.output))
    }

    static func reason(_ errors: Data) -> String {
        let line = String(decoding: errors, as: UTF8.self).split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespaces) }.last { !$0.isEmpty }
        return line.map { String($0.prefix(160)) } ?? "The container engine isn’t running"
    }

    /// `docker inspect` output: Docker's and Podman's share these fields.
    static func parse(_ data: Data) -> [ContainerInfo] {
        guard let list = (try? JSONSerialization.jsonObject(with: data)) as? [[String: Any]] else { return [] }
        return list.compactMap { item in
            guard let id = item["Id"] as? String, !id.isEmpty else { return nil }
            let state = item["State"] as? [String: Any]
            if let running = state?["Running"] as? Bool, !running { return nil }
            let config = item["Config"] as? [String: Any] ?? [:]
            let labels = config["Labels"] as? [String: String] ?? [:]
            var name = (item["Name"] as? String) ?? ""
            if name.hasPrefix("/") { name.removeFirst() }
            var info = ContainerInfo(id: id, name: name.isEmpty ? String(id.prefix(12)) : name,
                                     image: config["Image"] as? String ?? "")
            if let folder = labels["devcontainer.local_folder"], !folder.isEmpty {
                info.folder = folder
                info.user = remoteUser(labels["devcontainer.metadata"])
                let mounts = item["Mounts"] as? [[String: Any]] ?? []
                info.workspace = mounts.first { ($0["Source"] as? String) == folder }?["Destination"] as? String
            }
            return info
        }
    }

    /// The `remoteUser` the devcontainer's configuration gave, the last one
    /// winning as with every merged setting.
    static func remoteUser(_ metadata: String?) -> String? {
        guard let metadata, let entries = (try? JSONSerialization.jsonObject(with: Data(metadata.utf8))) as? [Any] else {
            return nil
        }
        let users = entries.compactMap { ($0 as? [String: Any])?["remoteUser"] as? String }
        return users.last { !$0.isEmpty && !$0.contains(where: { $0.isWhitespace || $0.isNewline }) && $0.first != "-" }
    }

    struct Output {
        let status: Int32
        let output: Data
        let errors: Data
    }

    /// One short command of the CLI; nil when it can't start or takes too long.
    func run(_ arguments: [String], timeout: TimeInterval) -> Output? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: binary)
        process.arguments = arguments
        process.environment = environment
        let output = Pipe()
        let errors = Pipe()
        process.standardOutput = output
        process.standardError = errors
        process.standardInput = FileHandle.nullDevice
        let done = DispatchSemaphore(value: 0)
        process.terminationHandler = { _ in done.signal() }
        do { try process.run() } catch { return nil }
        var outData = Data()
        var errData = Data()
        let group = DispatchGroup()
        group.enter()
        DispatchQueue.global(qos: .utility).async {
            outData = output.fileHandleForReading.readDataToEndOfFile()
            group.leave()
        }
        group.enter()
        DispatchQueue.global(qos: .utility).async {
            errData = errors.fileHandleForReading.readDataToEndOfFile()
            group.leave()
        }
        guard done.wait(timeout: .now() + timeout) == .success else {
            process.terminate()
            return nil
        }
        _ = group.wait(timeout: .now() + 2)
        return Output(status: process.terminationStatus, output: outData, errors: errData)
    }
}

enum ContainerListFailure: Error, Equatable {
    /// No Docker or Podman command line on this Mac.
    case noEngine
    /// It is there but didn't answer: its VM or daemon isn't running.
    case engine(String)
}

/// The VS Code window of a devcontainer: `vscode://vscode-remote/` with the
/// Dev Containers authority, which brings an open window forward or opens one.
enum ContainerEditor {
    static func url(folder: String, workspace: String?) -> URL? {
        let hex = Data(folder.utf8).map { String(format: "%02x", $0) }.joined()
        let inside = workspace ?? "/workspaces/" + URL(fileURLWithPath: folder).lastPathComponent
        var components = URLComponents()
        components.scheme = "vscode"
        components.host = "vscode-remote"
        components.path = "/dev-container+" + hex + (inside.hasPrefix("/") ? inside : "/" + inside)
        return components.url
    }
}
