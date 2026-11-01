// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Reads the files of a LevelDB store another app keeps open: its journal
/// and its sorted tables, never as a writer and without its lock. Discord's
/// local storage and Chromium's download store both go through here.
/// Formats: github.com/google/leveldb/doc/{log_format,table_format}.md and
/// github.com/google/snappy/blob/main/format_description.txt.
enum LevelDB {
    static let maxFile = 64 * 1024 * 1024
    static let maxBlock = 64 * 1024 * 1024
    static let logBlock = 32768

    /// One entry: its key, its value (empty when deleted), its sequence
    /// number and its kind, 1 for a value and 0 for a deletion.
    typealias Visit = (_ key: ArraySlice<UInt8>, _ value: ArraySlice<UInt8>, _ sequence: UInt64, _ kind: UInt8) -> Void

    /// The store's own numbered files. The human-readable LOG files are not.
    static func isDataFile(_ url: URL) -> Bool {
        guard ["log", "ldb", "sst"].contains(url.pathExtension) else { return false }
        let stem = url.deletingPathExtension().lastPathComponent
        return !stem.isEmpty && stem.allSatisfy(\.isASCII) && stem.allSatisfy(\.isNumber)
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
            guard let value = varint64() else { return nil }
            return Int(exactly: value)
        }

        mutating func varint64() -> UInt64? {
            var value: UInt64 = 0
            var shift: UInt64 = 0
            while shift < 64 {
                guard let byte = byte() else { return nil }
                if shift == 63, byte > 1 { return nil }
                value |= UInt64(byte & 127) << shift
                if byte & 128 == 0 { return value }
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

    /// A write batch counts whole or not at all: one that does not parse to
    /// its end visits nothing.
    private static func batch(_ bytes: ArraySlice<UInt8>, visit: Visit) {
        guard let sequence = u64(bytes), let count = u32(bytes.dropFirst(8)).map(Int.init),
              count <= bytes.count, bytes.count >= 12 else { return }
        var cursor = Cursor(bytes.dropFirst(12))
        var found: [(ArraySlice<UInt8>, ArraySlice<UInt8>, UInt64, UInt8)] = []
        for index in 0..<count {
            guard let kind = cursor.byte(), let key = cursor.string() else { return }
            let value: ArraySlice<UInt8>
            switch kind {
            case 0: value = []
            case 1:
                guard let string = cursor.string() else { return }
                value = string
            default: return
            }
            let (position, overflow) = sequence.addingReportingOverflow(UInt64(index))
            guard !overflow else { return }
            found.append((key, value, position, kind))
        }
        guard cursor.atEnd else { return }
        for entry in found { visit(entry.0, entry.1, entry.2, entry.3) }
    }

    /// A partial or corrupt tail, as a live writer leaves it, is ignored, so
    /// only committed batches count.
    static func visitLog(_ bytes: [UInt8], visit: Visit) {
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
                batch(payload, visit: visit)
            case 2:
                fragment = Array(payload)
            case 3, 4:
                guard var parts = fragment else { continue }
                guard parts.count + payload.count <= maxBlock else { fragment = nil; continue }
                parts.append(contentsOf: payload)
                if kind == 4 {
                    batch(parts[...], visit: visit)
                    fragment = nil
                } else {
                    fragment = parts
                }
            default:
                fragment = nil
            }
        }
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
    /// compressed: only a block `relevant` accepts is read. It gets the
    /// previous block's upper key (empty for the first) and the block's own,
    /// and the block holds the keys after the first, up to the second.
    /// False when the table is not valid.
    static func visitTable(_ bytes: [UInt8], relevant: (_ after: [UInt8], _ upTo: [UInt8]) -> Bool,
                           visit: Visit) -> Bool {
        let magic: [UInt8] = [0x57, 0xfb, 0x80, 0x8b, 0x24, 0x75, 0x47, 0xdb]
        guard bytes.count >= 48, Array(bytes.suffix(8)) == magic else { return false }
        var footer = Cursor(bytes[(bytes.count - 48)...])
        // The metaindex block's handle comes first.
        guard footer.varint() != nil, footer.varint() != nil, let index = block(bytes, handle: &footer) else { return false }
        var previousUpper: [UInt8] = []
        return entries(index) { separator, handle in
            guard separator.count >= 8 else { return false }
            let upper = Array(separator.dropLast(8))
            defer { previousUpper = upper }
            guard relevant(previousUpper, upper) else { return true }
            var cursor = Cursor(handle)
            guard let data = block(bytes, handle: &cursor) else { return false }
            return entries(data) { key, value in
                guard key.count >= 8, let tag = u64(key[(key.count - 8)...]) else { return false }
                visit(key[..<(key.count - 8)], value, tag >> 8, UInt8(tag & 255))
                return true
            }
        }
    }

    /// Whether a block holding the keys after `after`, up to `upTo`, can hold `key`.
    static func range(after: [UInt8], upTo: [UInt8], holds key: [UInt8]) -> Bool {
        !upTo.lexicographicallyPrecedes(key) && (after.isEmpty || !key.lexicographicallyPrecedes(after))
    }

    /// Whether such a block can hold a key that starts with `prefix`.
    static func range(after: [UInt8], upTo: [UInt8], holdsPrefix prefix: [UInt8]) -> Bool {
        // Keys with the prefix sort from the prefix itself up to, but not
        // including, the first key past every one of them.
        guard !upTo.lexicographicallyPrecedes(prefix) else { return false }
        guard !after.isEmpty else { return true }
        if after.starts(with: prefix) { return true }
        return after.lexicographicallyPrecedes(prefix)
    }
}
