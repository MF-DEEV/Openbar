// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Darwin
import Foundation

/// Chromium's in-progress download store, as real files in disposable
/// profiles: records, the LevelDB journal and tables, several profiles, and
/// the folder scan that merges the browser's counts with the partial files.
enum NotchChromiumDownloadTests {
    static func run(_ suite: TestSuite) {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("notch-chromium-\(UUID().uuidString)", isDirectory: true)
        do {
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
            defer {
                chmodTree(root)
                try? FileManager.default.removeItem(at: root)
            }
            records(suite)
            try stores(suite, root: root)
            try tables(suite, root: root)
            try damagedStores(suite, root: root)
            try partials(suite, root: root)
            try scanCompletion(suite, root: root)
            try scanWithoutPartials(suite, root: root)
        } catch { suite.expect(false, "chromium download fixture failed: \(error)") }
    }

    // MARK: Encoding

    private static func varint(_ value: UInt64, _ out: inout [UInt8]) {
        var number = value
        while number >= 128 { out.append(UInt8(number & 127) | 128); number >>= 7 }
        out.append(UInt8(number))
    }

    private static func le32(_ value: UInt32) -> [UInt8] { (0..<4).map { UInt8(truncatingIfNeeded: value >> ($0 * 8)) } }
    private static func le64(_ value: UInt64) -> [UInt8] { (0..<8).map { UInt8(truncatingIfNeeded: value >> ($0 * 8)) } }

    private static func field(_ number: Int, _ value: UInt64) -> [UInt8] {
        var out: [UInt8] = []
        varint(UInt64(number) << 3, &out)
        varint(value, &out)
        return out
    }

    private static func field(_ number: Int, bytes: [UInt8]) -> [UInt8] {
        var out: [UInt8] = []
        varint(UInt64(number) << 3 | 2, &out)
        varint(UInt64(bytes.count), &out)
        return out + bytes
    }

    private static func pickle(_ path: String) -> [UInt8] {
        let utf8 = Array(path.utf8)
        let padded = utf8 + [UInt8](repeating: 0, count: (4 - utf8.count % 4) % 4)
        return le32(UInt32(4 + padded.count)) + le32(UInt32(utf8.count)) + padded
    }

    /// A DownloadDBEntry as Chromium writes it, URLs and all.
    static func record(current: String, target: String, received: Int64, total: Int64?, state: UInt64 = 0,
                       paused: Bool = false, extra: [UInt8] = []) -> [UInt8] {
        var progress = field(1, bytes: Array("https://example.com/private?token=1".utf8))
            + field(2, bytes: Array("https://example.com/referrer".utf8))
            + field(4, bytes: Array("https://example.com/tab".utf8))
        if let total { progress += field(10, UInt64(bitPattern: total)) }
        progress += field(13, bytes: pickle(current)) + field(14, bytes: pickle(target))
            + field(15, UInt64(bitPattern: received)) + field(21, state) + field(24, paused ? 1 : 0)
            // A fixed64 and a fixed32 field this reader does not know.
            + [0x89, 0x02] + le64(7) + [0x95, 0x02] + le32(9) + extra
        let info = field(1, bytes: Array("guid".utf8)) + field(2, 3) + field(4, bytes: progress)
        return field(1, bytes: info)
    }

    private static func key(_ guid: String) -> [UInt8] { Array("21_download,\(guid)".utf8) }

    private static func batch(_ sequence: UInt64, _ operations: [([UInt8], [UInt8]?)]) -> [UInt8] {
        var out = le64(sequence) + le32(UInt32(operations.count))
        for (key, value) in operations {
            out.append(value == nil ? 0 : 1)
            varint(UInt64(key.count), &out)
            out += key
            if let value { varint(UInt64(value.count), &out); out += value }
        }
        return out
    }

