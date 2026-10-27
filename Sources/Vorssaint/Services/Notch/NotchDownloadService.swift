// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import Combine
import Darwin

final class NotchDownloadService: ObservableObject {
    static let shared = NotchDownloadService()

    /// Progress may unpublish on any queue. Only this main-actor boundary
    /// holds the weak UI owner; native callbacks carry a Sendable reference.
    /// Each folder subscribes on its own, so a nested folder's publication
    /// reaches both and only the folder holding the file keeps it.
    @MainActor private final class ProgressCallbacks {
        weak var owner: NotchDownloadService?
        let folder: String
        let watch: UUID

        init(owner: NotchDownloadService, folder: String, watch: UUID) {
            self.owner = owner
            self.folder = folder
            self.watch = watch
        }

        func published(_ progress: Progress, id: UUID) {
            guard let owner, let current = owner.watches[folder], current.id == watch else { return }
            current.observer.add(progress, id: id)
        }

        func unpublished(_ id: UUID) {
            guard let owner, let current = owner.watches[folder], current.id == watch else { return }
            current.observer.remove(id)
        }
    }

    /// One watched folder: its directory events and published progress.
    private struct Watch {
        let id: UUID
        let folder: URL
        let source: DispatchSourceFileSystemObject
        let observer: NotchDownloadProgressObserver
        let subscriber: Any
    }

    @Published private(set) var items: [NotchDownloadItem] = []
    /// What is watched, and chosen folders that cannot be, for Settings and
    /// the page's folder menu.
    @Published private(set) var folders: [NotchDownloadFolder] = []
    /// The last folder choice could not be kept.
    @Published private(set) var folderUnavailable = false
    var onArrival: ((NotchDownloadItem) -> Void)?
    /// A download went away unfinished: cancelled or failed.
    var onFailure: (() -> Void)?

    private var running = false
    private var watches: [String: Watch] = [:]
    /// The saved bookmarks the chosen folders were resolved from.
    private var bookmarks: [Data] = []
    private var chosen: [(url: URL, bookmark: Data)] = []
    private var missing: [Data] = []
    /// Chosen folders whose security scope this process holds.
    private var scopes: [URL] = []
    /// Folders that exist but could not be read. Retrying would only fail
    /// again, so they wait for the next change in Settings.
    private var unreadable: Set<String> = []
    private var discovery: DispatchWorkItem?
    private var fileSources: [URL: DispatchSourceFileSystemObject] = [:]
    private var announcements: NSObjectProtocol?
    private var progressItems: [String: [NotchDownloadItem]] = [:]
    private var folderItems: [NotchDownloadItem] = []
    private var partials: [URL: NotchPartialDownload] = [:]
    private var finished: [NotchDownloadItem] = []
    private var expiry: DispatchWorkItem?
    private var inactivityExpiry: DispatchWorkItem?
    private var scanWork: DispatchWorkItem?
    private var generation = UUID()
    private var scanning = false
    private var rescan = false
    private var chooser: NSOpenPanel?
    var isChoosingFolder: Bool { chooser != nil }
    private var chooserID = UUID()
    /// The pending chooser was begun from the island's Downloads page.
    private var chooserInNotch = false
    private let queue = DispatchQueue(label: "com.vorssaint.notch.downloads", qos: .utility)
    /// Browsers change their folder without telling anyone; look again this often.
    private static let discoveryInterval: TimeInterval = 30

    private var watchedFolders: [URL] { watches.values.map(\.folder) }
    private var progressObservers: [String: NotchDownloadProgressObserver] { watches.mapValues(\.observer) }

    private init() {}

    func syncWithPreferences() {
        guard NotchSupport.isEnabled(), AppFeature.notchDownloads.isAvailable,
              UserDefaults.standard.bool(forKey: DefaultsKey.notchDownloadsEnabled),
              NotchSupport.modules().contains(.downloads) else { stop(); return }
        let saved = UserDefaults.standard.array(forKey: DefaultsKey.notchDownloadsFolderBookmarks) as? [Data] ?? []
        guard !running || saved != bookmarks else { return }
        if !running { begin() }
        resolveChosenFolders(saved)
        unreadable.removeAll()
        discoverFolders()
    }

