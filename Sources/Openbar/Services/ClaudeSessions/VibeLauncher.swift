// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Mistral Vibe's terminal UI started so that its app-server runs through the
/// island's proxy (`VibeProxy`), which lets the island answer its approvals
/// and questions. Vibe's launcher takes the server's command from its own
/// `exec_rust_cli(app_server_cmd:)`, so the line runs that from Vibe's Python
/// and ends in the same terminal UI `vibe` starts. Where Vibe is not set up
/// as this expects, the plain `vibe` runs and its hooks report as before.
enum VibeLauncher {
    struct Install: Equatable {
        /// The Python Vibe is installed into.
        let python: String
        /// Vibe's own `vibe-app-server`.
        let server: String
    }

    /// The code the Python runs: the first argument is the server's command
    /// as JSON, the rest is what `vibe` is given.
    static let code = "import json,sys;from vibe.cli.launcher import exec_rust_cli;"
        + "exec_rust_cli(sys.argv[2:],app_server_cmd=json.loads(sys.argv[1]))"

    /// Where Vibe's Python and server are, from the `vibe` found on the path.
    /// `read` gives a file's text.
    static func install(vibe: String, isExecutable: (String) -> Bool = FileManager.default.isExecutableFile(atPath:),
                        read: (String) -> String? = { try? String(contentsOfFile: $0, encoding: .utf8) }) -> Install? {
        let real = URL(fileURLWithPath: vibe).resolvingSymlinksInPath()
        let folder = real.deletingLastPathComponent()
        let server = folder.appending(path: "vibe-app-server").path
        guard isExecutable(server) else { return nil }
        var candidates = python(inScript: read(real.path) ?? "")
        candidates += ["python", "python3"].map { folder.appending(path: $0).path }
        guard let python = candidates.first(where: isExecutable) else { return nil }
        return Install(python: python, server: server)
    }

    /// The interpreter a console script names: uv's and pipx's start with
    /// `#!/bin/sh` and a line `'''exec' '<python>' "$0" "$@"`, others with the
    /// interpreter as the shebang.
    static func python(inScript text: String) -> [String] {
        var found: [String] = []
        for line in text.split(separator: "\n", omittingEmptySubsequences: false).prefix(4).map(String.init) {
            if line.hasPrefix("#!") {
                let path = String(line.dropFirst(2)).trimmingCharacters(in: .whitespaces)
                if !path.hasSuffix("/sh"), !path.hasSuffix("/bash"), !path.contains(" ") { found.append(path) }
            } else if line.hasPrefix("'''exec'") {
                let parts = line.split(separator: "'", omittingEmptySubsequences: true).map(String.init)
                if let path = parts.first(where: { $0.hasPrefix("/") && $0.contains("python") }) { found.append(path) }
            }
        }
        return found
    }

    /// The line a terminal runs: Vibe's Python starts the terminal UI with the
    /// island's proxy as its server, then the first prompt, as `vibe` takes it.
    static func line(install: Install, proxy: String, socket: String, prompt: String?, folder: String?) -> String? {
        let command = [proxy, VibeProxy.flag, socket, install.server]
        guard let data = try? JSONSerialization.data(withJSONObject: command), let json = String(data: data, encoding: .utf8)
        else { return nil }
        var arguments = ["-I", "-c", code, json]
        if let prompt, !prompt.isEmpty { arguments.append(prompt) }
        return CodexCLI.line(executable: install.python, arguments: arguments, folder: folder)
    }
}