    private static func log(_ batches: [[UInt8]]) -> [UInt8] {
        var out: [UInt8] = []
        for batch in batches {
            var rest = batch[...]
            var first = true
            while !rest.isEmpty {
                var left = LevelDB.logBlock - out.count % LevelDB.logBlock
                if left < 7 { out += [UInt8](repeating: 0, count: left); left = LevelDB.logBlock }
                let length = min(rest.count, left - 7)
                let last = length == rest.count
                let kind: UInt8 = first && last ? 1 : first ? 2 : last ? 4 : 3
                let payload = rest.prefix(length)
                out += le32(LevelDB.checksum(kind: kind, payload, kindFirst: true))
                out += [UInt8(length & 255), UInt8(length >> 8), kind]
                out += payload
                rest = rest.dropFirst(length)
                first = false
            }
        }
        return out
    }

    private static func appendBlock(_ out: inout [UInt8], _ entries: [([UInt8], [UInt8])]) -> [UInt8] {
        var handle: [UInt8] = []
        varint(UInt64(out.count), &handle)
        var payload: [UInt8] = []
        for (key, value) in entries {
            payload.append(0)
            varint(UInt64(key.count), &payload)
            varint(UInt64(value.count), &payload)
            payload += key + value
        }
        payload += le32(0) + le32(1)
        varint(UInt64(payload.count), &handle)
        out += payload
        out.append(0)
        out += le32(LevelDB.checksum(kind: 0, payload[...], kindFirst: false))
        return handle
    }

    /// A sorted table, one data block per group so blocks the prefix cannot
    /// reach stay unread.
    private static func table(_ groups: [[([UInt8], UInt64, [UInt8]?)]]) -> [UInt8] {
        var out: [UInt8] = []
        var index: [([UInt8], [UInt8])] = []
        for group in groups {
            let entries = group.map { key, sequence, value in
                (key + le64(sequence << 8 | (value == nil ? 0 : 1)), value ?? [])
            }
            index.append((entries.last!.0, appendBlock(&out, entries)))
        }
        let handle = appendBlock(&out, index)
        var footer: [UInt8] = [0, 0] + handle
        footer += [UInt8](repeating: 0, count: 40 - footer.count)
        return out + footer + [0x57, 0xfb, 0x80, 0x8b, 0x24, 0x75, 0x47, 0xdb]
    }

    private static func profile(_ home: URL, _ browser: String, _ name: String?) throws -> URL {
        var url = home.appendingPathComponent("Library/Application Support/\(browser)", isDirectory: true)
        if let name { url.appendPathComponent(name, isDirectory: true) }
        let store = url.appendingPathComponent(NotchChromiumDownloads.storeName, isDirectory: true)
        try FileManager.default.createDirectory(at: store, withIntermediateDirectories: true)
        return store
    }

    private static func chmodTree(_ url: URL) {
        guard let paths = FileManager.default.enumerator(atPath: url.path) else { return }
        for case let path as String in paths {
            _ = try? FileManager.default.setAttributes([.posixPermissions: 0o755],
                                                       ofItemAtPath: url.appendingPathComponent(path).path)
        }
    }

    // MARK: Records

