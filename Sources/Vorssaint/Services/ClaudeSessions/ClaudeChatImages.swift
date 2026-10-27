// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import ImageIO
import UniformTypeIdentifiers

/// An image a prompt carried, kept small. One that came inline, as Claude
/// Code writes a pasted screenshot into its transcript, is shrunk to a
/// preview when read and its bytes let go; one that lies in a file, as a
/// Codex prompt names it, is read only when shown. Nothing is ever fetched
/// from the network.
struct ClaudeChatImage: Equatable {
    enum Source: Equatable {
        /// A small JPEG made when the prompt was read.
        case preview(Data)
        /// An absolute path on this Mac.
        case file(String)
    }

    /// Tells images apart without comparing their bytes.
    let key: Int
    let source: Source

    static func == (lhs: ClaudeChatImage, rhs: ClaudeChatImage) -> Bool { lhs.key == rhs.key }

    /// Images one prompt shows at most.
    static let perPrompt = 16
    /// Longer inline data is left out rather than decoded.
    static let largestInline = 28 << 20
    /// Larger files are left out; the agents refuse them anyway.
    static let largestFile = 20 << 20
    /// The longest side of a preview, in pixels.
    static let previewSide = 640
    /// Inline previews a conversation keeps; older prompts let theirs go.
    static let kept = 24

    var isInline: Bool { if case .preview = source { true } else { false } }

    // MARK: Reading

    /// An image Claude Code wrote inline as base64, shrunk to a preview.
    static func inline(base64 data: String, mediaType: String) -> ClaudeChatImage? {
        guard mediaType.hasPrefix("image/"), data.utf8.count <= largestInline,
              let bytes = Data(base64Encoded: data, options: .ignoreUnknownCharacters),
              let preview = preview(of: bytes) else { return nil }
        return ClaudeChatImage(key: key("inline", data), source: .preview(preview))
    }

    /// A `data:image/…;base64,` address shrunk to a preview; any other
    /// address, a web one included, is never read.
    static func inline(dataURL: String) -> ClaudeChatImage? {
        guard dataURL.hasPrefix("data:image/"), let comma = dataURL.firstIndex(of: ",") else { return nil }
        let header = dataURL[dataURL.index(dataURL.startIndex, offsetBy: 5)..<comma]
        guard header.hasSuffix(";base64") else { return nil }
        return inline(base64: String(dataURL[dataURL.index(after: comma)...]),
                      mediaType: String(header.dropLast(";base64".count)))
    }

    /// An image file on this Mac, read when shown.
    static func file(_ path: String) -> ClaudeChatImage? {
        guard path.hasPrefix("/") else { return nil }
        return ClaudeChatImage(key: key("file", path), source: .file(path))
    }

    /// The images of a prompt's blocks: base64 sources only, as Claude Code
    /// writes a paste.
    static func claude(_ blocks: [[String: Any]]) -> [ClaudeChatImage] {
        var images: [ClaudeChatImage] = []
        for block in blocks where block["type"] as? String == "image" {
            guard images.count < perPrompt else { break }
            guard let source = block["source"] as? [String: Any], source["type"] as? String == "base64",
                  let mediaType = source["media_type"] as? String, let data = source["data"] as? String,
                  let image = inline(base64: data, mediaType: mediaType) else { continue }
            images.append(image)
        }
        return images
    }

    /// The images of a Codex prompt's input: files it names, and images sent
    /// inline. Those already made for the same source are kept, so reading
    /// the thread's history again makes no preview twice.
    static func codex(_ content: [[String: Any]], reusing known: [ClaudeChatImage] = []) -> [ClaudeChatImage] {
        codexSources(content).compactMap { source in known.first { $0.key == source.key } ?? source.make() }
    }

    /// What tells a Codex prompt's images apart, without making any preview.
    static func codexKeys(_ content: [[String: Any]]) -> [Int] { codexSources(content).map(\.key) }

