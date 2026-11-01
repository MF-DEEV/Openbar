// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Darwin
import Foundation

/// What a Chromium browser itself counts for one running download.
struct NotchBrowserDownload: Equatable {
    /// The partial file it writes, as the browser names it.
    let currentPath: String
    /// The name it will get once complete.
    let targetPath: String
    let receivedBytes: Int64
    /// Nil when the server did not say.
    let totalBytes: Int64?
    let paused: Bool
}

/// Chromium keeps each running download's byte counts in its profile's
/// `shared_proto_db`, a LevelDB store; its History database only ever gets
/// downloads that have ended. This reads that store's files, never as a
/// writer and without its lock, so a browser that holds the store open is not
/// disturbed. Of each record only the paths, byte counts, state and pause
/// flag are decoded: URL, referrer and tab fields are skipped by their length
/// without being read, and nothing else in the profile is opened.
/// Record format: components/download/database/proto/download_entry.proto.
enum NotchChromiumDownloads {
    /// leveldb_proto's DOWNLOAD_DB client (21) and the browser namespace.
    static let keyPrefix = Array("21_download,".utf8)
    static let storeName = "shared_proto_db"
    /// Running downloads decoded from one read, newest records first.
    static let maximumDownloads = 64
    /// A store beyond this is not a profile's download store.
    static let maximumStoreBytes = 32 << 20
    /// Profiles are found again this often; the store itself on every read.
    static let storeInterval: TimeInterval = 30

    // MARK: Records

    private enum Field {
        static let downloadInfo = 1, inProgressInfo = 4
        static let totalBytes = 10, currentPath = 13, targetPath = 14, receivedBytes = 15
        static let state = 21, paused = 24
    }

    /// Chromium's download states: only a running one is kept.
    private static let inProgress = 0

    /// One protobuf message's fields, calling `field` for each with its
    /// number, wire type and payload. Length-delimited payloads the caller
    /// does not ask for are stepped over unread.
    private static func fields(_ bytes: ArraySlice<UInt8>,
                               _ field: (Int, UInt64, ArraySlice<UInt8>) -> Void) -> Bool {
        var cursor = LevelDB.Cursor(bytes)
        while !cursor.atEnd {
            guard let tag = cursor.varint64(), tag >> 3 > 0, tag >> 3 <= UInt64(Int32.max) else { return false }
            let number = Int(tag >> 3)
            switch tag & 7 {
            case 0:
                guard let value = cursor.varint64() else { return false }
                field(number, value, [])
            case 1:
                guard cursor.take(8) != nil else { return false }
            case 2:
                guard let payload = cursor.string() else { return false }
                field(number, 0, payload)
            case 5:
                guard cursor.take(4) != nil else { return false }
            default:
                return false
            }
        }
        return true
    }

    /// A base::Pickle holding one FilePath: the payload size, then the path
    /// as a length and its UTF-8 bytes, padded to four.
    static func pickledPath(_ bytes: ArraySlice<UInt8>) -> String? {
        guard let payload = LevelDB.u32(bytes), Int(payload) == bytes.count - 4,
              let length = LevelDB.u32(bytes.dropFirst(4)).map(Int.init), length > 0,
              length <= bytes.count - 8 else { return nil }
        let start = bytes.startIndex + 8
        guard let path = String(bytes: bytes[start..<start + length], encoding: .utf8),
              path.hasPrefix("/"), !path.contains("\0") else { return nil }
        return path
    }

    /// A running download from one record, or nil for any other record,
    /// including one that has ended or cannot be read.
    static func download(fromRecord value: ArraySlice<UInt8>) -> NotchBrowserDownload? {
        var info: ArraySlice<UInt8>?
        guard fields(value, { number, _, payload in
            if number == Field.downloadInfo { info = payload }
        }), let info else { return nil }
        var progress: ArraySlice<UInt8>?
        guard fields(info, { number, _, payload in
            if number == Field.inProgressInfo { progress = payload }
        }), let progress else { return nil }
        var current: String?, target: String?
        var received: UInt64?, total: UInt64?, state: UInt64?, paused = false
        guard fields(progress, { number, scalar, payload in
            switch number {
            case Field.currentPath: current = pickledPath(payload)
            case Field.targetPath: target = pickledPath(payload)
            case Field.receivedBytes: received = scalar
            case Field.totalBytes: total = scalar
            case Field.state: state = scalar
            case Field.paused: paused = scalar != 0
            default: break
            }
        }) else { return nil }
        // Counts are int64 on the wire; a negative one arrives huge.
        guard state == UInt64(inProgress), let current, let target,
              let received, let bytes = Int64(exactly: received) else { return nil }
        let known = total.flatMap { Int64(exactly: $0) }.flatMap { $0 > 0 ? $0 : nil }
        return NotchBrowserDownload(currentPath: current, targetPath: target, receivedBytes: bytes,
                                    totalBytes: known, paused: paused)
    }