    private static func records(_ suite: TestSuite) {
        let decode = { (bytes: [UInt8]) in NotchChromiumDownloads.download(fromRecord: bytes[...]) }
        let known = decode(record(current: "/d/Unconfirmed 1.crdownload", target: "/d/a.zip", received: 32, total: 100))
        suite.expect(known == NotchBrowserDownload(currentPath: "/d/Unconfirmed 1.crdownload", targetPath: "/d/a.zip",
                                                   receivedBytes: 32, totalBytes: 100, paused: false),
                     "a running record gives its paths, received bytes and known total, past fields it does not know")
        suite.expect(decode(record(current: "/d/a.part", target: "/d/a", received: 5, total: nil))?.totalBytes == nil
                     && decode(record(current: "/d/a.part", target: "/d/a", received: 5, total: 0))?.totalBytes == nil
                     && decode(record(current: "/d/a.part", target: "/d/a", received: 5, total: -1))?.totalBytes == nil,
                     "a missing, zero or negative total is unknown, not a share of zero")
        suite.expect(decode(record(current: "/d/a.part", target: "/d/a", received: 5, total: 9, paused: true))?.paused == true,
                     "a paused download says so")
        suite.expect(decode(record(current: "/d/a.part", target: "/d/a", received: 9, total: 9, state: 1)) == nil
                     && decode(record(current: "/d/a.part", target: "/d/a", received: 1, total: 9, state: 2)) == nil
                     && decode(record(current: "/d/a.part", target: "/d/a", received: 1, total: 9, state: 3)) == nil,
                     "completed, cancelled and interrupted records are left out")
        suite.expect(decode(record(current: "/d/a.part", target: "/d/a", received: -4, total: 9)) == nil,
                     "a negative received count is not a download")
        suite.expect(decode(record(current: "relative.part", target: "/d/a", received: 1, total: 9)) == nil
                     && decode(record(current: "", target: "/d/a", received: 1, total: 9)) == nil,
                     "an empty or relative path is not a file the island can match")
        let valid = record(current: "/d/a.part", target: "/d/a", received: 1, total: 9)
        suite.expect(decode(Array(valid.dropLast(3))) == nil,
                     "a truncated record is unreadable rather than half read")
        suite.expect(decode(record(current: "/d/a.part", target: "/d/a", received: 1, total: 9, extra: [0x9b, 0x02])) == nil,
                     "a group wire type, which the format no longer uses, makes the record unreadable")
        var badPickle = pickle("/d/a.part")
        badPickle[0] = 99
        suite.expect(NotchChromiumDownloads.pickledPath(badPickle[...]) == nil
                     && NotchChromiumDownloads.pickledPath(pickle("/d/a.part")[...]) == "/d/a.part"
                     && NotchChromiumDownloads.pickledPath(Array(pickle("/d/é.part"))[...]) == "/d/é.part",
                     "a pickled path must agree with its own payload size and holds UTF-8")
    }

    // MARK: Stores

    private static func stores(_ suite: TestSuite, root: URL) throws {
        let home = root.appendingPathComponent("stores-home", isDirectory: true)
        let chrome = try profile(home, "Google/Chrome", "Default")
        let second = try profile(home, "Google/Chrome", "Profile 1")
        let opera = try profile(home, "com.operasoftware.Opera", nil)
        try FileManager.default.createDirectory(at: home.appendingPathComponent(
            "Library/Application Support/Google/Chrome/Profile 2"), withIntermediateDirectories: true)
        let found = Set(NotchChromiumDownloads.stores(home: home).map(\.standardizedFileURL))
        suite.expect(found == Set([chrome, second, opera].map(\.standardizedFileURL)),
                     "every profile's store is found, including a browser that is a profile itself")

        let first = record(current: "/d/one.crdownload", target: "/d/one", received: 1, total: 10)
        let later = record(current: "/d/one.crdownload", target: "/d/one", received: 7, total: 10)
        let removed = record(current: "/d/two.crdownload", target: "/d/two", received: 2, total: 10)
        let other = Array("22_other,thing".utf8)
        try Data(log([
            batch(1, [(key("a"), first), (other, first)]),
            batch(3, [(key("b"), removed)]),
            batch(5, [(key("a"), later)]),
            batch(6, [(key("b"), nil)]),
        ]) + [1, 2, 3, 4, 5, 6, 7, 8, 9]).write(to: chrome.appendingPathComponent("000003.log"))
        try Data(log([batch(2, [(key("c"), record(current: "/e/three.part", target: "/e/three",
                                                   received: 4, total: nil))])]))
            .write(to: second.appendingPathComponent("000005.log"))
        try Data("not a store".utf8).write(to: opera.appendingPathComponent("LOG"))
        var reads = 0
        let reader = NotchChromiumDownloads.Reader(home: home, now: { reads += 1; return Date() })
        let downloads = reader.read()
        suite.expect(Set(downloads.map(\.currentPath)) == ["/d/one.crdownload", "/e/three.part"],
                     "each record's newest version counts, deleted ones and other clients' keys do not, across profiles")
        suite.expect(downloads.first { $0.currentPath == "/d/one.crdownload" }?.receivedBytes == 7,
                     "a later save of the same download replaces the earlier count")
        suite.expect(reads > 0, "the reader looks for profiles when it reads")

        // The store changes as the browser saves again.
        try Data(log([batch(9, [(key("a"), nil)])])).write(to: chrome.appendingPathComponent("000007.log"))
        suite.expect(Set(reader.read().map(\.currentPath)) == ["/e/three.part"],
                     "a removal in a newer journal file ends the download")
        try FileManager.default.removeItem(at: second.appendingPathComponent("000005.log"))
        suite.expect(reader.read().isEmpty, "a file that went away takes its downloads with it")
    }

