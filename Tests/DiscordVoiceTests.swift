// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Discord's mute and deafen on the island: the controls' labels, the saved
/// store read without writing, the shortcuts followed while Discord is
/// hidden, and when the island speaks up. A port of the Windows tests.
enum DiscordVoiceTests {
    static func run(_ suite: TestSuite) {
        controlContracts(suite)
        valueContracts(suite)
        logContracts(suite)
        tableContracts(suite)
        snappyContracts(suite)
        cachedReaderContracts(suite)
        keybindContracts(suite)
        trackerContracts(suite)
        modifierContracts(suite)
        readingContracts(suite)
        announcementContracts(suite)
        gateContracts(suite)
        localProfileDiagnostic(suite)
    }

    // MARK: Controls

    private static func controlContracts(_ suite: TestSuite) {
        func same(_ actual: (headphones: Bool, muted: Bool)?, _ expected: (Bool, Bool)?) -> Bool {
            switch (actual, expected) {
            case (nil, nil): return true
            case let (actual?, expected?): return actual.headphones == expected.0 && actual.muted == expected.1
            default: return false
            }
        }
        typealias Control = DiscordVoiceSupport
        suite.expect(same(Control.control("Mute", checked: nil), (false, false))
                     && same(Control.control("Unmute (Ctrl+Shift+M)", checked: nil), (false, true))
                     && same(Control.control("Wyłącz wyciszenie", checked: nil), (false, true))
                     && same(Control.control("Wyłącz wyciszenie dźwięku", checked: nil), (true, true)),
                     "a label names what the control does, so Unmute means the microphone is muted now")
        suite.expect(same(Control.control("Wyłącz dźwięk", checked: false), (true, false))
                     && same(Control.control("Deafen", checked: true), (true, true))
                     && same(Control.control("Mute", checked: true), (false, true))
                     && same(Control.control("Unmute", checked: false), (false, false)),
                     "a switch's checked state says how it is now, whatever its label")
        suite.expect(same(Control.control("Mute channel", checked: nil), nil)
                     && same(Control.control("", checked: nil), nil)
                     && same(Control.control("(Mute", checked: nil), nil),
                     "an unknown label is never read as an open microphone")
    }

    // MARK: Store

    private static let key = DiscordStorage.mediaKeys[0]
    private static let keybindKey = DiscordStorage.keybindKeys[0]

    private static func varint(_ number: Int, _ out: inout [UInt8]) {
        var number = number
        while number >= 128 { out.append(UInt8(number & 127) | 128); number >>= 7 }
        out.append(UInt8(number))
    }

    private static func le32(_ value: UInt32) -> [UInt8] { (0..<4).map { UInt8(truncatingIfNeeded: value >> ($0 * 8)) } }
    private static func le64(_ value: UInt64) -> [UInt8] { (0..<8).map { UInt8(truncatingIfNeeded: value >> ($0 * 8)) } }