    private func begin() {
        running = true
        let requested = generation
        announcements = DistributedNotificationCenter.default().addObserver(
            forName: NotchDownloadSupport.finishedNotification, object: nil, queue: .main) { [weak self] note in
            guard let self, self.generation == requested, let path = note.object as? String else { return }
            let folders = self.watchedFolders
            self.queue.async { [weak self] in
                let item = folders.lazy.compactMap { NotchDownloadSupport.announcedItem(atPath: path, folder: $0) }.first
                DispatchQueue.main.async {
                    guard let self, self.generation == requested, let item else { return }
                    self.recordCompletion(item)
                    self.refreshItems()
                }
            }
        }
    }

    /// Chosen folders keep their access through security-scoped bookmarks.
    /// Their scope starts before any read, so a protected folder someone
    /// picked never asks again.
    private func resolveChosenFolders(_ saved: [Data]) {
        let released = scopes
        bookmarks = saved
        chosen = []
        missing = []
        scopes = []
        for data in saved {
            var stale = false
            guard let url = try? URL(resolvingBookmarkData: data,
                                     options: [.withSecurityScope, .withoutUI, .withoutMounting],
                                     relativeTo: nil, bookmarkDataIsStale: &stale), !stale, url.isFileURL else {
                missing.append(data)
                continue
            }
            if url.startAccessingSecurityScopedResource() { scopes.append(url) }
            chosen.append((url, data))
        }
        // Let an already-running file read finish before releasing its scope.
        if !released.isEmpty { queue.async { released.forEach { $0.stopAccessingSecurityScopedResource() } } }
    }

    /// Browser settings are read off the main queue; the folders found
    /// replace the watched set on it.
    private func discoverFolders() {
        discovery?.cancel(); discovery = nil
        let requested = generation
        let chosen = chosen, missing = missing
        queue.async { [weak self] in
            let found = NotchDownloadFolders.discover(chosen: chosen, missing: missing)
            DispatchQueue.main.async {
                guard let self, self.generation == requested, self.running else { return }
                self.apply(found)
                // Looks begun close together each finish here; keep one timer.
                self.discovery?.cancel()
                let work = DispatchWorkItem { [weak self] in self?.discoverFolders() }
                self.discovery = work
                DispatchQueue.main.asyncAfter(deadline: .now() + Self.discoveryInterval, execute: work)
            }
        }
    }

    /// Watch exactly the folders found: new ones start, vanished ones end.
    private func apply(_ found: [NotchDownloadFolder]) {
        var listed: [NotchDownloadFolder] = []
        var wanted = Set<String>()
        for var folder in found {
            let key = folder.url.path
            if !folder.unavailable, !unreadable.contains(key), watches[key] == nil {
                if let watch = startWatching(folder.url) { watches[key] = watch } else { unreadable.insert(key) }
            }
            if !folder.unavailable, unreadable.contains(key) { folder.unavailable = true }
            if !folder.unavailable { wanted.insert(key) }
            listed.append(folder)
        }
        let ended = Set(watches.keys).subtracting(wanted)
        for key in ended { endWatching(key) }
        if folders != listed { folders = listed }
        if !ended.isEmpty { refreshItems() }
        scheduleScan()
    }

