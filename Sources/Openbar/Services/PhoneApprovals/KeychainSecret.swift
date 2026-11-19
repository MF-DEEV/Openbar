// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Foundation
import Security

/// A secret kept in the login keychain, never in the app's preferences. The
/// item is named after the app's bundle id, so a test copy under another id
/// neither reads nor replaces the real app's.
enum KeychainSecret {
    static func service(_ name: String) -> String {
        (Bundle.main.bundleIdentifier ?? "com.openbar.app") + "." + name
    }

    private static func query(_ name: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: service(name),
         kSecAttrAccount as String: name]
    }

    static func read(_ name: String) -> String? {
        var query = query(name)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess, let data = item as? Data else { return nil }
        return String(data: data, encoding: .utf8)
    }

    @discardableResult
    static func write(_ value: String, _ name: String, label: String) -> Bool {
        let data = Data(value.utf8)
        let update: [String: Any] = [kSecValueData as String: data, kSecAttrLabel as String: label]
        let status = SecItemUpdate(query(name) as CFDictionary, update as CFDictionary)
        if status == errSecSuccess { return true }
        guard status == errSecItemNotFound else { return false }
        var add = query(name)
        add[kSecValueData as String] = data
        add[kSecAttrLabel as String] = label
        return SecItemAdd(add as CFDictionary, nil) == errSecSuccess
    }

    @discardableResult
    static func delete(_ name: String) -> Bool {
        let status = SecItemDelete(query(name) as CFDictionary)
        return status == errSecSuccess || status == errSecItemNotFound
    }
}
