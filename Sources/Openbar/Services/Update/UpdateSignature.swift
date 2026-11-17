// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import CryptoKit
import Foundation
import Security

/// Who may ship Openbar to an installed copy. A downloaded update must carry
/// an Ed25519 signature made with Openbar's update key (Tools/sign-update.swift)
/// before it reaches the installer, and the installer then insists that the new
/// bundle satisfies the running app's own code requirement. A download changed
/// on the way, or a bundle signed by someone else, is refused.
enum UpdateSignature {
    /// Public half of the update key in the release keychain.
    static let publicKeyBase64 = "hPt7FvEZxr1a3NEPutYc8pqndrXEoiIypPNrF6rDJWk="

    /// A `.sig` file is a base64 signature and a newline; anything larger is not one.
    static let maxSignatureBytes = 1024

    /// True only when `signatureText` is a valid signature of the file's bytes
    /// under `publicKeyBase64`. Every malformed input is a plain false.
    static func verify(fileAt url: URL,
                       signatureText: String,
                       publicKeyBase64: String = publicKeyBase64) -> Bool {
        guard let data = try? Data(contentsOf: url, options: .alwaysMapped) else { return false }
        return verify(data: data, signatureText: signatureText, publicKeyBase64: publicKeyBase64)
    }

    static func verify(data: Data, signatureText: String, publicKeyBase64: String = publicKeyBase64) -> Bool {
        let trimmed = signatureText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let signature = Data(base64Encoded: trimmed), signature.count == 64,
              let keyData = Data(base64Encoded: publicKeyBase64),
              let key = try? Curve25519.Signing.PublicKey(rawRepresentation: keyData)
        else { return false }
        return key.isValidSignature(signature, for: data)
    }

    /// The running app's designated requirement as text, such as
    /// `identifier "com.openbar.app" and certificate leaf = H"…"`. The new
    /// bundle must satisfy it, so an update keeps the identity the system's
    /// privacy grants are tied to. Nil for an unsigned process.
    static func runningDesignatedRequirement() -> String? {
        var code: SecCode?
        guard SecCodeCopySelf([], &code) == errSecSuccess, let code else { return nil }
        var staticCode: SecStaticCode?
        guard SecCodeCopyStaticCode(code, [], &staticCode) == errSecSuccess, let staticCode else { return nil }
        var requirement: SecRequirement?
        guard SecCodeCopyDesignatedRequirement(staticCode, [], &requirement) == errSecSuccess,
              let requirement else { return nil }
        var text: CFString?
        guard SecRequirementCopyString(requirement, [], &text) == errSecSuccess, let text else { return nil }
        return text as String
    }
}
