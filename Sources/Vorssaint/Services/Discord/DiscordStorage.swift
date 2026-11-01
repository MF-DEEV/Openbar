// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Discord's mute and deafen switches, as its own controls show them.
struct DiscordVoiceState: Equatable {
    var muted: Bool
    var deafened: Bool
}

/// Reads Discord's saved keybinds and its saved voice state from the local
/// LevelDB store, a port of the Windows reader. The saved voice state is not
/// live: Discord can leave it unchanged after a click, so it only seeds the
/// state at startup and never overrides the controls. The store is only ever
/// read, never opened as a writer, and no other value is decoded.
/// The file formats are read by `LevelDB`.
enum DiscordStorage {
    static let maxFile = LevelDB.maxFile
    static let maxBlock = LevelDB.maxBlock
    static let logBlock = LevelDB.logBlock

    private static func keys(_ name: String) -> [[UInt8]] {
        ["discord.com", "discordapp.com", "canary.discord.com", "ptb.discord.com"]
            .map { Array("_https://\($0)\u{0}\u{1}\(name)".utf8) }
    }

    static let mediaKeys = keys("MediaEngineStore")
    static let keybindKeys = keys("keybinds")

    /// Each installed edition keeps its own profile beside the others.
    static func directory(profile: String, home: URL = FileManager.default.homeDirectoryForCurrentUser) -> URL {
        home.appendingPathComponent("Library/Application Support", isDirectory: true)
            .appendingPathComponent(profile, isDirectory: true)
            .appendingPathComponent("Local Storage/leveldb", isDirectory: true)
    }

    /// A matching key gives `.some`: the value it holds, or nil when it was
    /// deleted or cannot be read. Any other key gives nil and is skipped.
    typealias Decode<T> = (ArraySlice<UInt8>, ArraySlice<UInt8>, UInt8) -> T??

    /// Most entries belong to other stores, so a key is compared by its
    /// length before its bytes and nothing is copied until one matches.
    static func matches(_ key: ArraySlice<UInt8>, _ keys: [[UInt8]]) -> Bool {
        keys.contains { $0.count == key.count && $0.elementsEqual(key) }
    }

    struct Record<T> {
        let sequence: UInt64
        let state: T?
    }

    static func voiceReader() -> Reader<DiscordVoiceState> {
        Reader(keys: mediaKeys, decode: decodeVoice)
    }

    static func keybindReader() -> Reader<[DiscordKeybind]> {
        Reader(keys: keybindKeys) { key, value, kind in
            guard matches(key, keybindKeys) else { return nil }
            return .some(kind == 1 ? json(value).map(DiscordKeybinds.parse) : nil)
        }
    }

    /// Files are decoded again only when their size or date changes, so the
    /// store can be read often while Discord appends to its journal.
    final class Reader<T> {
        private let keys: [[UInt8]]
        private let decode: Decode<T>
        private var files: [URL: (fingerprint: Fingerprint, record: Record<T>?)] = [:]

        private struct Fingerprint: Equatable {
            let size: UInt64
            let modified: Date
        }

        init(keys: [[UInt8]], decode: @escaping Decode<T>) {
            self.keys = keys
            self.decode = decode
        }

