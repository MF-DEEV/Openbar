// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Darwin
import Foundation

/// One folder the island watches, with every reason it is watched.
struct NotchDownloadFolder: Identifiable, Equatable {
    enum Source: Equatable {
        /// The Mac's own Downloads folder.
        case downloads
        /// A browser saves there; the name is the browser's own.
        case browser(String)
        /// Someone added it in Settings. The bookmark carries its access.
        case chosen(Data)
    }

    let url: URL
    var sources: [Source]
    /// Listed but not watched: a chosen folder that no longer resolves, or
    /// one that exists but could not be read.
    var unavailable = false

    var id: String { url.path + (unavailable ? "#unavailable" : "") }

    var bookmarks: [Data] {
        sources.compactMap { if case let .chosen(data) = $0 { return data } else { return nil } }
    }

    var browsers: [String] {
        var seen = Set<String>()
        return sources.compactMap { if case let .browser(name) = $0 { return name } else { return nil } }
            .filter { seen.insert($0).inserted }
    }
}

/// Where downloads land on this Mac. Only folder paths are read from browser
/// settings; never history, URLs or file contents.
enum NotchDownloadFolders {
    /// Chromium browsers by their folder in Application Support. Each keeps
    /// its profiles as subfolders, or (like Opera) is a profile itself.
    static let chromiumBrowsers: [(name: String, path: String)] = [
        ("Chrome", "Google/Chrome"), ("Chrome Beta", "Google/Chrome Beta"),
        ("Chrome Dev", "Google/Chrome Dev"), ("Chrome Canary", "Google/Chrome Canary"),
        ("Chromium", "Chromium"),
        ("Edge", "Microsoft Edge"), ("Edge Beta", "Microsoft Edge Beta"),
        ("Edge Dev", "Microsoft Edge Dev"), ("Edge Canary", "Microsoft Edge Canary"),
        ("Brave", "BraveSoftware/Brave-Browser"), ("Brave Beta", "BraveSoftware/Brave-Browser-Beta"),
        ("Brave Nightly", "BraveSoftware/Brave-Browser-Nightly"),
        ("Arc", "Arc/User Data"), ("Vivaldi", "Vivaldi"),
        ("Opera", "com.operasoftware.Opera"), ("Opera GX", "com.operasoftware.OperaGX"),
        ("Yandex", "Yandex/YandexBrowser"),
    ]
    /// Firefox and the browsers built on it keep prefs.js per profile here,
    /// the same folders the island's cover search reads.
    static let firefoxBrowsers: [(name: String, path: String)] = [
        ("Firefox", "Firefox/Profiles"), ("Zen", "zen/Profiles"),
        ("LibreWolf", "librewolf/Profiles"), ("Waterfox", "Waterfox/Profiles"),
    ]
    static let maximumProfiles = 64
    /// Preferences beyond this are not a browser's settings file.
    static let maximumSettingsBytes = 32 << 20

    // MARK: Browser settings