    func chooseFolder() {
        guard chooser == nil, AppFeature.notchDownloads.isAvailable else { return }
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.directoryURL = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first
        panel.message = FeatureStrings.notchFiles(L10n.shared.language).addFolderHint
        let parent = folderPickerParent()
        let beganInNotch = parent != nil
        let requested = UUID()
        chooserID = requested
        chooser = panel
        chooserInNotch = beganInNotch
        let completed: (NSApplication.ModalResponse) -> Void = { [weak self, weak panel, weak parent] response in
            guard let self, let panel, self.chooser === panel, self.chooserID == requested else { return }
            self.chooser = nil
            self.chooserInNotch = false
            guard !beganInNotch || parent.map(self.canReturnToDownloads) == true else { return }
            if response == .OK, let url = panel.url, AppFeature.notchDownloads.isAvailable {
                do {
                    let data = try url.bookmarkData(options: .withSecurityScope,
                                                    includingResourceValuesForKeys: nil, relativeTo: nil)
                    // Added beside the folders already watched. Adding one that
                    // is watched anyway still keeps its access by this choice.
                    let saved = UserDefaults.standard.array(forKey: DefaultsKey.notchDownloadsFolderBookmarks) as? [Data] ?? []
                    UserDefaults.standard.set(saved + [data], forKey: DefaultsKey.notchDownloadsFolderBookmarks)
                    UserDefaults.standard.set(true, forKey: DefaultsKey.notchDownloadsEnabled)
                    self.folderUnavailable = false
                    self.syncWithPreferences()
                    NotchService.shared.syncWithPreferences()
                } catch { self.folderUnavailable = true }
            }
            guard let parent else { return }
            let returnID = self.chooserID
            // Native panel dismissal restores its previous key window after the
            // completion callback. Return on the next turn without changing pin.
            DispatchQueue.main.async { [weak self, weak parent] in
                guard let self, let parent, self.chooserID == returnID, self.chooser == nil,
                      self.canReturnToDownloads(parent) else { return }
                NotchService.shared.open(.downloads, feedback: false)
            }
        }
        if let parent {
            // An attached sheet moves/reskins a borderless island. Keep the
            // chooser independent and above its parent instead, without
            // changing the pin; isChoosingFolder keeps the surface alive.
            panel.level = NSWindow.Level(rawValue: parent.level.rawValue + 1)
            // Like the sheet it replaces, it stays up while another app is active.
            panel.hidesOnDeactivate = false
            panel.begin(completionHandler: completed)
            NSApp.activate(ignoringOtherApps: true)
            // Activation alone can leave the nonactivating island holding focus.
            panel.makeKeyAndOrderFront(nil)
        } else {
            NSApp.activate(ignoringOtherApps: true)
            panel.begin(completionHandler: completed)
        }
    }

    private func folderPickerParent() -> NSWindow? {
        guard let window = NotchService.shared.presentationWindow, canReturnToDownloads(window),
              NSApp.currentEvent?.window === window || NSApp.keyWindow === window else { return nil }
        return window
    }

    private func canReturnToDownloads(_ window: NSWindow) -> Bool {
        let notch = NotchService.shared
        return AppFeature.notchDownloads.isAvailable && NotchSupport.isEnabled()
            && NotchSupport.modules().contains(.downloads) && notch.acceptsUserInteraction
            && notch.presentationWindow === window && window.isVisible
            && notch.expanded && notch.selected == .downloads && !notch.showingAppPanel
            && notch.selectedMetric == nil && notch.captureControls == nil
    }

    /// Removes a folder added in Settings. One the Mac or a browser also
    /// names stays watched, without the added access.
    func removeFolder(_ folder: NotchDownloadFolder) {
        let removed = folder.bookmarks
        guard !removed.isEmpty else { return }
        let saved = UserDefaults.standard.array(forKey: DefaultsKey.notchDownloadsFolderBookmarks) as? [Data] ?? []
        UserDefaults.standard.set(saved.filter { !removed.contains($0) }, forKey: DefaultsKey.notchDownloadsFolderBookmarks)
        folderUnavailable = false
        syncWithPreferences()
    }

    private func cancelFolderChoice() {
        chooserID = UUID()
        chooserInNotch = false
        chooser?.cancel(nil)
        chooser = nil
    }

    /// The island's Downloads page went away: a folder chosen now could no
    /// longer return to it and would be dropped in silence, so its chooser
    /// ends with it. One begun in Settings stays up.
    func cancelNotchFolderChoice() {
        guard chooserInNotch, chooser != nil else { return }
        cancelFolderChoice()
    }

    func stop() {
        cancelFolderChoice()
        generation = UUID()
        running = false
        scanWork?.cancel(); scanWork = nil
        expiry?.cancel(); expiry = nil
        inactivityExpiry?.cancel(); inactivityExpiry = nil
        discovery?.cancel(); discovery = nil
        for key in Array(watches.keys) { endWatching(key) }
        fileSources.values.forEach { $0.cancel() }
        fileSources.removeAll()
        progressItems.removeAll()
        if let announcements { DistributedNotificationCenter.default().removeObserver(announcements) }
        announcements = nil
        let released = scopes
        // Let an already-running file read finish before releasing its scope.
        if !released.isEmpty { queue.async { released.forEach { $0.stopAccessingSecurityScopedResource() } } }
        scopes = []
        bookmarks = []
        chosen = []
        missing = []
        unreadable.removeAll()
        folders = []
        folderItems.removeAll()
        partials.removeAll()
        finished.removeAll()
        items = []
        scanning = false
        rescan = false
    }