    private static func tables(_ suite: TestSuite, root: URL) throws {
        let home = root.appendingPathComponent("tables-home", isDirectory: true)
        let store = try profile(home, "BraveSoftware/Brave-Browser", "Default")
        let old = record(current: "/d/old.part", target: "/d/old", received: 1, total: 10)
        let kept = record(current: "/d/kept.part", target: "/d/kept", received: 6, total: 10)
        let newer = record(current: "/d/newer.part", target: "/d/newer", received: 8, total: 10)
        try Data(table([
            [(Array("10_before".utf8), 1, Array("x".utf8))],
            [(key("a"), 5, old), (key("b"), 6, kept), (key("c"), 10, newer)],
            [(Array("33_after".utf8), 2, [0xff, 0xff])],
        ])).write(to: store.appendingPathComponent("000010.ldb"))
        // The journal deletes "a" later than the table wrote it, and writes
        // "c" earlier than the table did.
        try Data(log([batch(9, [(key("a"), nil)]),
                      batch(3, [(key("c"), record(current: "/d/stale.part", target: "/d/stale",
                                                  received: 2, total: 10))])]))
            .write(to: store.appendingPathComponent("000011.log"))
        let downloads = NotchChromiumDownloads.Reader(home: home).read()
        suite.expect(Set(downloads.map(\.currentPath)) == ["/d/kept.part", "/d/newer.part"],
                     "tables and the journal are resolved by sequence number, whichever file holds the newer entry")
        suite.expect(LevelDB.range(after: [], upTo: Array("21_download,z".utf8), holdsPrefix: key(""))
                     && !LevelDB.range(after: [], upTo: Array("10_x".utf8), holdsPrefix: key(""))
                     && LevelDB.range(after: Array("21_download,a".utf8), upTo: Array("33".utf8), holdsPrefix: key(""))
                     && LevelDB.range(after: Array("10".utf8), upTo: Array("33".utf8), holdsPrefix: key(""))
                     && !LevelDB.range(after: Array("21_downloae".utf8), upTo: Array("33".utf8), holdsPrefix: key("")),
                     "only blocks whose key range can reach the download prefix are read")
    }