    /// The save and "Save as" folders from a Chromium profile's Preferences.
    static func chromiumDirectories(fromPreferences data: Data) -> [String] {
        guard let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return [] }
        return ["download", "savefile"].compactMap {
            (root[$0] as? [String: Any])?["default_directory"] as? String
        }.filter { !$0.isEmpty }
    }

    /// Firefox's custom download folder. Its own folder list number says
    /// whether that path is in use: 0 is the Desktop, 1 (the default, written
    /// only when changed) the Mac's Downloads and 2 the custom path.
    static func firefoxDirectory(fromPrefs text: String) -> String? {
        var folderList: String?
        var directory: String?
        for line in text.split(whereSeparator: \.isNewline) {
            guard let (name, value) = userPref(String(line)) else { continue }
            switch (name, value) {
            case ("browser.download.folderList", .raw(let raw)): folderList = raw
            case ("browser.download.dir", .string(let path)): directory = path
            default: break
            }
        }
        return folderList == "2" ? directory.flatMap { $0.isEmpty ? nil : $0 } : nil
    }

    enum PrefValue: Equatable { case string(String), raw(String) }

    /// One `user_pref("name", value);` line from prefs.js.
    static func userPref(_ line: String) -> (String, PrefValue)? {
        let text = Array(line.trimmingCharacters(in: .whitespaces).unicodeScalars)
        let opening = Array("user_pref(".unicodeScalars)
        guard text.starts(with: opening) else { return nil }
        var index = opening.count
        func skipSpaces() { while index < text.count, text[index] == " " || text[index] == "\t" { index += 1 } }
        skipSpaces()
        guard let name = stringLiteral(text, at: &index) else { return nil }
        skipSpaces()
        guard index < text.count, text[index] == "," else { return nil }
        index += 1
        skipSpaces()
        guard index < text.count else { return nil }
        if text[index] == "\"" || text[index] == "'" {
            guard let value = stringLiteral(text, at: &index) else { return nil }
            skipSpaces()
            guard index < text.count, text[index] == ")" else { return nil }
            return (name, .string(value))
        }
        guard let end = text[index...].firstIndex(of: ")") else { return nil }
        var raw = String.UnicodeScalarView()
        raw.append(contentsOf: text[index..<end])
        return (name, .raw(String(raw).trimmingCharacters(in: .whitespaces)))
    }

    /// A quoted prefs.js string with Firefox's escapes: \\, \", \', \n, \r,
    /// \t, \xHH and \uHHHH (paired for characters beyond the first plane).
    static func stringLiteral(_ text: [Unicode.Scalar], at index: inout Int) -> String? {
        guard index < text.count, text[index] == "\"" || text[index] == "'" else { return nil }
        let quote = text[index]
        index += 1
        var result = String.UnicodeScalarView()
        var pendingHigh: UInt32?
        func hex(_ count: Int) -> UInt32? {
            guard index + count <= text.count else { return nil }
            var value: UInt32 = 0
            for scalar in text[index..<index + count] {
                guard let digit = Character(scalar).hexDigitValue else { return nil }
                value = value * 16 + UInt32(digit)
            }
            index += count
            return value
        }
        while index < text.count {
            let scalar = text[index]
            index += 1
            if scalar == quote { return pendingHigh == nil ? String(result) : nil }
            guard scalar == "\\" else {
                guard pendingHigh == nil else { return nil }
                result.append(scalar)
                continue
            }
            guard index < text.count else { return nil }
            let escape = text[index]
            index += 1
            var code: UInt32
            switch escape {
            case "n": code = 0x0A
            case "r": code = 0x0D
            case "t": code = 0x09
            case "x": guard let value = hex(2) else { return nil }; code = value
            case "u": guard let value = hex(4) else { return nil }; code = value
            default: code = escape.value
            }
            if let high = pendingHigh {
                guard (0xDC00...0xDFFF).contains(code) else { return nil }
                code = 0x10000 + ((high - 0xD800) << 10) + (code - 0xDC00)
                pendingHigh = nil
            } else if (0xD800...0xDBFF).contains(code) {
                pendingHigh = code
                continue
            }
            guard let decoded = Unicode.Scalar(code) else { return nil }
            result.append(decoded)
        }
        return nil
    }

    /// Every browser profile's download folder, by browser name. Folders that
    /// do not exist are left to the caller, which watches only real ones.
    static func browserFolders(home: URL) -> [(browser: String, url: URL)] {
        let support = home.appendingPathComponent("Library/Application Support", isDirectory: true)
        var found: [(String, URL)] = []
        for browser in chromiumBrowsers {
            let root = support.appendingPathComponent(browser.path, isDirectory: true)
            for profile in profiles(in: root, including: root) {
                guard let data = settings(at: profile.appendingPathComponent("Preferences")) else { continue }
                for path in chromiumDirectories(fromPreferences: data) {
                    if let url = directoryURL(path, home: home) { found.append((browser.name, url)) }
                }
            }
        }
        for browser in firefoxBrowsers {
            let root = support.appendingPathComponent(browser.path, isDirectory: true)
            for profile in profiles(in: root, including: nil) {
                guard let data = settings(at: profile.appendingPathComponent("prefs.js")),
                      let text = String(data: data, encoding: .utf8),
                      let path = firefoxDirectory(fromPrefs: text),
                      let url = directoryURL(path, home: home) else { continue }
                found.append((browser.name, url))
            }
        }
        return found
    }

    private static func profiles(in root: URL, including base: URL?) -> [URL] {
        let children = (try? FileManager.default.contentsOfDirectory(at: root,
            includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey],
            options: [.skipsHiddenFiles, .skipsSubdirectoryDescendants])) ?? []
        let folders = children.filter {
            let values = try? $0.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
            return values?.isDirectory == true && values?.isSymbolicLink != true
        }.sorted { $0.path < $1.path }
        return Array(([base].compactMap { $0 } + folders).prefix(maximumProfiles))
    }

    private static func settings(at url: URL) -> Data? {
        guard let values = try? url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey]),
              values.isRegularFile == true, (values.fileSize ?? 0) <= maximumSettingsBytes else { return nil }
        return try? Data(contentsOf: url, options: .mappedIfSafe)
    }

    /// An absolute folder path, or one under home written with a tilde.
    static func directoryURL(_ path: String, home: URL) -> URL? {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        let absolute: String
        if trimmed == "~" || trimmed.hasPrefix("~/") {
            absolute = home.path + trimmed.dropFirst()
        } else {
            absolute = trimmed
        }
        guard absolute.hasPrefix("/"), !absolute.contains("\0") else { return nil }
        return URL(fileURLWithPath: absolute, isDirectory: true).standardizedFileURL
    }

    // MARK: One list

    /// The same folder can arrive as a symlink, in another letter case or
    /// through a firmlink. Its device and inode name it once; a path that is
    /// not a folder has no identity and so never gets a watcher.
    static func identity(of url: URL) -> String? {
        var info = stat()
        guard url.withUnsafeFileSystemRepresentation({ path in
            path.map { stat($0, &info) == 0 } ?? false
        }), (info.st_mode & S_IFMT) == S_IFDIR else { return nil }
        return "\(info.st_dev):\(info.st_ino)"
    }

    /// Folders in order of first mention, each physical folder once with
    /// every source that named it. A folder inside another stays separate:
    /// each scan reads only its own top level, so they never share a file.
    static func merged(_ candidates: [(url: URL, source: NotchDownloadFolder.Source)],
                       identity: (URL) -> String? = identity(of:)) -> [NotchDownloadFolder] {
        var result: [NotchDownloadFolder] = []
        var positions: [String: Int] = [:]
        for candidate in candidates {
            guard let key = identity(candidate.url) else { continue }
            if let position = positions[key] {
                if !result[position].sources.contains(candidate.source) {
                    result[position].sources.append(candidate.source)
                }
            } else {
                positions[key] = result.count
                // Watch the real path: the watcher refuses a symlink.
                result.append(NotchDownloadFolder(url: candidate.url.resolvingSymlinksInPath().standardizedFileURL,
                                                  sources: [candidate.source]))
            }
        }
        return result
    }

    /// The Mac's Downloads folder, each browser's and the chosen ones, as one
    /// list. Chosen folders that could not be resolved follow, unavailable.
    static func discover(chosen: [(url: URL, bookmark: Data)], missing: [Data],
                         downloads: URL? = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first,
                         home: URL = FileManager.default.homeDirectoryForCurrentUser) -> [NotchDownloadFolder] {
        var candidates: [(url: URL, source: NotchDownloadFolder.Source)] = []
        if let downloads { candidates.append((downloads.standardizedFileURL, .downloads)) }
        candidates += browserFolders(home: home).map { ($0.url, .browser($0.browser)) }
        candidates += chosen.map { ($0.url.standardizedFileURL, .chosen($0.bookmark)) }
        var result = merged(candidates)
        // A chosen folder that is gone for now, like one on an ejected disk,
        // stays listed so its row can say so; it returns when it does.
        let represented = Set(result.flatMap(\.bookmarks))
        let absent: [(URL?, Data)] = chosen.filter { !represented.contains($0.bookmark) }
            .map { ($0.url.standardizedFileURL, $0.bookmark) }
        for (stored, data) in absent + missing.map({ (storedURL(of: $0), $0) }) {
            let url = stored ?? URL(fileURLWithPath: "/", isDirectory: true)
            if let position = result.firstIndex(where: { $0.unavailable && $0.url == url }) {
                result[position].sources.append(.chosen(data))
            } else {
                result.append(NotchDownloadFolder(url: url, sources: [.chosen(data)], unavailable: true))
            }
        }
        return result
    }

    /// Where a bookmark pointed, read from the bookmark alone.
    static func storedURL(of bookmark: Data) -> URL? {
        guard let path = URL.resourceValues(forKeys: [.pathKey], fromBookmarkData: bookmark)?.path else { return nil }
        return URL(fileURLWithPath: path, isDirectory: true).standardizedFileURL
    }
}