    private func startWatching(_ url: URL) -> Watch? {
        guard url.isFileURL, let source = watch(url, directory: true) else { return nil }
        let key = url.path
        let id = UUID()
        let requested = generation
        let observer = NotchDownloadProgressObserver(folder: url, queue: queue) { [weak self] items, completed in
            DispatchQueue.main.async {
                guard let self, self.generation == requested, self.watches[key]?.id == id else { return }
                self.progressItems[key] = items
                completed.forEach(self.recordCompletion)
                self.refreshItems()
            }
        }
        let callbacks = MainActor.assumeIsolated { ProgressCallbacks(owner: self, folder: key, watch: id) }
        let subscriber = Progress.addSubscriber(forFileURL: url) { published in
            let id = UUID()
            DispatchQueue.main.async {
                callbacks.published(published, id: id)
            }
            return {
                DispatchQueue.main.async {
                    callbacks.unpublished(id)
                }
            }
        }
        return Watch(id: id, folder: url, source: source, observer: observer, subscriber: subscriber)
    }

    /// A folder left the watched set. What it held leaves the page with it,
    /// without counting as finished or failed.
    private func endWatching(_ key: String) {
        guard let watch = watches.removeValue(forKey: key) else { return }
        watch.source.cancel()
        watch.observer.stop()
        Progress.removeSubscriber(watch.subscriber)
        progressItems.removeValue(forKey: key)
        for (url, partial) in partials where partial.folderPath == key {
            partials.removeValue(forKey: url)
            for watched in [partial.url] + [partial.contentURL].compactMap({ $0 }) {
                fileSources.removeValue(forKey: watched)?.cancel()
            }
        }
        folderItems.removeAll { $0.url.deletingLastPathComponent().standardizedFileURL.path == key }
    }

    /// Folders the scan could not read. One still there is unreadable and
    /// shown so; one gone may come back, which the next look will notice.
    private func lose(_ keys: Set<String>, unreadable stillThere: Set<String>) {
        guard !keys.isEmpty else { return }
        keys.forEach(endWatching)
        unreadable.formUnion(stillThere)
        refreshItems()
        discoverFolders()
    }

    private func watch(_ url: URL, directory: Bool) -> DispatchSourceFileSystemObject? {
        let descriptor = open(url.path, O_EVTONLY | O_NOFOLLOW | O_CLOEXEC)
        guard descriptor >= 0 else { return nil }
        let source = DispatchSource.makeFileSystemObjectSource(fileDescriptor: descriptor,
            eventMask: [.write, .extend, .attrib, .rename, .delete, .revoke], queue: .main)
        source.setEventHandler { [weak self, weak source] in
            guard let self else { return }
            if directory, let source, !source.data.intersection([.rename, .delete, .revoke]).isEmpty {
                // Moved, deleted or unmounted: stop watching it and look again.
                // A chosen folder that moved is found by its bookmark anew.
                self.endWatching(url.path)
                self.refreshItems()
                self.resolveChosenFolders(self.bookmarks)
                self.discoverFolders()
            } else { self.scheduleScan() }
        }
        source.setCancelHandler { close(descriptor) }
        source.resume()
        return source
    }