        func read(_ directory: URL) -> T? {
            let manager = FileManager.default
            guard let entries = try? manager.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil) else {
                return nil
            }
            var latest: Record<T>?
            var seen = Set<URL>()
            for path in entries {
                let ext = path.pathExtension
                guard LevelDB.isDataFile(path) else { continue }
                guard let attributes = try? manager.attributesOfItem(atPath: path.path),
                      let size = (attributes[.size] as? NSNumber)?.uint64Value,
                      let modified = attributes[.modificationDate] as? Date else { return nil }
                if size > UInt64(maxFile) { return nil }
                let fingerprint = Fingerprint(size: size, modified: modified)
                if files[path]?.fingerprint != fingerprint {
                    guard let data = try? Data(contentsOf: path, options: .mappedIfSafe), data.count <= maxFile else {
                        return nil
                    }
                    let bytes = [UInt8](data)
                    let record: Record<T>?
                    if ext == "log" {
                        record = readLog(bytes, decode: decode)
                    } else {
                        guard let table = readTable(bytes, keys: keys, decode: decode) else { return nil }
                        record = table
                    }
                    files[path] = (fingerprint, record)
                }
                newer(&latest, files[path]?.record)
                seen.insert(path)
            }
            files = files.filter { seen.contains($0.key) }
            return latest?.state
        }
    }

    // MARK: Values

    static func voiceState(_ value: ArraySlice<UInt8>) -> DiscordVoiceState? {
        guard let settings = json(value) as? [String: Any], let current = settings["default"] as? [String: Any],
              let deafened = current["deaf"] as? Bool, let mute = current["mute"] as? Bool else { return nil }
        return DiscordVoiceState(muted: mute || deafened, deafened: deafened)
    }

    /// Chromium's local storage keeps a string as Latin-1 behind a 1, or as
    /// UTF-16 behind a 0.
    static func json(_ value: ArraySlice<UInt8>) -> Any? {
        guard let encoding = value.first else { return nil }
        let body = value.dropFirst()
        let text: String
        switch encoding {
        case 1:
            text = String(body.map { Character(Unicode.Scalar($0)) })
        case 0 where body.count % 2 == 0:
            let units = stride(from: body.startIndex, to: body.endIndex, by: 2).map {
                UInt16(body[$0]) | UInt16(body[$0 + 1]) << 8
            }
            // A lone surrogate makes the value unreadable rather than repaired.
            var scalars = String.UnicodeScalarView()
            var iterator = units.makeIterator()
            var decoder = UTF16()
            decoding: while true {
                switch decoder.decode(&iterator) {
                case .scalarValue(let scalar): scalars.append(scalar)
                case .emptyInput: break decoding
                case .error: return nil
                }
            }
            text = String(scalars)
        default:
            return nil
        }
        return try? JSONSerialization.jsonObject(with: Data(text.utf8))
    }

    static func decodeVoice(_ key: ArraySlice<UInt8>, _ value: ArraySlice<UInt8>, _ kind: UInt8) -> DiscordVoiceState?? {
        guard matches(key, mediaKeys) else { return nil }
        return .some(kind == 1 ? voiceState(value) : nil)
    }

    static func newer<T>(_ current: inout Record<T>?, _ candidate: Record<T>?) {
        guard let candidate else { return }
        if current.map({ candidate.sequence > $0.sequence }) ?? true { current = candidate }
    }

    private static func consider<T>(_ current: inout Record<T>?, key: ArraySlice<UInt8>, value: ArraySlice<UInt8>,
                                    sequence: UInt64,
                                    kind: UInt8, decode: Decode<T>) {
        if let state = decode(key, value, kind) { newer(&current, Record(sequence: sequence, state: state)) }
    }

    // MARK: Files

    typealias Cursor = LevelDB.Cursor

    static func u32(_ bytes: ArraySlice<UInt8>) -> UInt32? { LevelDB.u32(bytes) }

    static func checksum(kind: UInt8, _ bytes: ArraySlice<UInt8>, kindFirst: Bool) -> UInt32 {
        LevelDB.checksum(kind: kind, bytes, kindFirst: kindFirst)
    }

    static func snappy(_ bytes: ArraySlice<UInt8>) -> [UInt8]? { LevelDB.snappy(bytes) }

    /// A partial or corrupt tail, as Discord writes it, is ignored, so only
    /// committed batches count.
    static func readLog<T>(_ bytes: [UInt8], decode: Decode<T>) -> Record<T>? {
        var latest: Record<T>?
        LevelDB.visitLog(bytes) { key, value, sequence, kind in
            consider(&latest, key: key, value: value, sequence: sequence, kind: kind, decode: decode)
        }
        return latest
    }

    /// Only a block whose range can hold one of `keys` is read.
    /// Nil when the table is not valid; `.some(nil)` when it holds no match.
    static func readTable<T>(_ bytes: [UInt8], keys: [[UInt8]], decode: Decode<T>) -> Record<T>?? {
        var latest: Record<T>?
        let valid = LevelDB.visitTable(bytes, relevant: { after, upTo in
            keys.contains { LevelDB.range(after: after, upTo: upTo, holds: $0) }
        }, visit: { key, value, sequence, kind in
            consider(&latest, key: key, value: value, sequence: sequence, kind: kind, decode: decode)
        })
        return valid ? .some(latest) : nil
    }
}