    // MARK: Stores

    /// Every Chromium profile's download store under Application Support.
    static func stores(home: URL) -> [URL] {
        let support = home.appendingPathComponent("Library/Application Support", isDirectory: true)
        var found: [URL] = []
        for browser in NotchDownloadFolders.chromiumBrowsers {
            let root = support.appendingPathComponent(browser.path, isDirectory: true)
            for profile in NotchDownloadFolders.profiles(in: root, including: root) {
                let store = profile.appendingPathComponent(storeName, isDirectory: true)
                guard let values = try? store.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey]),
                      values.isDirectory == true, values.isSymbolicLink != true else { continue }
                found.append(store)
            }
        }
        return found
    }

    /// Reads every profile's store, decoding a file again only when its size
    /// or date changes. Confined to one queue by its owner. A store that is
    /// missing, locked against reading, mid-write or of an unknown shape
    /// simply gives nothing, and the folder scan carries on without it.
    final class Reader {
        private struct Fingerprint: Equatable {
            let size: UInt64
            let modified: Date
        }

        private struct Entry {
            let sequence: UInt64
            let download: NotchBrowserDownload?
        }

        private let home: URL
        private let now: () -> Date
        private var stores: [URL] = []
        private var storesFound: Date?
        private var files: [URL: (fingerprint: Fingerprint, entries: [[UInt8]: Entry])] = [:]

        init(home: URL = FileManager.default.homeDirectoryForCurrentUser, now: @escaping () -> Date = Date.init) {
            self.home = home
            self.now = now
        }

        func read() -> [NotchBrowserDownload] {
            if storesFound.map({ now().timeIntervalSince($0) >= storeInterval }) ?? true {
                stores = NotchChromiumDownloads.stores(home: home)
                storesFound = now()
            }
            var seen = Set<URL>()
            var result: [(UInt64, NotchBrowserDownload)] = []
            for store in stores {
                result += read(store, seen: &seen)
            }
            files = files.filter { seen.contains($0.key) }
            return result.sorted { $0.0 > $1.0 }.prefix(maximumDownloads).map(\.1)
        }

        private func read(_ store: URL, seen: inout Set<URL>) -> [(UInt64, NotchBrowserDownload)] {
            let manager = FileManager.default
            guard let entries = try? manager.contentsOfDirectory(at: store,
                      includingPropertiesForKeys: [.fileSizeKey, .contentModificationDateKey, .isRegularFileKey]) else {
                return []
            }
            var latest: [[UInt8]: Entry] = [:]
            var total: UInt64 = 0
            for path in entries where LevelDB.isDataFile(path) {
                guard let values = try? path.resourceValues(forKeys: [.fileSizeKey, .contentModificationDateKey,
                                                                      .isRegularFileKey]),
                      values.isRegularFile == true, let size = values.fileSize.map(UInt64.init),
                      let modified = values.contentModificationDate else { continue }
                total += size
                // An oversized store is skipped whole: half of it could
                // resurrect downloads its newer half removed.
                guard total <= UInt64(maximumStoreBytes) else { return [] }
                let fingerprint = Fingerprint(size: size, modified: modified)
                if files[path]?.fingerprint != fingerprint {
                    guard let data = try? Data(contentsOf: path, options: .mappedIfSafe),
                          data.count <= maximumStoreBytes else { return [] }
                    let bytes = [UInt8](data)
                    var found: [[UInt8]: Entry] = [:]
                    let visit: LevelDB.Visit = { key, value, sequence, kind in
                        guard key.starts(with: NotchChromiumDownloads.keyPrefix) else { return }
                        let id = Array(key)
                        guard (found[id]?.sequence).map({ sequence > $0 }) ?? true else { return }
                        found[id] = Entry(sequence: sequence,
                                          download: kind == 1 ? NotchChromiumDownloads.download(fromRecord: value) : nil)
                    }
                    if path.pathExtension == "log" {
                        LevelDB.visitLog(bytes, visit: visit)
                    } else if !LevelDB.visitTable(bytes, relevant: { after, upTo in
                        LevelDB.range(after: after, upTo: upTo, holdsPrefix: NotchChromiumDownloads.keyPrefix)
                    }, visit: visit) {
                        // A table being replaced mid-read; the next read sees the new one.
                        return []
                    }
                    files[path] = (fingerprint, found)
                }
                seen.insert(path)
                for (key, entry) in files[path]?.entries ?? [:]
                where (latest[key]?.sequence).map({ entry.sequence > $0 }) ?? true {
                    latest[key] = entry
                }
            }
            return latest.values.compactMap { entry in entry.download.map { (entry.sequence, $0) } }
        }
    }

    // MARK: Partials

    /// The browser's record for each partial it is writing, matched by the
    /// file itself rather than its spelling: the browser can name a folder
    /// through a symlink (/tmp for /private/tmp) or in another letter case.
    /// A partial the store says nothing of this time keeps what it was given
    /// before, while it is still the same file.
    static func annotate(_ partials: [URL: NotchPartialDownload], previous: [URL: NotchPartialDownload],
                         downloads: [NotchBrowserDownload]) -> [URL: NotchPartialDownload] {
        guard !partials.isEmpty else { return partials }
        var byFile: [String: NotchBrowserDownload] = [:]
        for download in downloads {
            guard let identity = NotchDownloadSupport.fileIdentity(at: URL(fileURLWithPath: download.currentPath)),
                  byFile[identity] == nil else { continue }
            byFile[identity] = download
        }
        var result = partials
        for (url, partial) in partials {
            if let identity = partial.resourceID, partial.contentURL == nil, let download = byFile[identity] {
                result[url] = partial.annotated(with: download)
            } else if let earlier = previous[url], earlier.browser != nil,
                      earlier.resourceID == partial.resourceID, partial.contentURL == nil {
                result[url] = partial.annotated(with: earlier.browser!.download)
            }
        }
        return result
    }

    /// Bytes received and the share done for a partial. A browser's count
    /// beats the file's length, which can be preallocated or sparse; the
    /// length only moves the count on between the browser's own saves, every
    /// ten seconds or so, while the file is plainly being written in order.
    static func progress(of partial: NotchPartialDownload) -> (bytes: Int64, fraction: Double?) {
        guard let browser = partial.browser else { return (partial.bytes, nil) }
        var bytes = max(0, browser.download.receivedBytes)
        if let total = browser.download.totalBytes, browser.dense,
           partial.bytes > bytes, partial.bytes < total {
            bytes = partial.bytes
        }
        guard let total = browser.download.totalBytes, total > 0 else { return (bytes, nil) }
        return (bytes, min(1, Double(bytes) / Double(total)))
    }

    /// Whether the file's blocks cover its whole length, so it was written in
    /// order rather than stretched ahead of its data.
    static func isDense(_ url: URL) -> Bool {
        var info = stat()
        guard url.withUnsafeFileSystemRepresentation({ path in
            path.map { lstat($0, &info) == 0 } ?? false
        }), (info.st_mode & S_IFMT) == S_IFREG else { return false }
        return Int64(info.st_blocks) * 512 >= Int64(info.st_size)
    }
}