    private func scheduleScan() {
        guard !watches.isEmpty, scanWork == nil else { return }
        let work = DispatchWorkItem { [weak self] in
            self?.scanWork = nil
            self?.scan()
        }
        scanWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3, execute: work)
    }

    private func scan() {
        let folders = watchedFolders
        guard !folders.isEmpty else { return }
        guard !scanning else { rescan = true; return }
        scanning = true
        let id = generation
        let previous = partials
        let observers = progressObservers
        queue.async { [weak self] in
            var snapshots: [NotchDownloadSupport.FolderSnapshot] = []
            var lost = Set<String>()
            for folder in folders {
                if let snapshot = NotchDownloadSupport.scanFolder(folder) { snapshots.append(snapshot) }
                else { lost.insert(folder.path) }
            }
            let stillThere = lost.filter { NotchDownloadFolders.identity(of: URL(fileURLWithPath: $0)) != nil }
            let current = NotchDownloadSupport.combined(snapshots)
            // A folder that could not be read says nothing about its partials.
            let judged = previous.values.filter { !lost.contains($0.folderPath) }
            let completed = judged.compactMap { old -> NotchDownloadItem? in
                guard current.partials[old.url] == nil,
                      !FileManager.default.fileExists(atPath: old.url.path),
                      let values = try? old.expectedURL.resourceValues(forKeys: NotchDownloadSupport.keys),
                      values.isRegularFile == true,
                      NotchDownloadSupport.didFinish(old, at: old.expectedURL,
                          resourceID: NotchDownloadSupport.fileIdentity(at: old.expectedURL)) else { return nil }
                return NotchDownloadItem(id: old.expectedURL.path, url: old.expectedURL,
                    name: old.expectedURL.lastPathComponent, receivedBytes: Int64(values.fileSize ?? 0),
                    fraction: 1, completed: true, active: false, date: Date())
            }
            // A partial gone with no file in its place and no other partial
            // taking it over did not finish.
            let failed = judged.filter { old in
                current.partials[old.url] == nil
                    && !current.partials.values.contains { $0.expectedURL == old.expectedURL }
                    && !FileManager.default.fileExists(atPath: old.url.path)
                    && !FileManager.default.fileExists(atPath: old.expectedURL.path)
                    && observers[old.folderPath]?.didFinish(old) != true
            }
            DispatchQueue.main.async {
                guard let self, self.generation == id else { return }
                self.scanning = false
                self.partials = current.partials
                self.folderItems = current.files
                let watched = Set(current.partials.values.flatMap { [$0.url] + [$0.contentURL].compactMap { $0 } })
                let removed = Set(self.fileSources.keys).subtracting(watched)
                for url in removed { self.fileSources.removeValue(forKey: url)?.cancel() }
                for url in watched where self.fileSources[url] == nil {
                    self.fileSources[url] = self.watch(url, directory: false)
                }
                completed.forEach(self.recordCompletion)
                if !failed.isEmpty {
                    // The publisher may finish just after its partial vanishes.
                    // Wait for its evidence, never for an unrelated arrival.
                    self.queue.asyncAfter(deadline: .now() + 2) { [weak self] in
                        let unfinished = failed.contains { observers[$0.folderPath]?.didFinish($0) != true }
                        DispatchQueue.main.async {
                            guard let self, self.generation == id, unfinished else { return }
                            self.onFailure?()
                        }
                    }
                }
                self.progressObservers.values.forEach { $0.requestRefresh() }
                self.lose(lost, unreadable: stillThere)
                self.refreshItems()
                if self.rescan { self.rescan = false; self.scheduleScan() }
            }
        }
    }

    private func recordCompletion(_ item: NotchDownloadItem) {
        guard !finished.contains(where: { $0.id == item.id }) else { return }
        finished.insert(item, at: 0)
        finished = Array(finished.prefix(5))
        onArrival?(item)
        expiry?.cancel()
        let work = DispatchWorkItem { [weak self] in
            self?.finished.removeAll(); self?.expiry = nil; self?.refreshItems()
        }
        expiry = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 15, execute: work)
    }

    private func refreshItems() {
        guard running else { return }
        inactivityExpiry?.cancel(); inactivityExpiry = nil
        if let next = partials.values.map({ $0.modified.addingTimeInterval(120) })
            .filter({ $0 > Date() }).min() {
            let work = DispatchWorkItem { [weak self] in self?.refreshItems() }
            inactivityExpiry = work
            DispatchQueue.main.asyncAfter(deadline: .now() + max(0.1, next.timeIntervalSinceNow), execute: work)
        }
        var active = progressItems.values.flatMap { $0 }
        var seen = Set<String>()
        active = active.filter { seen.insert($0.id).inserted }
        let represented = Set(active.map { $0.url.standardizedFileURL })
        for partial in partials.values where !represented.contains(partial.url.standardizedFileURL)
            && !represented.contains(partial.expectedURL.standardizedFileURL) {
            active.append(NotchDownloadItem(id: partial.url.path, url: partial.url,
                name: partial.expectedURL.lastPathComponent, receivedBytes: partial.bytes,
                fraction: nil, completed: false, active: Date().timeIntervalSince(partial.modified) < 120,
                date: partial.modified))
        }
        let updated = NotchDownloadSupport.mergedItems(active: active, files: folderItems, finished: finished)
        if items != updated { items = updated }
    }
}