    private static func damagedStores(_ suite: TestSuite, root: URL) throws {
        let home = root.appendingPathComponent("damaged-home", isDirectory: true)
        let locked = try profile(home, "Google/Chrome", "Default")
        let broken = try profile(home, "Microsoft Edge", "Default")
        let good = try profile(home, "Vivaldi", "Default")
        try Data(log([batch(1, [(key("a"), record(current: "/l/a.part", target: "/l/a", received: 1, total: 2))])]))
            .write(to: locked.appendingPathComponent("000003.log"))
        try FileManager.default.setAttributes([.posixPermissions: 0], ofItemAtPath: locked.appendingPathComponent("000003.log").path)
        var table = table([[(key("b"), 4, record(current: "/b/b.part", target: "/b/b", received: 1, total: 2))]])
        table[3] ^= 0xff
        try Data(table).write(to: broken.appendingPathComponent("000004.ldb"))
        try Data(log([batch(1, [(key("c"), record(current: "/g/c.part", target: "/g/c", received: 1, total: 2))])]))
            .write(to: good.appendingPathComponent("000003.log"))
        let downloads = NotchChromiumDownloads.Reader(home: home).read()
        suite.expect(downloads.map(\.currentPath) == ["/g/c.part"],
                     "an unreadable store and a damaged table give nothing, and other profiles are still read")
        try FileManager.default.setAttributes([.posixPermissions: 0], ofItemAtPath: good.path)
        suite.expect(NotchChromiumDownloads.Reader(home: home).read().isEmpty,
                     "a store folder that cannot be listed is skipped without failing")
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: good.path)
        let huge = try profile(home, "Chromium", "Default")
        let handle = try FileHandle(forWritingTo: { () -> URL in
            let url = huge.appendingPathComponent("000009.log")
            FileManager.default.createFile(atPath: url.path, contents: nil)
            return url
        }())
        try handle.truncate(atOffset: UInt64(NotchChromiumDownloads.maximumStoreBytes + 1))
        try handle.close()
        suite.expect(NotchChromiumDownloads.Reader(home: home).read().map(\.currentPath) == ["/g/c.part"],
                     "a store past the size bound is skipped whole")
    }

    // MARK: Partials

    private static func sparse(_ url: URL, length: Int64, written: [UInt8]) throws {
        FileManager.default.createFile(atPath: url.path, contents: nil)
        let handle = try FileHandle(forWritingTo: url)
        try handle.write(contentsOf: Data(written))
        try handle.truncate(atOffset: UInt64(length))
        try handle.close()
    }

    private static func partials(_ suite: TestSuite, root: URL) throws {
        let fm = FileManager.default
        let folder = root.appendingPathComponent("partials", isDirectory: true)
        let elsewhere = root.appendingPathComponent("partials-elsewhere", isDirectory: true)
        try fm.createDirectory(at: folder, withIntermediateDirectories: false)
        try fm.createDirectory(at: elsewhere, withIntermediateDirectories: false)
        // The browser names the folder through a symlink.
        let alias = root.appendingPathComponent("partials-alias")
        try fm.createSymbolicLink(at: alias, withDestinationURL: folder)

        let unconfirmed = folder.appendingPathComponent("Unconfirmed 123.crdownload")
        try Data(repeating: 1, count: 40).write(to: unconfirmed)
        let stretched = folder.appendingPathComponent("disk.img.crdownload")
        try sparse(stretched, length: 1 << 20, written: [1])
        let unknown = folder.appendingPathComponent("stream.bin.crdownload")
        try Data(repeating: 1, count: 30).write(to: unknown)
        let away = folder.appendingPathComponent("away.zip.crdownload")
        try Data(repeating: 1, count: 10).write(to: away)
        let unlisted = folder.appendingPathComponent("other.zip.crdownload")
        try Data(repeating: 1, count: 10).write(to: unlisted)

        let downloads = [
            NotchBrowserDownload(currentPath: alias.appendingPathComponent(unconfirmed.lastPathComponent).path,
                                 targetPath: alias.appendingPathComponent("report.zip").path,
                                 receivedBytes: 20, totalBytes: 100, paused: false),
            // A duplicate record for the same file does not make a second row.
            NotchBrowserDownload(currentPath: unconfirmed.path, targetPath: folder.appendingPathComponent("dupe.zip").path,
                                 receivedBytes: 99, totalBytes: 100, paused: false),
            NotchBrowserDownload(currentPath: stretched.path, targetPath: folder.appendingPathComponent("disk.img").path,
                                 receivedBytes: 32, totalBytes: 1 << 20, paused: false),
            NotchBrowserDownload(currentPath: unknown.path, targetPath: folder.appendingPathComponent("stream.bin").path,
                                 receivedBytes: 25, totalBytes: nil, paused: true),
            NotchBrowserDownload(currentPath: away.path, targetPath: elsewhere.appendingPathComponent("away.zip").path,
                                 receivedBytes: 5, totalBytes: 10, paused: false),
        ]
        let scanned = NotchDownloadSupport.scanFolder(folder)?.partials ?? [:]
        let annotated = NotchChromiumDownloads.annotate(scanned, previous: [:], downloads: downloads)
        func partial(_ url: URL) -> NotchPartialDownload? { annotated[url.standardizedFileURL] }
        suite.expect(annotated.count == scanned.count, "every partial stays listed exactly once")

        let first = partial(unconfirmed)
        suite.expect(first?.expectedURL.lastPathComponent == "report.zip"
                     && first?.expectedURL.deletingLastPathComponent().standardizedFileURL == folder.standardizedFileURL,
                     "an Unconfirmed partial takes its real name from the record, matched through a symlinked path")
        let counted = first.map(NotchChromiumDownloads.progress(of:))
        suite.expect(counted?.bytes == 40 && counted.flatMap(\.fraction).map { abs($0 - 0.4) < 0.0001 } == true,
                     "a file written in order moves the count on past the browser's last save")

        let preallocated = partial(stretched).map(NotchChromiumDownloads.progress(of:))
        suite.expect(preallocated?.bytes == 32
                     && preallocated.flatMap(\.fraction).map { abs($0 - 32.0 / Double(1 << 20)) < 0.000001 } == true,
                     "a file stretched to its full length counts by the browser's received bytes, not its length")

        let streaming = partial(unknown)
        let open = streaming.map(NotchChromiumDownloads.progress(of:))
        suite.expect(open?.bytes == 25 && open?.fraction == nil && streaming?.browser?.download.paused == true,
                     "with no known total the row stays indeterminate and keeps its byte count")

        suite.expect(partial(away)?.expectedURL == away.standardizedFileURL.deletingPathExtension()
                     && partial(away).map(NotchChromiumDownloads.progress(of:))?.fraction == 0.5,
                     "a target in another folder keeps the partial's own name but still counts")
        let plain = partial(unlisted)
        suite.expect(plain?.browser == nil && plain.map(NotchChromiumDownloads.progress(of:))?.fraction == nil
                     && plain.map(NotchChromiumDownloads.progress(of:))?.bytes == 10,
                     "a partial with no record keeps the file-only fallback")

        // The next read misses the record, for example mid-write.
        let carried = NotchChromiumDownloads.annotate(scanned, previous: annotated, downloads: [])
        suite.expect(carried[unconfirmed.standardizedFileURL]?.expectedURL.lastPathComponent == "report.zip",
                     "a partial keeps its record while it is the same file")
        try fm.removeItem(at: unconfirmed)
        try Data([2]).write(to: unconfirmed)
        let replaced = NotchDownloadSupport.scanFolder(folder)?.partials ?? [:]
        suite.expect(NotchChromiumDownloads.annotate(replaced, previous: annotated, downloads: [])[
            unconfirmed.standardizedFileURL]?.browser == nil,
                     "a new file at the same path does not inherit the old record")
        // Chromium's own NSProgress names the partial's path.
        let native = NotchDownloadItem(id: unconfirmed.path, url: unconfirmed, name: "Unconfirmed 123",
                                       receivedBytes: 30, fraction: 0.3, completed: false)
        let renamed = NotchChromiumDownloads.merged(native, with: first)
        suite.expect(renamed.name == "report.zip" && renamed.fraction == 0.3 && renamed.receivedBytes == 30
                     && renamed.id == native.id && renamed.url == native.url,
                     "a native row for a recorded partial keeps its own counts and takes the real name")
        let vague = NotchDownloadItem(id: unconfirmed.path, url: unconfirmed, name: "Unconfirmed 123",
                                      receivedBytes: nil, fraction: nil, completed: false)
        suite.expect(NotchChromiumDownloads.merged(vague, with: first).fraction.map { abs($0 - 0.4) < 0.0001 } == true,
                     "a native row with no share takes the record's known total")
        suite.expect(NotchChromiumDownloads.merged(native, with: plain) == native
                     && NotchChromiumDownloads.merged(native, with: nil) == native,
                     "a native row with no record is left as published")
        suite.expect(NotchChromiumDownloads.progress(of: NotchPartialDownload(
            url: unknown, expectedURL: unknown, bytes: 9, resourceID: nil, modified: Date())) == (9, nil),
                     "without a record the file length and no share are shown, as before")
    }

    // MARK: Scan

    private static func wait(_ condition: () -> Bool) -> Bool {
        let deadline = Date().addingTimeInterval(3)
        while !condition(), Date() < deadline {
            _ = RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.01))
        }
        return condition()
    }

    /// The production scan reads the store while a partial exists, names an
    /// Unconfirmed partial by its target and finishes it once on the rename,
    /// even when the browser's announcement of the same file arrives too.
    private static func scanCompletion(_ suite: TestSuite, root: URL) throws {
        let fm = FileManager.default
        let home = root.appendingPathComponent("scan-home", isDirectory: true)
        let folder = root.appendingPathComponent("scan-folder", isDirectory: true)
        try fm.createDirectory(at: folder, withIntermediateDirectories: false)
        let store = try profile(home, "Google/Chrome", "Default")
        let unconfirmed = folder.appendingPathComponent("Unconfirmed 4242.crdownload")
        try Data(repeating: 1, count: 10).write(to: unconfirmed)
        let target = folder.appendingPathComponent("movie.mp4")
        try Data(log([batch(1, [(key("m"), record(current: unconfirmed.path, target: target.path,
                                                  received: 10, total: 40))])]))
            .write(to: store.appendingPathComponent("000003.log"))

        let service = NotchDownloadScanTests.Service()
        service.folder = folder
        service.browserDownloads = NotchChromiumDownloads.Reader(home: home)
        var arrivals: [String] = []
        var failures = 0
        service.onArrival = { arrivals.append($0.name) }
        service.onFailure = { failures += 1 }
        defer { service.stop(); service.queue.sync {} }
        service.scan()
        suite.expect(wait { !service.scanning && !service.partials.isEmpty },
                     "the scan tracks the browser's partial")
        let tracked = service.partials[unconfirmed.standardizedFileURL]
        suite.expect(tracked?.expectedURL.lastPathComponent == "movie.mp4"
                     && tracked.map(NotchChromiumDownloads.progress(of:))?.fraction == 0.25,
                     "the scan merges the browser's name and known total into the partial")

        try fm.moveItem(at: unconfirmed, to: target)
        service.scan()
        suite.expect(wait { arrivals == ["movie.mp4"] && !service.scanning },
                     "the rename of an Unconfirmed partial to its target is a completed download")
        if let item = NotchDownloadSupport.announcedItem(atPath: target.path, folder: folder) {
            service.recordCompletion(item)
        }
        let deadline = Date().addingTimeInterval(2.2)
        _ = wait { Date() >= deadline }
        suite.expect(arrivals == ["movie.mp4"] && service.finished.count == 1,
                     "the browser's own announcement of the same file does not notify twice")
        suite.expect(failures == 0, "a renamed Unconfirmed partial is never called a failure")
    }

    /// With no partial in any folder the browser store is not read at all,
    /// and a stopped service does not scan.
    private static func scanWithoutPartials(_ suite: TestSuite, root: URL) throws {
        let folder = root.appendingPathComponent("scan-empty", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: false)
        try Data([1]).write(to: folder.appendingPathComponent("done.zip"))
        var reads = 0
        let service = NotchDownloadScanTests.Service()
        service.folder = folder
        service.browserDownloads = NotchChromiumDownloads.Reader(home: root, now: { reads += 1; return Date() })
        defer { service.stop(); service.queue.sync {} }
        service.scan()
        suite.expect(wait { !service.scanning && !service.folderItems.isEmpty } && reads == 0,
                     "a scan with no partial leaves the browser's store alone")
        try Data([1]).write(to: folder.appendingPathComponent("late.zip.crdownload"))
        service.stop()
        service.queue.sync {}
        service.scan()
        service.queue.sync {}
        suite.expect(reads == 0 && service.partials.isEmpty,
                     "once monitoring stops no folder is scanned and no store is read")
    }
}