    private static func codexSources(_ content: [[String: Any]]) -> [(key: Int, make: () -> ClaudeChatImage?)] {
        var sources: [(key: Int, make: () -> ClaudeChatImage?)] = []
        for block in content {
            guard sources.count < perPrompt else { break }
            switch block["type"] as? String {
            case "localImage":
                guard let path = block["path"] as? String, let image = file(path) else { continue }
                sources.append((image.key, { image }))
            case "image":
                guard let url = block["url"] as? String, url.hasPrefix("data:image/") else { continue }
                let key = key("inline", url)
                sources.append((key, { inline(dataURL: url).map { ClaudeChatImage(key: key, source: $0.source) } }))
            default: break
            }
        }
        return sources
    }

    private static func key(_ kind: String, _ value: String) -> Int {
        var hasher = Hasher()
        hasher.combine(kind)
        hasher.combine(value)
        return hasher.finalize()
    }

    // MARK: Previews

    /// A JPEG no longer than `previewSide` on its longest side, or nil when
    /// the bytes are no image.
    static func preview(of data: Data, side: Int = previewSide) -> Data? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let image = thumbnail(source, side: side) else { return nil }
        let output = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(output, UTType.jpeg.identifier as CFString, 1, nil)
        else { return nil }
        CGImageDestinationAddImage(destination, image, [kCGImageDestinationLossyCompressionQuality: 0.8] as CFDictionary)
        return CGImageDestinationFinalize(destination) ? output as Data : nil
    }

    private static func thumbnail(_ source: CGImageSource, side: Int) -> CGImage? {
        CGImageSourceCreateThumbnailAtIndex(source, 0, [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: side,
            kCGImageSourceShouldCacheImmediately: true,
        ] as CFDictionary)
    }

    /// Leaves inline previews only on the latest prompts, so a long
    /// conversation of screenshots holds a few, not hundreds. A prompt that
    /// lets its images go keeps its text and their labels.
    static func trim<Messages: BidirectionalCollection & MutableCollection>(
        _ messages: inout Messages, images: WritableKeyPath<Messages.Element, [ClaudeChatImage]>, kept: Int = kept
    ) {
        var count = 0
        var index = messages.endIndex
        while index != messages.startIndex {
            messages.formIndex(before: &index)
            let inline = messages[index][keyPath: images].filter(\.isInline).count
            guard inline > 0 else { continue }
            if count + inline > kept {
                messages[index][keyPath: images].removeAll(where: \.isInline)
            } else {
                count += inline
            }
        }
    }

    // MARK: Showing

    private static let cache: NSCache<NSNumber, NSImage> = {
        let cache = NSCache<NSNumber, NSImage>()
        cache.countLimit = 64
        return cache
    }()
    private static let loading = DispatchQueue(label: "com.vorssaint.claude-chat-images", qos: .userInitiated)

    /// The picture already read, if any.
    static func cached(_ image: ClaudeChatImage) -> NSImage? { cache.object(forKey: image.key as NSNumber) }

    /// Reads the picture away from the main thread and calls back on it;
    /// nil when it is gone, too large or no image.
    static func load(_ image: ClaudeChatImage, completion: @escaping (NSImage?) -> Void) {
        if let known = cached(image) { return completion(known) }
        loading.async {
            let picture = decode(image)
            if let picture { cache.setObject(picture, forKey: image.key as NSNumber) }
            DispatchQueue.main.async { completion(picture) }
        }
    }

    static func decode(_ image: ClaudeChatImage) -> NSImage? {
        let cgImage: CGImage?
        switch image.source {
        case .preview(let data):
            cgImage = CGImageSourceCreateWithData(data as CFData, nil).flatMap { CGImageSourceCreateImageAtIndex($0, 0, nil) }
        case .file(let path):
            let url = URL(fileURLWithPath: path)
            guard let size = (try? url.resourceValues(forKeys: [.fileSizeKey, .isRegularFileKey])),
                  size.isRegularFile == true, (size.fileSize ?? .max) <= largestFile,
                  let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
            cgImage = thumbnail(source, side: previewSide)
        }
        return cgImage.map { NSImage(cgImage: $0, size: NSSize(width: $0.width, height: $0.height)) }
    }

    /// Forgets every picture read, as when previews are turned off.
    static func forget() { cache.removeAllObjects() }
}
