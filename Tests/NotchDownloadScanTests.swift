// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// The production scan and completion methods run with the real filesystem
/// and progress observer. Only UI refreshes and file watchers are replaced.
enum NotchDownloadScanTests {
    class Fixture {
        let queue = DispatchQueue(label: "com.vorssaint.tests.download-scan")
        var folder: URL?
        var generation = UUID()
        var progressObserver: NotchDownloadProgressObserver?
        /// No browser store unless a test gives it a home with one.
        var browserDownloads = NotchChromiumDownloads.Reader(home: URL(fileURLWithPath: "/var/empty"))
        var partials: [URL: NotchPartialDownload] = [:]
        var folderItems: [NotchDownloadItem] = []
        var fileSources: [URL: DispatchSourceFileSystemObject] = [:]
        var finished: [NotchDownloadItem] = []
        var expiry: DispatchWorkItem?
        var scanning = false
        var rescan = false
        var folderUnavailable = false
        var onArrival: ((NotchDownloadItem) -> Void)?
        var onFailure: (() -> Void)?
        var refreshes = 0
        /// Folders the scan gave up on, and those of them still there.
        var lost: Set<String> = []
        var unreadable: Set<String> = []
        /// Folders watched beside the first, without their own observers.
        var extraFolders: [URL] = []
        var watchedFolders: [URL] { (folder.map { [$0] } ?? []) + extraFolders }
        var progressObservers: [String: NotchDownloadProgressObserver] {
            guard let folder, let progressObserver else { return [:] }
            return [folder.path: progressObserver]
        }
        func lose(_ keys: Set<String>, unreadable stillThere: Set<String>) {
            lost.formUnion(keys)
            unreadable.formUnion(stillThere)
        }
        func watch(_ url: URL, directory: Bool) -> DispatchSourceFileSystemObject? { nil }
        func scheduleScan() {}
        func refreshItems() { refreshes += 1 }
        func stop() {
            generation = UUID()
            expiry?.cancel()
            progressObserver?.stop()
            progressObserver = nil
            folder = nil
        }
    }

    static func run(_ suite: TestSuite, folder: URL) throws {
        try concurrentDownloads(suite, folder: folder)
        try severalFolders(suite, folder: folder)
        try folderLeavingDuringScan(suite, folder: folder)
        for completion in ["before", "after", "cancelled", "unknown", "stopped"] {
            try archive(suite, folder: folder, completion: completion)
        }
    }

