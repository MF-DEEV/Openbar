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
/// Formats: github.com/google/leveldb/doc/{log_format,table_format}.md and
/// github.com/google/snappy/blob/main/format_description.txt.
enum DiscordStorage {
    static let maxFile = 64 * 1024 * 1024
    static let maxBlock = 64 * 1024 * 1024
    static let logBlock = 32768

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
                guard ["log", "ldb", "sst"].contains(ext) else { continue }
                // Database files are numeric; the human-readable LOG files are not.
                let stem = path.deletingPathExtension().lastPathComponent
                guard !stem.isEmpty, stem.allSatisfy(\.isASCII), stem.allSatisfy(\.isNumber) else { continue }
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

    // MARK: Encoding

    struct Cursor {
        let bytes: ArraySlice<UInt8>
        var position: Int

        init(_ bytes: ArraySlice<UInt8>) {
            self.bytes = bytes
            position = bytes.startIndex
        }

        var atEnd: Bool { position >= bytes.endIndex }

        mutating func take(_ count: Int) -> ArraySlice<UInt8>? {
            guard count >= 0, count <= bytes.endIndex - position else { return nil }
            defer { position += count }
            return bytes[position..<position + count]
        }

        mutating func byte() -> UInt8? {
            guard position < bytes.endIndex else { return nil }
            defer { position += 1 }
            return bytes[position]
        }

        mutating func varint() -> Int? {
            var value: UInt64 = 0
            var shift: UInt64 = 0
            while shift < 64 {
                guard let byte = byte() else { return nil }
                if shift == 63, byte > 1 { return nil }
                value |= UInt64(byte & 127) << shift
                if byte & 128 == 0 { return Int(exactly: value) }
                shift += 7
            }
            return nil
        }

        mutating func string() -> ArraySlice<UInt8>? {
            guard let length = varint() else { return nil }
            return take(length)
        }
    }

    static func u32(_ bytes: ArraySlice<UInt8>) -> UInt32? {
        guard bytes.count >= 4 else { return nil }
        let start = bytes.startIndex
        return UInt32(bytes[start]) | UInt32(bytes[start + 1]) << 8 | UInt32(bytes[start + 2]) << 16
            | UInt32(bytes[start + 3]) << 24
    }

    static func u64(_ bytes: ArraySlice<UInt8>) -> UInt64? {
        guard bytes.count >= 8, let low = u32(bytes), let high = u32(bytes.dropFirst(4)) else { return nil }
        return UInt64(low) | UInt64(high) << 32
    }

    /// The CRC-32C remainder of every byte, so a block costs one lookup per
    /// byte. The byte loops here stay plain index loops over raw buffers:
    /// the Developer build is unoptimized, where generic iteration over a
    /// megabyte block took seconds.
    private static let crcTable: [UInt32] = (0..<256).map { index in
        var crc = UInt32(index)
        for _ in 0..<8 { crc = (crc >> 1) ^ (0x82f6_3b78 & (0 &- (crc & 1))) }
        return crc
    }

    /// LevelDB's masked CRC-32C. A log record checks its type before its
    /// payload, a table block its payload before its type.
    static func checksum(kind: UInt8, _ bytes: ArraySlice<UInt8>, kindFirst: Bool) -> UInt32 {
        crcTable.withUnsafeBufferPointer { table in
            var crc = ~UInt32(0)
            if kindFirst { crc = table[Int((crc ^ UInt32(kind)) & 255)] ^ (crc >> 8) }
            bytes.withUnsafeBufferPointer { buffer in
                var index = 0
                let count = buffer.count
                while index < count {
                    crc = table[Int((crc ^ UInt32(buffer[index])) & 255)] ^ (crc >> 8)
                    index += 1
                }
            }
            if !kindFirst { crc = table[Int((crc ^ UInt32(kind)) & 255)] ^ (crc >> 8) }
            let value = ~crc
            return ((value >> 15) | (value << 17)) &+ 0xa282_ead8
        }
    }

    // MARK: Log