extension NotchPartialDownload {
    /// The browser's record and whether the file was written in order.
    struct Browser: Equatable {
        let download: NotchBrowserDownload
        let dense: Bool
    }

    /// Chromium first writes a dangerous-looking download as "Unconfirmed
    /// 123.crdownload" and renames it at the end. Its record names the real
    /// file, which becomes the name shown and the file that proves it
    /// finished, as long as it lands in this same folder.
    func annotated(with download: NotchBrowserDownload) -> NotchPartialDownload {
        let folder = url.deletingLastPathComponent()
        let target = URL(fileURLWithPath: download.targetPath)
        var expected = expectedURL
        let name = target.lastPathComponent
        if !name.isEmpty, NotchDownloadSupport.expectedURL(for: target) == nil,
           let here = NotchDownloadFolders.identity(of: folder),
           NotchDownloadFolders.identity(of: target.deletingLastPathComponent()) == here {
            expected = folder.appendingPathComponent(name).standardizedFileURL
        }
        var copy = NotchPartialDownload(url: url, expectedURL: expected, bytes: bytes, resourceID: resourceID,
                                        modified: modified, contentURL: contentURL, source: source)
        copy.browser = Browser(download: download, dense: NotchChromiumDownloads.isDense(url))
        return copy
    }
}