    private static func value(_ muted: Bool, _ deafened: Bool) -> [UInt8] {
        [1] + Array(#"{"default":{"mute":\#(muted),"deaf":\#(deafened)},"stream":{"mute":true,"deaf":true}}"#.utf8)
    }

    private static func writeBatch(_ sequence: UInt64, _ operations: [([UInt8], [UInt8]?)]) -> [UInt8] {
        var out = le64(sequence) + le32(UInt32(operations.count))
        for (key, value) in operations {
            out.append(value == nil ? 0 : 1)
            varint(key.count, &out)
            out += key
            if let value { varint(value.count, &out); out += value }
        }
        return out
    }

    private static func appendLog(_ out: inout [UInt8], _ batch: [UInt8]) {
        var rest = batch[...]
        var first = true
        while !rest.isEmpty {
            var left = DiscordStorage.logBlock - out.count % DiscordStorage.logBlock
            if left < 7 { out += [UInt8](repeating: 0, count: left); left = DiscordStorage.logBlock }
            let length = min(rest.count, left - 7)
            let last = length == rest.count
            let kind: UInt8 = first && last ? 1 : first ? 2 : last ? 4 : 3
            let payload = rest.prefix(length)
            out += le32(DiscordStorage.checksum(kind: kind, payload, kindFirst: true))
            out += [UInt8(length & 255), UInt8(length >> 8), kind]
            out += payload
            rest = rest.dropFirst(length)
            first = false
        }
    }

    private static func log(_ sequence: UInt64, _ muted: Bool, _ deafened: Bool) -> [UInt8] {
        var out: [UInt8] = []
        appendLog(&out, writeBatch(sequence, [(key, value(muted, deafened))]))
        return out
    }

    private static func literal(_ bytes: [UInt8]) -> [UInt8] {
        var out: [UInt8] = []
        varint(bytes.count, &out)
        if bytes.count < 61 {
            out.append(UInt8((bytes.count - 1) << 2))
        } else {
            out.append(61 << 2)
            out += [UInt8((bytes.count - 1) & 255), UInt8((bytes.count - 1) >> 8)]
        }
        return out + bytes
    }

    private static func entry(_ key: [UInt8], _ value: [UInt8]) -> [UInt8] {
        var out: [UInt8] = [0]
        varint(key.count, &out)
        varint(value.count, &out)
        return out + key + value + le32(0) + le32(1)
    }

    private static func appendBlock(_ out: inout [UInt8], _ bytes: [UInt8], compressed: Bool) -> [UInt8] {
        var handle: [UInt8] = []
        varint(out.count, &handle)
        let payload = compressed ? literal(bytes) : bytes
        varint(payload.count, &handle)
        out += payload
        out.append(compressed ? 1 : 0)
        out += le32(DiscordStorage.checksum(kind: compressed ? 1 : 0, payload[...], kindFirst: false))
        return handle
    }

    private static func table(_ sequence: UInt64, _ muted: Bool, _ deafened: Bool, compressed: Bool) -> [UInt8] {
        var out: [UInt8] = []
        let internalKey = key + le64(sequence << 8 | 1)
        let data = appendBlock(&out, entry(internalKey, value(muted, deafened)), compressed: compressed)
        let index = appendBlock(&out, entry(internalKey, data), compressed: false)
        var footer: [UInt8] = [0, 0] + index
        footer += [UInt8](repeating: 0, count: 40 - footer.count)
        return out + footer + [0x57, 0xfb, 0x80, 0x8b, 0x24, 0x75, 0x47, 0xdb]
    }

    private static func readLog(_ bytes: [UInt8]) -> (UInt64, DiscordVoiceState?)? {
        DiscordStorage.readLog(bytes, decode: DiscordStorage.decodeVoice).map { ($0.sequence, $0.state) }
    }

    private static func readTable(_ bytes: [UInt8]) -> (UInt64, DiscordVoiceState?)?? {
        DiscordStorage.readTable(bytes, keys: DiscordStorage.mediaKeys, decode: DiscordStorage.decodeVoice)
            .map { $0.map { ($0.sequence, $0.state) } }
    }

    private static func valueContracts(_ suite: TestSuite) {
        let decode = DiscordStorage.decodeVoice
        suite.expect(decode(key[...], value(true, false)[...], 1) != nil
                     && decode(Array("_https://discord.com\u{0}\u{1}token".utf8)[...], value(true, false)[...], 1) == nil
                     && decode(Array("_https://example.com\u{0}\u{1}MediaEngineStore".utf8)[...], value(true, false)[...], 1) == nil
                     && decode(key.dropLast()[...], value(true, false)[...], 1) == nil,
                     "only Discord's own media store is decoded, never a token or another site")
        suite.expect(DiscordStorage.voiceState(value(false, false)[...]) == DiscordVoiceState(muted: false, deafened: false)
                     && DiscordStorage.voiceState(value(false, true)[...]) == DiscordVoiceState(muted: true, deafened: true)
                     && DiscordStorage.voiceState(Array("\u{1}{\"default\":{\"mute\":false}}".utf8)[...]) == nil,
                     "the default context decides, deafened mutes too, and a partial value is unknown")
        let json = Array(value(true, false).dropFirst())
        let utf16 = [UInt8(0)] + String(decoding: json, as: UTF8.self).utf16.flatMap { [UInt8($0 & 255), UInt8($0 >> 8)] }
        suite.expect(DiscordStorage.voiceState(utf16[...]) == DiscordStorage.voiceState(value(true, false)[...])
                     && DiscordStorage.voiceState([0, 0x00, 0xD8][...]) == nil,
                     "a UTF-16 value reads like a Latin-1 one, and a lone surrogate is unreadable")
    }

    private static func logContracts(_ suite: TestSuite) {
        var bytes = log(10, false, false)
        appendLog(&bytes, writeBatch(11, [(key, value(true, false))]))
        let expected = (UInt64(11), DiscordVoiceState(muted: true, deafened: false))
        func matches(_ read: (UInt64, DiscordVoiceState?)?) -> Bool { read?.0 == expected.0 && read?.1 == expected.1 }
        suite.expect(matches(readLog(bytes)), "the journal's latest batch wins")
        var tail = log(12, false, false)
        tail.removeLast()
        suite.expect(matches(readLog(bytes + tail)), "a batch still being written is ignored")
        var corrupt = log(13, false, false)
        corrupt[0] ^= 1
        suite.expect(matches(readLog(log(11, true, false) + corrupt)), "a corrupt record is ignored")

        var fragmented: [UInt8] = []
        appendLog(&fragmented, writeBatch(20, [(Array("unrelated".utf8), [UInt8](repeating: 120, count: 40000)),
                                               (key, value(false, true))]))
        let read = readLog(fragmented)
        suite.expect(read?.0 == 21 && read?.1 == DiscordVoiceState(muted: true, deafened: true),
                     "a batch split across blocks, behind a large unrelated value, still counts")
        var deletion: [UInt8] = []
        appendLog(&deletion, writeBatch(22, [(key, nil)]))
        let deleted = readLog(deletion)
        suite.expect(deleted?.0 == 22 && deleted?.1 == nil, "a deleted value leaves the state unknown")
        var padded: [UInt8] = []
        appendLog(&padded, writeBatch(30, [(key, value(true, true))]) + [0])
        var overcounted = writeBatch(31, [(key, value(true, true))])
        overcounted[8] = 2
        appendLog(&padded, overcounted)
        suite.expect(readLog(padded) == nil, "a batch with stray bytes or a wrong count is not trusted")
    }

    private static func tableContracts(_ suite: TestSuite) {
        for compressed in [false, true] {
            let bytes = table(8, true, false, compressed: compressed)
            let read = readTable(bytes)
            suite.expect(read??.0 == 8 && read??.1 == DiscordVoiceState(muted: true, deafened: false),
                         "a table reads \(compressed ? "compressed" : "plain")")
            var corrupt = bytes
            corrupt[0] ^= 1
            suite.expect(readTable(corrupt) == nil, "a block that fails its checksum makes the table unreadable")
        }
        let crc32c: UInt32 = 0xe306_9283
        suite.expect(DiscordStorage.checksum(kind: UInt8(ascii: "9"), Array("12345678".utf8)[...], kindFirst: false)
                     == ((crc32c >> 15) | (crc32c << 17)) &+ 0xa282_ead8,
                     "the checksum is LevelDB's masked CRC-32C")

        // Three data blocks: an unrelated store before the media key and one
        // after it, both failing their checksums. Only the middle block may be read.
        func internalKey(_ user: [UInt8], _ sequence: UInt64) -> [UInt8] { user + le64(sequence << 8 | 1) }
        func corrupted(_ bytes: inout [UInt8], _ handle: [UInt8]) {
            var cursor = DiscordStorage.Cursor(handle[...])
            if let offset = cursor.varint() { bytes[offset] ^= 1 }
        }
        let prefix = Array("_https://discord.com\u{0}\u{1}".utf8)
        let before = Array("_https://a.example\u{0}\u{1}x".utf8)
        let neighbour = prefix + Array("MediaEngineStoreX".utf8)
        let after = prefix + Array("Y".utf8)
        var bytes: [UInt8] = []
        let first = appendBlock(&bytes, entry(internalKey(before, 3), [1]), compressed: false)
        corrupted(&bytes, first)
        let middle = appendBlock(&bytes, entries([(internalKey(key, 9), value(true, false)),
                                                  (internalKey(key, 7), value(false, false)),
                                                  (internalKey(neighbour, 4), [1])]), compressed: true)
        let last = appendBlock(&bytes, entry(internalKey(after, 5), [1]), compressed: false)
        corrupted(&bytes, last)
        let index = appendBlock(&bytes, entries([(internalKey(before, 3), first), (internalKey(neighbour, 4), middle),
                                                 (internalKey(after, 5), last)]), compressed: false)
        var footer: [UInt8] = [0, 0] + index
        footer += [UInt8](repeating: 0, count: 40 - footer.count)
        let read = readTable(bytes + footer + [0x57, 0xfb, 0x80, 0x8b, 0x24, 0x75, 0x47, 0xdb])
        suite.expect(read??.0 == 9 && read??.1 == DiscordVoiceState(muted: true, deafened: false),
                     "the index skips blocks that cannot hold the key, and the newest version in a block wins")

        // One edition's key ends a block; another edition's newer key opens the next.
        let app = DiscordStorage.mediaKeys[1]
        var split: [UInt8] = []
        let stable = appendBlock(&split, entry(internalKey(key, 12), value(true, false)), compressed: false)
        let other = appendBlock(&split, entry(internalKey(app, 20), value(false, true)), compressed: false)
        let splitIndex = appendBlock(&split, entries([(internalKey(key, 12), stable), (internalKey(app, 20), other)]),
                                     compressed: false)
        var splitFooter: [UInt8] = [0, 0] + splitIndex
        splitFooter += [UInt8](repeating: 0, count: 40 - splitFooter.count)
        let editions = readTable(split + splitFooter + [0x57, 0xfb, 0x80, 0x8b, 0x24, 0x75, 0x47, 0xdb])
        suite.expect(editions??.0 == 20 && editions??.1 == DiscordVoiceState(muted: true, deafened: true),
                     "a block after one edition's key is still read for the next edition's")

        // The footer carries no checksum; a handle past every byte is refused, not trapped on.
        var hostile: [UInt8] = [0, 0]
        varint(Int.max, &hostile)
        varint(1, &hostile)
        hostile += [UInt8](repeating: 0, count: 40 - hostile.count) + [0x57, 0xfb, 0x80, 0x8b, 0x24, 0x75, 0x47, 0xdb]
        suite.expect(readTable(hostile) == nil, "a footer pointing past the file is refused")
        var huge: [UInt8] = [0]
        varint(9, &huge)
        varint(0, &huge)
        huge += [UInt8](repeating: 0, count: 9) + [1]
        varint(Int.max, &huge)
        varint(0, &huge)
        huge += le32(0) + le32(1)
        var hugeTable: [UInt8] = []
        let hugeIndex = appendBlock(&hugeTable, huge, compressed: false)
        var hugeFooter: [UInt8] = [0, 0] + hugeIndex
        hugeFooter += [UInt8](repeating: 0, count: 40 - hugeFooter.count)
        suite.expect(readTable(hugeTable + hugeFooter + [0x57, 0xfb, 0x80, 0x8b, 0x24, 0x75, 0x47, 0xdb]) == nil,
                     "an index entry claiming an endless key is refused")
    }

    /// A block of entries stored whole, each its own restart point.
    private static func entries(_ pairs: [([UInt8], [UInt8])]) -> [UInt8] {
        var out: [UInt8] = []
        var restarts: [UInt8] = []
        for (key, value) in pairs {
            restarts += le32(UInt32(out.count))
            out.append(0)
            varint(key.count, &out)
            varint(value.count, &out)
            out += key + value
        }
        return out + restarts + le32(UInt32(pairs.count))
    }

    private static func snappyContracts(_ suite: TestSuite) {
        for bytes: [UInt8] in [[6, 0, 97, 5, 1], [6, 0, 97, 18, 1, 0], [6, 0, 97, 19, 1, 0, 0, 0]] {
            suite.expect(DiscordStorage.snappy(bytes[...]) == Array("aaaaaa".utf8), "every copy encoding expands")
        }
        suite.expect(DiscordStorage.snappy([6, 0, 97, 5, 0][...]) == nil, "a copy from before the start is refused")
        // Any prefix of a valid file must be refused or read, never crash.
        for bytes in [table(5, false, true, compressed: true), log(5, true, false), literal(Array("test".utf8))] {
            for end in 0..<bytes.count {
                let prefix = Array(bytes[..<end])
                _ = readTable(prefix)
                _ = readLog(prefix)
                _ = DiscordStorage.snappy(prefix[...])
            }
        }
        suite.expect(true, "truncated tables, journals and blocks are rejected without a crash")
    }

    private static func cachedReaderContracts(_ suite: TestSuite) {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("vorssaint-discord-test-\(ProcessInfo.processInfo.processIdentifier)-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let base = directory.appendingPathComponent("000001.ldb")
        let journal = directory.appendingPathComponent("000002.log")
        func write(_ bytes: [UInt8], _ url: URL) { try? Data(bytes).write(to: url) }
        write(table(1, false, false, compressed: true), base)
        write(log(2, true, false), journal)
        write(Array("not a database".utf8), directory.appendingPathComponent("LOG"))
        let reader = DiscordStorage.voiceReader()
        suite.expect(reader.read(directory) == DiscordVoiceState(muted: true, deafened: false),
                     "the journal is newer than the table, and the human-readable LOG is skipped")
        var bytes = log(2, true, false)
        appendLog(&bytes, writeBatch(3, [(key, value(false, false))]))
        write(bytes, journal)
        suite.expect(reader.read(directory) == DiscordVoiceState(muted: false, deafened: false),
                     "an appended batch is read again")
        write(table(3, false, false, compressed: true), base)
        try? FileManager.default.removeItem(at: journal)
        suite.expect(reader.read(directory) == DiscordVoiceState(muted: false, deafened: false),
                     "compaction into a table keeps the state")

        let config = Array("\u{1}{\"_state\":{\"test\":{\"enabled\":true,\"action\":\"TOGGLE_MUTE\",\"shortcut\":[[0,13,2]]}}}".utf8)
        var keybinds: [UInt8] = []
        appendLog(&keybinds, writeBatch(4, [(keybindKey, config)]))
        write(keybinds, journal)
        let bindings = DiscordStorage.keybindReader()
        suite.expect(bindings.read(directory)?.first?.keys == [.key(0x26)], "the keybinds store gives the shortcut")
        let disabled = Array("\u{1}{\"_state\":{\"test\":{\"enabled\":false,\"action\":\"TOGGLE_MUTE\",\"shortcut\":[[0,13,2]]}}}".utf8)
        keybinds = []
        appendLog(&keybinds, writeBatch(5, [(keybindKey, disabled)]))
        write(keybinds, journal)
        suite.expect(bindings.read(directory)?.isEmpty == true, "a shortcut turned off in Discord is dropped")
        let before = (try? FileManager.default.contentsOfDirectory(atPath: directory.path))?.sorted()
        _ = reader.read(directory)
        suite.expect((try? FileManager.default.contentsOfDirectory(atPath: directory.path))?.sorted() == before,
                     "reading never writes to the store")
    }

    // MARK: Shortcuts

    private static func keybindContracts(_ suite: TestSuite) {
        let bindings = DiscordKeybinds.parse(["_state": [
            "one": ["enabled": true, "action": "TOGGLE_MUTE", "shortcut": [[0, 224, 2], [0, 13, 2]]],
            "two": ["enabled": true, "action": "TOGGLE_DEAFEN", "shortcut": [[1, 3, 2]]],
            "three": ["action": "PUSH_TO_MUTE", "shortcut": [[0, 58, 2]]],
            "disabled": ["enabled": false, "action": "TOGGLE_MUTE", "shortcut": [[0, 66, 2]]],
            "unrelated": ["action": "SAVE_CLIP", "shortcut": [[0, 65, 2]]],
            "unknownKey": ["action": "TOGGLE_MUTE", "shortcut": [[0, 250, 2]]],
        ] as [String: Any]])
        suite.expect(bindings.count == 3
                     && bindings.contains { $0.keys == [.key(0x3B), .key(0x26)] && $0.action == .toggleMute }
                     && bindings.contains { $0.keys == [.mouse(3)] && $0.action == .toggleDeafen }
                     && bindings.contains { $0.keys == [.key(0x7A)] && $0.action == .holdMute },
                     "the person's own chords and mouse buttons are read, by their HID usages, with no fixed hotkeys")
        suite.expect(DiscordKeybinds.virtualKey(usage: 4) == 0x00 && DiscordKeybinds.virtualKey(usage: 44) == 0x31
                     && DiscordKeybinds.virtualKey(usage: 231) == 0x36 && DiscordKeybinds.virtualKey(usage: 250) == nil,
                     "HID usages become the virtual keys NSEvent reports")
        suite.expect(DiscordKeybinds.parse(["nothing": true]).isEmpty && DiscordKeybinds.parse([1, 2]).isEmpty,
                     "a store without keybinds gives none")
        suite.expect(bindings.map(\.id) == bindings.map(\.id).sorted(),
                     "bindings come in a stable order, so a reread does not forget a held push-to-mute")
        var held = DiscordShortcutTracker()
        held.active = true
        held.seed(DiscordVoiceState(muted: false, deafened: false))
        held.setBindings(bindings)
        held.sample({ _ in false }, now: Date())
        held.sample({ $0 == .key(0x7A) }, now: Date())
        held.setBindings(DiscordKeybinds.parse(["_state": [
            "three": ["action": "PUSH_TO_MUTE", "shortcut": [[0, 58, 2]]],
            "two": ["enabled": true, "action": "TOGGLE_DEAFEN", "shortcut": [[1, 3, 2]]],
            "one": ["enabled": true, "action": "TOGGLE_MUTE", "shortcut": [[0, 224, 2], [0, 13, 2]]],
        ] as [String: Any]]))
        suite.expect(held.state?.muted == true, "the same shortcuts read again keep a push-to-mute held")
    }

    private static func tracker(_ keys: [DiscordKey], _ action: DiscordKeyAction) -> DiscordShortcutTracker {
        var tracker = DiscordShortcutTracker()
        tracker.active = true
        tracker.seed(DiscordVoiceState(muted: false, deafened: false))
        tracker.setBindings([DiscordKeybind(id: "custom", keys: keys, action: action)])
        tracker.sample({ _ in false }, now: Date())
        return tracker
    }

    private static func trackerContracts(_ suite: TestSuite) {
        let now = Date()
        let chord: Set<DiscordKey> = [.key(0x3B), .key(0x26)]
        var t = tracker([.key(0x3B), .key(0x26)], .toggleMute)
        t.sample({ chord.contains($0) }, now: now)
        suite.expect(t.state?.muted == true, "a chord mutes")
        t.live(DiscordVoiceState(muted: false, deafened: false), now: now.addingTimeInterval(0.02))
        suite.expect(t.state?.muted == true, "a control read before Discord answered the key does not undo it")
        t.sample({ chord.contains($0) }, now: now)
        suite.expect(t.state?.muted == true, "a held chord toggles once")
        t.sample({ _ in false }, now: now)
        t.sample({ chord.contains($0) }, now: now)
        suite.expect(t.state?.muted == false, "the next press unmutes")
        suite.expect(t.live(DiscordVoiceState(muted: true, deafened: false), now: now.addingTimeInterval(1))
                     && t.state?.muted == true, "once Discord has had time, its controls win")

        t = tracker([.key(0x65)], .toggleDeafen)
        t.sample({ $0 == .key(0x65) }, now: now)
        suite.expect(t.state == DiscordVoiceState(muted: true, deafened: true), "deafen mutes the microphone too")
        t.sample({ _ in false }, now: now)
        t.sample({ $0 == .key(0x65) }, now: now)
        suite.expect(t.state == DiscordVoiceState(muted: false, deafened: false),
                     "undeafen gives back the microphone as it was")

        t = tracker([.key(0x26)], .toggleMute)
        t.live(DiscordVoiceState(muted: true, deafened: true), now: now)
        t.sample({ $0 == .key(0x26) }, now: now)
        suite.expect(t.state == DiscordVoiceState(muted: false, deafened: false),
                     "unmuting while deafened undeafens too, as Discord does")

        t = tracker([.key(0x65)], .toggleDeafen)
        t.live(DiscordVoiceState(muted: true, deafened: false), now: now)
        t.live(DiscordVoiceState(muted: true, deafened: true), now: now)
        t.sample({ $0 == .key(0x65) }, now: now)
        suite.expect(t.state == DiscordVoiceState(muted: true, deafened: false),
                     "a deafen seen on the controls remembers the microphone was muted before it")

        t = tracker([.mouse(4)], .holdMute)
        t.sample({ $0 == .mouse(4) }, now: now)
        suite.expect(t.state?.muted == true, "push-to-mute mutes while held")
        suite.expect(!t.live(DiscordVoiceState(muted: true, deafened: false), now: now),
                     "a held push-to-mute never replaces the state underneath")
        t.sample({ _ in false }, now: now)
        suite.expect(t.state?.muted == false, "releasing it gives the microphone back")

        t = tracker([.key(0x26)], .toggleMute)
        t.sample({ [.key(0x3B), .key(0x26)].contains($0) }, now: now)
        suite.expect(t.state?.muted == false, "an extra modifier keeps an unrelated shortcut from muting")
        t.setBindings([DiscordKeybind(id: "new", keys: [.key(0x28)], action: .toggleMute)])
        t.sample({ $0 == .key(0x28) }, now: now)
        suite.expect(t.state?.muted == false, "a key already held when the shortcut arrives is no press")

        var idle = DiscordShortcutTracker()
        idle.seed(DiscordVoiceState(muted: false, deafened: false))
        idle.setBindings([DiscordKeybind(id: "x", keys: [.key(0x26)], action: .toggleMute)])
        idle.sample({ _ in false }, now: now)
        idle.sample({ $0 == .key(0x26) }, now: now)
        suite.expect(idle.state?.muted == false, "nothing is followed while Discord is not running")
        var unseeded = DiscordShortcutTracker()
        unseeded.active = true
        suite.expect(unseeded.state == nil, "with no reading and no saved state, Discord is unknown")
        unseeded.seed(DiscordVoiceState(muted: true, deafened: false))
        unseeded.seed(DiscordVoiceState(muted: false, deafened: false))
        suite.expect(unseeded.state?.muted == true, "the saved state only seeds the start")
    }

    private static func modifierContracts(_ suite: TestSuite) {
        let shift: UInt = 1 << 17
        let command: UInt = 1 << 20
        suite.expect(DiscordKeybinds.modifierKeys(rawFlags: shift | 0x02) == [0x38]
                     && DiscordKeybinds.modifierKeys(rawFlags: shift | 0x04) == [0x3C]
                     && DiscordKeybinds.modifierKeys(rawFlags: shift | 0x06) == [0x38, 0x3C],
                     "each side's device bit says which Shift is held")
        suite.expect(DiscordKeybinds.modifierKeys(rawFlags: shift) == [0x38]
                     && DiscordKeybinds.modifierKeys(rawFlags: command | 0x10) == [0x36]
                     && DiscordKeybinds.modifierKeys(rawFlags: 0x02).isEmpty
                     && DiscordKeybinds.modifierKeys(rawFlags: 0).isEmpty,
                     "a flag without its side counts as the left key, and a stray side bit as nothing")
    }

    // MARK: Island

    private static func readingContracts(_ suite: TestSuite) {
        let deafened = DiscordVoiceSupport.reading(VoiceChatSnapshot(discord: DiscordVoiceState(muted: false, deafened: true)))
        suite.expect(deafened == VoiceChatReading(muted: true, deafened: true) && deafened.active,
                     "deafened mutes the microphone even if Discord reports it open")
        suite.expect(DiscordVoiceSupport.reading(VoiceChatSnapshot(recording: true, systemMuted: true,
                                                                   discord: DiscordVoiceState(muted: false, deafened: false))).muted
                     && DiscordVoiceSupport.reading(VoiceChatSnapshot(recording: true,
                                                                      discord: DiscordVoiceState(muted: true, deafened: false))).muted,
                     "Discord's mute and the Mac's own mute each mute the microphone")
        suite.expect(!DiscordVoiceSupport.reading(.empty).active
                     && !DiscordVoiceSupport.reading(VoiceChatSnapshot(recording: true)).active,
                     "an open microphone has no activity, recording or not")
    }

    private static func announcementContracts(_ suite: TestSuite) {
        let open = VoiceChatSnapshot(discord: DiscordVoiceState(muted: false, deafened: false), settled: true)
        let muted = VoiceChatSnapshot(discord: DiscordVoiceState(muted: true, deafened: false), settled: true)
        let deafened = VoiceChatSnapshot(discord: DiscordVoiceState(muted: true, deafened: true), settled: true)
        func say(_ from: VoiceChatSnapshot, _ to: VoiceChatSnapshot, initial: Bool = false, enabled: Bool = true) -> VoiceChatReading? {
            DiscordVoiceSupport.announcement(from: from, to: to, initial: initial, enabled: enabled)
        }
        suite.expect(say(open, muted) == VoiceChatReading(muted: true, deafened: false)
                     && say(muted, open) == VoiceChatReading(muted: false, deafened: false)
                     && say(muted, deafened) == VoiceChatReading(muted: true, deafened: true),
                     "muting, unmuting and deafening are each announced")
        suite.expect(say(muted, muted, initial: true) == nil && say(.empty, muted, initial: true) == nil,
                     "the state found at start is not a change")
        suite.expect(say(muted, .empty) == nil, "Discord going away is unknown, not an unmute")
        var found = muted
        found.settled = false
        suite.expect(say(.empty, found) == nil && say(open, found) == nil,
                     "Discord as found when first read, or its controls replacing the saved state, is not a change")
        suite.expect(say(.empty, muted) == VoiceChatReading(muted: true, deafened: false),
                     "Discord starting muted says so")
        var systemMuted = open
        systemMuted.recording = true
        systemMuted.systemMuted = true
        suite.expect(say(open, systemMuted) == nil, "the Mac's own mute keeps its own notice")
        suite.expect(say(open, muted, enabled: false) == nil, "nothing is announced while the status is off")
        let text = FeatureStrings.notchVoice(.enUS)
        suite.expect(text.status(VoiceChatReading(muted: true, deafened: true)) == "Headphones and microphone muted"
                     && text.status(VoiceChatReading(muted: true, deafened: false)) == "Microphone muted"
                     && text.status(VoiceChatReading(muted: false, deafened: false)) == "Microphone unmuted",
                     "the notice says what changed, as on Windows")
        suite.expect(AppLanguage.allCases.allSatisfy { language in
            let strings = FeatureStrings.notchVoice(language)
            return [strings.title, strings.muted, strings.unmuted, strings.deafened, strings.hint].allSatisfy { !$0.isEmpty }
                && Set([strings.muted, strings.unmuted, strings.deafened]).count == 3
        }, "every language names each state differently")
        suite.expect(NotchEvent.voiceChat.duration == 3, "an unmute notice leaves after three seconds")
    }

    private static func gateContracts(_ suite: TestSuite) {
        let domain = "com.vorssaint.tests.notch-voice"
        let defaults = UserDefaults(suiteName: domain)!
        defaults.removePersistentDomain(forName: domain)
        defer { defaults.removePersistentDomain(forName: domain) }
        for (key, value) in Defaults.registeredDefaults where key.hasPrefix("notch") { defaults.set(value, forKey: key) }
        for (key, value) in AppFeature.availabilityDefaults { defaults.set(value, forKey: key) }
        defaults.set(true, forKey: DefaultsKey.notchEnabled)
        suite.expect(Defaults.registeredDefaults[DefaultsKey.notchVoiceChat] as? Bool == true
                     && DiscordVoiceSupport.isEnabled(in: defaults),
                     "Microphone status starts on, as on Windows")
        defaults.set(false, forKey: DefaultsKey.notchVoiceChat)
        suite.expect(!DiscordVoiceSupport.isEnabled(in: defaults), "its own switch turns it off")
        defaults.set(true, forKey: DefaultsKey.notchVoiceChat)
        defaults.set(false, forKey: DefaultsKey.notchEnabled)
        suite.expect(!DiscordVoiceSupport.isEnabled(in: defaults), "the island's master switch gates it too")
        suite.expect(SettingsBackupSupport.exportKeys().contains(DefaultsKey.notchVoiceChat),
                     "the switch travels in settings backup")
        suite.expect(NotchSupport.compactActivities(timer: true, watch: true, voiceChat: true, sessions: true, downloads: true,
                                                    agents: true, calendar: true, music: true, keepAwake: true)
                        .prefix(4) == [.timer, .watch, .voiceChat, .sessions],
                     "a muted microphone follows what the person waits on and comes before everything else")
        suite.expect(NotchCompactActivity.voiceChat.module == .controls
                     && NotchCompactActivity.voiceChat.symbol == DiscordVoiceSupport.mutedSymbol,
                     "the activity opens Controls, where the microphone tile is")
        suite.expect(DiscordVoiceSupport.editions["com.hnc.Discord"] == "discord"
                     && DiscordStorage.directory(profile: "discord", home: URL(fileURLWithPath: "/Users/x")).path
                        == "/Users/x/Library/Application Support/discord/Local Storage/leveldb",
                     "Discord's profile is found where it keeps it")
    }

    /// Reads the real profile on this Mac only when asked, so the suite
    /// never depends on Discord being installed.
    private static func localProfileDiagnostic(_ suite: TestSuite) {
        guard ProcessInfo.processInfo.environment["VORSSAINT_DISCORD_DIAGNOSTIC"] == "1" else { return }
        let directory = DiscordStorage.directory(profile: "discord")
        let voice = DiscordStorage.voiceReader().read(directory)
        let bindings = DiscordStorage.keybindReader().read(directory)
        print("Local Discord voice state: \(String(describing: voice)); mute/deafen bindings: \(bindings?.count ?? -1)")
        suite.expect(voice != nil, "the local profile's saved voice state can be read")
        suite.expect(bindings != nil, "the local profile's keybinds can be read")
    }
}