    private static func batch<T>(_ bytes: ArraySlice<UInt8>, decode: Decode<T>) -> Record<T>? {
        guard let sequence = u64(bytes), let count = u32(bytes.dropFirst(8)).map(Int.init),
              count <= bytes.count, bytes.count >= 12 else { return nil }
        var cursor = Cursor(bytes.dropFirst(12))
        var latest: Record<T>?
        for index in 0..<count {
            guard let kind = cursor.byte(), let key = cursor.string() else { return nil }
            let value: ArraySlice<UInt8>
            switch kind {
            case 0: value = []
            case 1:
                guard let string = cursor.string() else { return nil }
                value = string
            default: return nil
            }
            let (position, overflow) = sequence.addingReportingOverflow(UInt64(index))
            guard !overflow else { return nil }
            consider(&latest, key: key, value: value, sequence: position, kind: kind, decode: decode)
        }
        return cursor.atEnd ? latest : nil
    }

    /// A partial or corrupt tail, as Discord writes it, is ignored, so only
    /// committed batches count.
    static func readLog<T>(_ bytes: [UInt8], decode: Decode<T>) -> Record<T>? {
        var latest: Record<T>?
        var fragment: [UInt8]?
        var offset = 0
        while offset + 7 <= bytes.count {
            let remaining = logBlock - offset % logBlock
            if remaining < 7 { offset += remaining; continue }
            let header = bytes[offset..<offset + 7]
            let length = Int(header[offset + 4]) | Int(header[offset + 5]) << 8
            let kind = header[offset + 6]
            if kind == 0, length == 0 { offset += remaining; fragment = nil; continue }
            if length + 7 > remaining { break }
            offset += 7
            guard offset + length <= bytes.count else { break }
            let payload = bytes[offset..<offset + length]
            offset += length
            guard checksum(kind: kind, payload, kindFirst: true) == u32(header) else { fragment = nil; continue }
            switch kind {
            case 1:
                fragment = nil
                newer(&latest, batch(payload, decode: decode))
            case 2:
                fragment = Array(payload)
            case 3, 4:
                guard var parts = fragment else { continue }
                guard parts.count + payload.count <= maxBlock else { fragment = nil; continue }
                parts.append(contentsOf: payload)
                if kind == 4 {
                    newer(&latest, batch(parts[...], decode: decode))
                    fragment = nil
                } else {
                    fragment = parts
                }
            default:
                fragment = nil
            }
        }
        return latest
    }

    // MARK: Table

    static func snappy(_ bytes: ArraySlice<UInt8>) -> [UInt8]? {
        var header = Cursor(bytes)
        guard let size = header.varint(), size <= maxBlock else { return nil }
        let start = header.position - bytes.startIndex
        var output = [UInt8](repeating: 0, count: size)
        var written = 0
        let valid = bytes.withUnsafeBufferPointer { input -> Bool in
            output.withUnsafeMutableBufferPointer { out -> Bool in
                var position = start
                let end = input.count
                while position < end {
                    let tag = input[position]
                    position += 1
                    var length: Int
                    if tag & 3 == 0 {
                        let code = Int(tag >> 2)
                        length = code + 1
                        if code >= 60 {
                            let extra = code - 59
                            guard position + extra <= end else { return false }
                            length = 0
                            var shift = 0
                            for offset in 0..<extra {
                                length |= Int(input[position + offset]) << shift
                                shift += 8
                            }
                            length += 1
                            position += extra
                        }
                        guard length <= end - position, length <= size - written else { return false }
                        var index = 0
                        while index < length {
                            out[written + index] = input[position + index]
                            index += 1
                        }
                        position += length
                        written += length
                    } else {
                        let distance: Int
                        switch tag & 3 {
                        case 1:
                            guard position < end else { return false }
                            length = 4 + Int((tag >> 2) & 7)
                            distance = (Int(tag) & 224) << 3 | Int(input[position])
                            position += 1
                        case 2:
                            guard position + 2 <= end else { return false }
                            length = 1 + Int(tag >> 2)
                            distance = Int(input[position]) | Int(input[position + 1]) << 8
                            position += 2
                        default:
                            guard position + 4 <= end else { return false }
                            length = 1 + Int(tag >> 2)
                            distance = Int(input[position]) | Int(input[position + 1]) << 8
                                | Int(input[position + 2]) << 16 | Int(input[position + 3]) << 24
                            position += 4
                        }
                        guard distance > 0, distance <= written, length <= size - written else { return false }
                        // A copy may overlap what it writes, so it goes byte by byte.
                        var index = 0
                        while index < length {
                            out[written + index] = out[written + index - distance]
                            index += 1
                        }
                        written += length
                    }
                }
                return true
            }
        }
        return valid && written == size ? output : nil
    }