    private static func wait(timeout: TimeInterval = 3, until condition: () -> Bool) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while !condition(), Date() < deadline {
            _ = RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.01))
        }
        return condition()
    }

    private static func concurrentDownloads(_ suite: TestSuite, folder: URL) throws {
        let root = folder.appendingPathComponent("concurrent-downloads")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        let success = root.appendingPathComponent("success.part")
        let cancelled = root.appendingPathComponent("cancelled.part")
        try Data([1]).write(to: success)
        try Data([2]).write(to: cancelled)
        let service = Service()
        service.folder = root
        service.partials = NotchDownloadSupport.scanFolder(root)?.partials ?? [:]
        var arrivals = 0
        var failures = 0
        service.onArrival = { _ in arrivals += 1 }
        service.onFailure = { failures += 1 }
        defer { service.stop(); service.queue.sync {} }
        try FileManager.default.moveItem(at: success, to: success.deletingPathExtension())
        try FileManager.default.removeItem(at: cancelled)
        service.scan()
        suite.expect(wait { arrivals == 1 && failures == 1 },
                     "one completed download cannot suppress another download's cancellation in the same scan")
        suite.expect(service.partials.isEmpty && service.finished.count == 1,
                     "the scan retires both partials while retaining only the successful arrival")
    }

    /// One scan reads every watched folder. A folder that vanished or cannot
    /// be read is handed back, and its partials are neither finished nor failed.
    private static func severalFolders(_ suite: TestSuite, folder: URL) throws {
        let fm = FileManager.default
        let names = ["first", "second", "gone", "locked"]
        let roots = names.map { folder.appendingPathComponent("several-\($0)", isDirectory: true) }
        for root in roots { try fm.createDirectory(at: root, withIntermediateDirectories: false) }
        let finishing = roots[0].appendingPathComponent("report.pdf.crdownload")
        let waiting = roots[1].appendingPathComponent("movie.mp4.part")
        try Data([1]).write(to: finishing)
        try Data([2]).write(to: waiting)
        try Data([3]).write(to: roots[2].appendingPathComponent("vanishing.part"))
        try Data([4]).write(to: roots[3].appendingPathComponent("hidden.part"))
        let service = Service()
        service.folder = roots[0]
        service.extraFolders = Array(roots.dropFirst())
        service.partials = NotchDownloadSupport.combined(roots.compactMap(NotchDownloadSupport.scanFolder)).partials
        suite.expect(service.partials.count == 4, "every watched folder's partial is tracked together")
        var arrivals: [String] = []
        var failures = 0
        service.onArrival = { arrivals.append($0.name) }
        service.onFailure = { failures += 1 }
        defer {
            _ = try? fm.setAttributes([.posixPermissions: 0o755], ofItemAtPath: roots[3].path)
            service.stop(); service.queue.sync {}
        }
        try fm.moveItem(at: finishing, to: finishing.deletingPathExtension())
        try fm.removeItem(at: roots[2])
        try fm.setAttributes([.posixPermissions: 0], ofItemAtPath: roots[3].path)
        service.scan()
        suite.expect(wait { arrivals == ["report.pdf"] && !service.scanning },
                     "a partial finishing in one folder arrives while the others are scanned too")
        suite.expect(Set(service.partials.keys.map(\.path)) == [waiting.standardizedFileURL.path],
                     "partials of folders read this time stay tracked; those of folders not read are dropped")
        suite.expect(service.folderItems.contains { $0.name == "report.pdf" },
                     "files of every folder are listed together")
        suite.expect(service.lost == [roots[2].path, roots[3].path] && service.unreadable == [roots[3].path],
                     "a vanished folder and an unreadable one are handed back; only the one still there is unreadable")
        let deadline = Date().addingTimeInterval(2.2)
        _ = wait { Date() >= deadline }
        suite.expect(failures == 0, "a partial in a folder that vanished or cannot be read is not called a failure")
    }

    /// A folder that stops being watched while its scan runs does not come
    /// back with the result, and its partial never counts as failed later.
    private static func folderLeavingDuringScan(_ suite: TestSuite, folder: URL) throws {
        let fm = FileManager.default
        let kept = folder.appendingPathComponent("leaving-kept", isDirectory: true)
        let leaving = folder.appendingPathComponent("leaving-gone", isDirectory: true)
        try fm.createDirectory(at: kept, withIntermediateDirectories: false)
        try fm.createDirectory(at: leaving, withIntermediateDirectories: false)
        let partial = leaving.appendingPathComponent("late.zip.part")
        try Data([1]).write(to: partial)
        try Data([2]).write(to: leaving.appendingPathComponent("listed.txt"))
        let service = Service()
        service.folder = kept
        service.extraFolders = [leaving]
        var failures = 0
        service.onFailure = { failures += 1 }
        defer { service.stop(); service.queue.sync {} }
        service.scan()
        // Unwatched before the scan's result reaches the main queue.
        service.extraFolders = []
        suite.expect(wait { service.refreshes > 0 && !service.scanning },
                     "the scan begun with both folders finishes")
        suite.expect(service.partials.isEmpty && service.folderItems.isEmpty,
                     "a folder unwatched during its scan brings back neither its partials nor its files")
        try fm.removeItem(at: partial)
        service.scan()
        _ = wait { !service.scanning && service.refreshes > 1 }
        let deadline = Date().addingTimeInterval(2.2)
        _ = wait { Date() >= deadline }
        suite.expect(failures == 0, "a partial of a folder no longer watched is never called a failure")
    }

    private static func archive(_ suite: TestSuite, folder: URL, completion: String) throws {
        let root = folder.appendingPathComponent("archive-\(completion)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        let package = root.appendingPathComponent("archive.zip.download", isDirectory: true)
        try FileManager.default.createDirectory(at: package, withIntermediateDirectories: false)
        try Data([1]).write(to: package.appendingPathComponent("archive.zip"))
        let service = Service()
        service.folder = root
        service.partials = NotchDownloadSupport.scanFolder(root)?.partials ?? [:]
        let observer = NotchDownloadProgressObserver(folder: root, queue: service.queue) { _, _ in }
        service.progressObserver = observer
        var failures = 0
        service.onFailure = { failures += 1 }
        defer { service.stop(); service.queue.sync {} }
        let progress = Progress(totalUnitCount: 10)
        progress.kind = .file
        progress.fileOperationKind = .downloading
        progress.fileURL = package
        let id = UUID()
        if completion != "unknown" {
            observer.add(progress, id: id)
            service.queue.sync {}
        }
        let extracted = root.appendingPathComponent("unrelated extracted name", isDirectory: true)
        try FileManager.default.createDirectory(at: extracted, withIntermediateDirectories: false)
        try FileManager.default.removeItem(at: package)
        func finish() {
            progress.fileURL = extracted
            progress.completedUnitCount = 10
            observer.remove(id)
            service.queue.sync {}
        }
        if completion == "before" { finish() }
        if completion == "cancelled" { progress.cancel(); observer.remove(id); service.queue.sync {} }
        service.scan()
        suite.expect(wait { service.refreshes > 0 && !service.scanning },
                     "\(completion): the folder scan sees the missing package")
        if completion == "after" { finish() }
        if completion == "stopped" { service.stop() }
        let deadline = Date().addingTimeInterval(2.2)
        _ = wait { Date() >= deadline }
        let expected = completion == "cancelled" || completion == "unknown" ? 1 : 0
        suite.expect(failures == expected,
                     "\(completion): only this transfer's proven success or an invalidated generation suppresses failure")
    }
}
