// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import UniformTypeIdentifiers

/// Images added to a reply. Claude Code attaches an image when a paste is
/// nothing but the path of a PNG, JPEG, GIF or WebP file, so each image
/// becomes such a file: one of those kinds is used where it lies, anything
/// else is saved as a PNG in a folder of the app's own that the system
/// clears.
enum ClaudeImageAttachments {
    /// The kinds Claude Code reads from a pasted path.
    static let readable: Set<String> = ["png", "jpg", "jpeg", "gif", "webp"]
    /// Images one reply carries at most.
    static let limit = 5
    /// Larger files are left out; Claude Code would refuse them anyway.
    static let largest = 20 << 20

    static var folder: URL {
        FileManager.default.temporaryDirectory.appending(path: "vorssaint-images", directoryHint: .isDirectory)
    }

    /// Whether a paste should add images rather than text: image files, or
    /// an image with no text beside it, as a screenshot or Copy Image gives.
    static func holdsImages(_ pasteboard: NSPasteboard) -> Bool {
        if !imageFiles(pasteboard).isEmpty { return true }
        return pasteboard.string(forType: .string)?.isEmpty != false && NSImage.canInit(with: pasteboard)
    }

    /// The pasteboard's images as files Claude Code can read.
    static func images(from pasteboard: NSPasteboard) -> [URL] {
        let files = imageFiles(pasteboard)
        if !files.isEmpty { return files.compactMap(usable) }
        guard pasteboard.string(forType: .string)?.isEmpty != false,
              let image = NSImage(pasteboard: pasteboard) else { return [] }
        return save(image).map { [$0] } ?? []
    }

    /// Files dropped or pasted that are images, in the order given.
    static func imageFiles(_ pasteboard: NSPasteboard) -> [URL] {
        let urls = pasteboard.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL] ?? []
        return urls.filter(isImage)
    }

    static func isImage(_ url: URL) -> Bool {
        guard url.isFileURL, let type = UTType(filenameExtension: url.pathExtension) else { return false }
        return type.conforms(to: .image)
    }

    /// A file Claude Code reads as it is, or a PNG copy of any other image.
    static func usable(_ url: URL) -> URL? {
        guard let size = (try? url.resourceValues(forKeys: [.fileSizeKey]))?.fileSize, size <= largest else { return nil }
        if readable.contains(url.pathExtension.lowercased()) { return url }
        return NSImage(contentsOf: url).flatMap(save)
    }

    static func save(_ image: NSImage) -> URL? {
        guard let tiff = image.tiffRepresentation, let bitmap = NSBitmapImageRep(data: tiff),
              let png = bitmap.representation(using: .png, properties: [:]), png.count <= largest else { return nil }
        let folder = folder
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true,
                                                 attributes: [.posixPermissions: 0o700])
        let url = folder.appending(path: UUID().uuidString + ".png")
        do {
            try png.write(to: url, options: [.atomic])
            try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
            return url
        } catch {
            return nil
        }
    }

    /// Images dropped on a reply: files, or image data saved as PNG.
    /// Calls back on the main thread with what could be read.
    static func images(from providers: [NSItemProvider], completion: @escaping ([URL]) -> Void) {
        let group = DispatchGroup()
        let lock = NSLock()
        var found: [(Int, URL)] = []
        for (index, provider) in providers.enumerated() {
            group.enter()
            let add = { (url: URL?) in
                if let url { lock.lock(); found.append((index, url)); lock.unlock() }
                group.leave()
            }
            if provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) {
                _ = provider.loadDataRepresentation(for: .fileURL) { data, _ in
                    let url = data.flatMap { URL(dataRepresentation: $0, relativeTo: nil) }
                    add(url.flatMap { isImage($0) ? usable($0) : nil })
                }
            } else if provider.hasItemConformingToTypeIdentifier(UTType.image.identifier) {
                _ = provider.loadDataRepresentation(for: .image) { data, _ in
                    add(data.flatMap(NSImage.init(data:)).flatMap(save))
                }
            } else {
                group.leave()
            }
        }
        group.notify(queue: .main) { completion(found.sorted { $0.0 < $1.0 }.map(\.1)) }
    }

    /// Clears saved copies a day old, which Claude Code has long since read.
    static func sweep(now: Date = Date()) {
        let folder = folder
        guard let names = try? FileManager.default.contentsOfDirectory(atPath: folder.path) else { return }
        for name in names {
            let url = folder.appending(path: name)
            let modified = (try? url.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate
            if let modified, now.timeIntervalSince(modified) > 86_400 { try? FileManager.default.removeItem(at: url) }
        }
    }
}
