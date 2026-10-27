// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Which folders Downloads watches: browser settings read from disposable
/// profiles, one entry per physical folder, and the old single choice kept.
enum NotchDownloadFolderTests {
    static func run(_ suite: TestSuite) {
        chromium(suite)
        firefox(suite)
        paths(suite)
        migration(suite)
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("notch-download-folders-\(UUID().uuidString)").resolvingSymlinksInPath()
        do {
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
            defer { try? FileManager.default.removeItem(at: root) }
            try merging(suite, root: root)
            try browsers(suite, root: root)
        } catch { suite.expect(false, "download folder fixture failed: \(error)") }
    }

    private static func chromium(_ suite: TestSuite) {
        let preferences = Data("""
        {"download": {"default_directory": "/Volumes/Data/Downloads", "prompt_for_download": false},
         "savefile": {"default_directory": "/Users/me/Saved \\"Pages\\""},
         "profile": {"name": "Work"}}
        """.utf8)
        suite.expect(NotchDownloadFolders.chromiumDirectories(fromPreferences: preferences)
                     == ["/Volumes/Data/Downloads", "/Users/me/Saved \"Pages\""],
                     "a Chromium profile yields its download and Save As folders, JSON escapes decoded")
        suite.expect(NotchDownloadFolders.chromiumDirectories(fromPreferences: Data(#"{"download": {}}"#.utf8)).isEmpty,
                     "a profile that never changed its folder names none")
        suite.expect(NotchDownloadFolders.chromiumDirectories(
                         fromPreferences: Data(#"{"download": {"default_directory": ""}}"#.utf8)).isEmpty,
                     "an empty folder setting is ignored")
        suite.expect(NotchDownloadFolders.chromiumDirectories(fromPreferences: Data("{not json".utf8)).isEmpty
                     && NotchDownloadFolders.chromiumDirectories(fromPreferences: Data("[1, 2]".utf8)).isEmpty,
                     "a damaged or foreign Preferences file names no folder")
    }

    private static func firefox(_ suite: TestSuite) {
        let custom = #"""
        // Mozilla User Preferences
        user_pref("browser.download.dir", "/Users/me/Down \"loads\\ é\u00e9\ud83d\ude00");
        user_pref("browser.download.folderList", 2);
        user_pref("browser.download.lastDir", "/Users/me/Elsewhere");
        """#
        suite.expect(NotchDownloadFolders.firefoxDirectory(fromPrefs: custom) == "/Users/me/Down \"loads\\ éé😀",
                     "Firefox's custom folder decodes quotes, backslashes and \\u escapes, including surrogate pairs")
        suite.expect(NotchDownloadFolders.firefoxDirectory(fromPrefs: #"""
            user_pref("browser.download.folderList", 2);
            user_pref("browser.download.dir", "/Users/me/D\x41");
            """#) == "/Users/me/DA", "order does not matter and \\x escapes decode")
        for list in ["0", "1"] {
            suite.expect(NotchDownloadFolders.firefoxDirectory(fromPrefs: """
                user_pref("browser.download.dir", "/Users/me/Custom");
                user_pref("browser.download.folderList", \(list));
                """) == nil, "folder list \(list) means Firefox does not use its custom path")
        }
        suite.expect(NotchDownloadFolders.firefoxDirectory(fromPrefs: #"user_pref("browser.download.dir", "/Users/me/Custom");"#) == nil,
                     "without a folder list Firefox saves to Downloads, not to a remembered custom path")
        suite.expect(NotchDownloadFolders.firefoxDirectory(fromPrefs: #"""
            user_pref("browser.download.folderList", 2);
            user_pref("browser.download.dir", "/Users/me/broken\u00");
            """#) == nil, "a malformed escape is rejected rather than guessed")
        suite.expect(NotchDownloadFolders.firefoxDirectory(fromPrefs: #"""
            user_pref("browser.download.folderList", 2);
            user_pref("browser.download.dir", "/Users/me/lonely\ud83d");
            """#) == nil, "an unpaired surrogate is rejected")
        suite.expect(NotchDownloadFolders.userPref(#"  user_pref( 'browser.download.dir' ,  '/Users/me/It\'s' ) ;"#)?.1
                     == .string("/Users/me/It's"), "single quotes and loose spacing parse like Firefox writes them")
        suite.expect(NotchDownloadFolders.userPref(#"pref("browser.download.dir", "/x");"#) == nil
                     && NotchDownloadFolders.userPref("// user_pref(\"browser.download.dir\", \"/x\");") == nil,
                     "only user_pref lines are read")
    }

    private static func paths(_ suite: TestSuite) {
        let home = URL(fileURLWithPath: "/Users/me", isDirectory: true)
        suite.expect(NotchDownloadFolders.directoryURL("~/Downloads", home: home)?.path == "/Users/me/Downloads",
                     "a tilde path resolves under home")
        suite.expect(NotchDownloadFolders.directoryURL("/Users/me/./a/../Downloads/", home: home)?.path == "/Users/me/Downloads",
                     "dot segments and a trailing slash normalise away")
        suite.expect(NotchDownloadFolders.directoryURL("Downloads", home: home) == nil
                     && NotchDownloadFolders.directoryURL("", home: home) == nil
                     && NotchDownloadFolders.directoryURL("C:\\Users\\me", home: home) == nil,
                     "relative, empty and foreign paths are never watched")
    }

    private static func migration(_ suite: TestSuite) {
        let domain = "com.vorssaint.tests.notch-download-folders"
        let defaults = UserDefaults(suiteName: domain)!
        defaults.removePersistentDomain(forName: domain)
        defer { defaults.removePersistentDomain(forName: domain) }
        Defaults.migrateNotchDownloadsFolders(in: defaults)
        suite.expect(defaults.object(forKey: DefaultsKey.notchDownloadsFolderBookmarks) == nil,
                     "nothing to migrate leaves the new list unset")
        let legacy = Data([7, 7, 7])
        defaults.set(legacy, forKey: DefaultsKey.notchDownloadsFolderBookmark)
        Defaults.migrateNotchDownloadsFolders(in: defaults)
        suite.expect(defaults.array(forKey: DefaultsKey.notchDownloadsFolderBookmarks) as? [Data] == [legacy]
                     && defaults.object(forKey: DefaultsKey.notchDownloadsFolderBookmark) == nil,
                     "the single chosen folder becomes the first added folder")
        Defaults.migrateNotchDownloadsFolders(in: defaults)
        suite.expect(defaults.array(forKey: DefaultsKey.notchDownloadsFolderBookmarks) as? [Data] == [legacy],
                     "migrating again changes nothing")
        let added = Data([1])
        defaults.set([added], forKey: DefaultsKey.notchDownloadsFolderBookmarks)
        defaults.set(legacy, forKey: DefaultsKey.notchDownloadsFolderBookmark)
        Defaults.migrateNotchDownloadsFolders(in: defaults)
        suite.expect(defaults.array(forKey: DefaultsKey.notchDownloadsFolderBookmarks) as? [Data] == [legacy, added],
                     "an old choice restored beside a newer list joins it without replacing it")
        defaults.set([legacy], forKey: DefaultsKey.notchDownloadsFolderBookmarks)
        defaults.set(legacy, forKey: DefaultsKey.notchDownloadsFolderBookmark)
        Defaults.migrateNotchDownloadsFolders(in: defaults)
        suite.expect(defaults.array(forKey: DefaultsKey.notchDownloadsFolderBookmarks) as? [Data] == [legacy],
                     "the same choice is never listed twice")
        suite.expect(!SettingsBackupSupport.exportKeys().contains(DefaultsKey.notchDownloadsFolderBookmarks),
                     "added folders are authority on one Mac and never travel in a settings backup")
    }

    private static func merging(_ suite: TestSuite, root: URL) throws {
        let fm = FileManager.default
        let downloads = root.appendingPathComponent("Downloads", isDirectory: true)
        let nested = downloads.appendingPathComponent("Chrome", isDirectory: true)
        let other = root.appendingPathComponent("Other", isDirectory: true)
        try fm.createDirectory(at: nested, withIntermediateDirectories: true)
        try fm.createDirectory(at: other, withIntermediateDirectories: true)
        let link = root.appendingPathComponent("Linked")
        try fm.createSymbolicLink(at: link, withDestinationURL: downloads)
        let missing = root.appendingPathComponent("Missing", isDirectory: true)
        let folders = NotchDownloadFolders.merged([
            (downloads, .downloads),
            (link, .browser("Chrome")),
            (URL(fileURLWithPath: downloads.path + "/../Downloads/", isDirectory: true), .browser("Firefox")),
            (nested, .browser("Edge")),
            (missing, .browser("Brave")),
            (other, .chosen(Data([1]))),
            (downloads, .downloads),
        ])
        suite.expect(folders.map(\.url.path) == [downloads.path, nested.path, other.path],
                     "one entry per real folder, in order of first mention, missing folders left out")
        suite.expect(folders.first?.sources == [.downloads, .browser("Chrome"), .browser("Firefox")],
                     "a symlink and an unnormalised path join the folder they name, each reason kept once")
        suite.expect(folders.dropFirst().first?.browsers == ["Edge"],
                     "a folder inside a watched one stays separate: scans read only their own top level")
        suite.expect(folders.last?.bookmarks == [Data([1])], "a chosen folder keeps the bookmark that grants it")
        let linkedOnly = NotchDownloadFolders.merged([(link, .browser("Chrome"))])
        suite.expect(linkedOnly.first?.url.path == downloads.path,
                     "a folder named through a symlink is watched at its real path")
        // On the usual case-insensitive volume another spelling is the same
        // folder; on a case-sensitive one it is simply another (missing) path.
        let shouted = URL(fileURLWithPath: downloads.path.replacingOccurrences(of: "/Downloads", with: "/DOWNLOADS"),
                          isDirectory: true)
        let caseless = NotchDownloadFolders.identity(of: shouted) != nil
        let cased = NotchDownloadFolders.merged([(downloads, .downloads), (shouted, .browser("Arc"))])
        suite.expect(cased.count == 1 && cased[0].url.path == downloads.path
                     && cased[0].sources == (caseless ? [.downloads, .browser("Arc")] : [.downloads]),
                     "letter case never makes the same folder twice")
        let file = root.appendingPathComponent("not-a-folder")
        try Data([1]).write(to: file)
        suite.expect(NotchDownloadFolders.merged([(file, .browser("Chrome"))]).isEmpty,
                     "a file where a folder was expected is not watched")

        let found = NotchDownloadFolders.discover(chosen: [(downloads, Data([2]))], missing: [Data([3]), Data([4])],
                                                  downloads: downloads, home: root)
        suite.expect(found.count == 2 && found[0].url.path == downloads.path && !found[0].unavailable
                     && found[0].sources == [.downloads, .chosen(Data([2]))],
                     "adding the Downloads folder in Settings keeps one row that also carries its access")
        suite.expect(found[1].unavailable && found[1].bookmarks == [Data([3]), Data([4])],
                     "chosen folders that no longer resolve stay listed, unavailable, so they can be removed")
        let ejected = NotchDownloadFolders.discover(chosen: [(missing, Data([5]))], missing: [],
                                                    downloads: nil, home: root)
        suite.expect(ejected.count == 1 && ejected[0].unavailable && ejected[0].url.path == missing.path
                     && ejected[0].bookmarks == [Data([5])],
                     "a chosen folder that is gone for now stays listed as unavailable, under its own path")
        let nothing = NotchDownloadFolders.discover(chosen: [], missing: [], downloads: missing, home: root)
        suite.expect(nothing.isEmpty, "a Mac without the folders watches nothing and starts no watcher")
    }

    private static func browsers(_ suite: TestSuite, root: URL) throws {
        let fm = FileManager.default
        let home = root.appendingPathComponent("home", isDirectory: true)
        let support = home.appendingPathComponent("Library/Application Support", isDirectory: true)
        func write(_ text: String, to relative: String) throws {
            let url = support.appendingPathComponent(relative)
            try fm.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data(text.utf8).write(to: url)
        }
        try write(#"{"download": {"default_directory": "/Volumes/Chrome Default"}}"#, to: "Google/Chrome/Default/Preferences")
        try write(#"{"savefile": {"default_directory": "~/Saved"}}"#, to: "Google/Chrome/Profile 2/Preferences")
        try write(#"{"download": {"default_directory": "relative/path"}}"#, to: "Google/Chrome/Profile 3/Preferences")
        try write("not json", to: "Microsoft Edge/Default/Preferences")
        try write(#"{"download": {"default_directory": "/Volumes/Opera"}}"#, to: "com.operasoftware.Opera/Preferences")
        try write(#"{"download": {"default_directory": "/Volumes/Arc"}}"#, to: "Arc/User Data/Default/Preferences")
        try write("""
            user_pref("browser.download.folderList", 2);
            user_pref("browser.download.dir", "/Volumes/Fox");
            """, to: "Firefox/Profiles/abc.default-release/prefs.js")
        try write(#"user_pref("browser.download.dir", "/Volumes/Unused");"#, to: "Firefox/Profiles/old.default/prefs.js")
        try write("""
            user_pref("browser.download.dir", "/Volumes/Zen");
            user_pref("browser.download.folderList", 2);
            """, to: "zen/Profiles/z.Default (release)/prefs.js")
        let found = NotchDownloadFolders.browserFolders(home: home)
        let pairs = Set(found.map { "\($0.browser)=\($0.url.path)" })
        suite.expect(pairs == ["Chrome=/Volumes/Chrome Default", "Chrome=\(home.path)/Saved", "Opera=/Volumes/Opera",
                               "Arc=/Volumes/Arc", "Firefox=/Volumes/Fox", "Zen=/Volumes/Zen"],
                     "every browser profile's folder is found, a profile root included; broken and relative settings are skipped: \(pairs.sorted())")
        suite.expect(NotchDownloadFolders.browserFolders(home: root.appendingPathComponent("nobody")).isEmpty,
                     "a Mac without browsers finds no folders")
    }
}