    private static func block(_ bytes: [UInt8], handle: inout Cursor) -> [UInt8]? {
        guard let offset = handle.varint(), let length = handle.varint(), length <= maxBlock else { return nil }
        // The footer has no checksum, so a handle can point anywhere.
        let (end, overflow) = offset.addingReportingOverflow(length)
        guard !overflow, bytes.count >= 5, end <= bytes.count - 5 else { return nil }
        let payload = bytes[offset..<end]
        let kind = bytes[end]
        guard checksum(kind: kind, payload, kindFirst: false) == u32(bytes[(end + 1)...]) else { return nil }
        switch kind {
        case 0: return Array(payload)
        case 1: return snappy(payload)
        default: return nil
        }
    }

    private static func entries(_ bytes: [UInt8], visit: ([UInt8], ArraySlice<UInt8>) -> Bool) -> Bool {
        guard bytes.count >= 4, let restarts = u32(bytes[(bytes.count - 4)...]).map(Int.init) else { return false }
        let (trailer, overflow) = restarts.multipliedReportingOverflow(by: 4)
        guard !overflow, trailer + 4 <= bytes.count else { return false }
        let end = bytes.count - 4 - trailer
        var cursor = Cursor(bytes[..<end])
        var key: [UInt8] = []
        while !cursor.atEnd {
            guard let shared = cursor.varint(), let unique = cursor.varint(), let length = cursor.varint(),
                  shared <= key.count, unique <= maxBlock - shared else { return false }
            key.removeLast(key.count - shared)
            guard let suffix = cursor.take(unique) else { return false }
            key.append(contentsOf: suffix)
            guard let value = cursor.take(length), visit(key, value) else { return false }
        }
        return true
    }

    /// The sorted index lets unrelated stores, even very large ones, stay
    /// compressed: only a block whose range can hold one of `keys` is read.
    /// Nil when the table is not valid; `.some(nil)` when it holds no match.
    static func readTable<T>(_ bytes: [UInt8], keys: [[UInt8]], decode: Decode<T>) -> Record<T>?? {
        let magic: [UInt8] = [0x57, 0xfb, 0x80, 0x8b, 0x24, 0x75, 0x47, 0xdb]
        guard bytes.count >= 48, Array(bytes.suffix(8)) == magic else { return nil }
        var footer = Cursor(bytes[(bytes.count - 48)...])
        // The metaindex block's handle comes first.
        guard footer.varint() != nil, footer.varint() != nil, let index = block(bytes, handle: &footer) else { return nil }
        var latest: Record<T>?
        var previousUpper: [UInt8] = []
        let valid = entries(index) { separator, handle in
            guard separator.count >= 8 else { return false }
            let upper = Array(separator.dropLast(8))
            // A block holds the keys after the previous separator, up to its own.
            let relevant = keys.contains { target in
                !upper.lexicographicallyPrecedes(target)
                    && (previousUpper.isEmpty || !target.lexicographicallyPrecedes(previousUpper))
            }
            previousUpper = upper
            guard relevant else { return true }
            var cursor = Cursor(handle)
            guard let data = block(bytes, handle: &cursor) else { return false }
            return entries(data) { key, value in
                guard key.count >= 8, let tag = u64(key[(key.count - 8)...]) else { return false }
                consider(&latest, key: key[..<(key.count - 8)], value: value, sequence: tag >> 8,
                         kind: UInt8(tag & 255), decode: decode)
                return true
            }
        }
        return valid ? .some(latest) : nil
    }
}
