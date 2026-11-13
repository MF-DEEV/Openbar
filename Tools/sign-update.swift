#!/usr/bin/env swift
// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

// Signs a Mac update (the release DMG) with Openbar's Ed25519 update key, the
// key whose public half is UpdateSignature.publicKeyBase64 in the app. The app
// refuses a download whose signature does not verify against it.
//
//   swift Tools/sign-update.swift --generate          create the key (once), print the public key
//   swift Tools/sign-update.swift --public-key        print the public key
//   swift Tools/sign-update.swift <file> [out.sig]    write <file>.sig (base64 signature)
//
// The private key lives in the login keychain as a generic password
// (service "openbar-update-signing"). OPENBAR_UPDATE_KEY (base64 raw private
// key) overrides it, for a test key. Back the keychain item up: losing it
// means installed copies can no longer update themselves.

import CryptoKit
import Foundation

let service = "openbar-update-signing"
let account = "ed25519"

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data("✗ \(message)\n".utf8))
    exit(1)
}

@discardableResult
func security(_ arguments: [String]) -> (status: Int32, output: String) {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/security")
    process.arguments = arguments
    let pipe = Pipe()
    process.standardOutput = pipe
    process.standardError = FileHandle.nullDevice
    do { try process.run() } catch { fail("cannot run security: \(error)") }
    let data = pipe.fileHandleForReading.readDataToEndOfFile()
    process.waitUntilExit()
    return (process.terminationStatus, String(decoding: data, as: UTF8.self)
        .trimmingCharacters(in: .whitespacesAndNewlines))
}

func storedKey() -> Curve25519.Signing.PrivateKey? {
    let encoded: String
    if let env = ProcessInfo.processInfo.environment["OPENBAR_UPDATE_KEY"], !env.isEmpty {
        encoded = env
    } else {
        let found = security(["find-generic-password", "-s", service, "-a", account, "-w"])
        guard found.status == 0 else { return nil }
        encoded = found.output
    }
    guard let raw = Data(base64Encoded: encoded) else { fail("the stored update key is not base64") }
    do { return try Curve25519.Signing.PrivateKey(rawRepresentation: raw) } catch {
        fail("the stored update key is not an Ed25519 key")
    }
}

let arguments = Array(CommandLine.arguments.dropFirst())
switch arguments.first {
case "--generate":
    if storedKey() != nil { fail("an update key already exists in the keychain; refusing to replace it") }
    let key = Curve25519.Signing.PrivateKey()
    let saved = security(["add-generic-password", "-s", service, "-a", account,
                          "-l", "Openbar update signing key", "-w", key.rawRepresentation.base64EncodedString()])
    guard saved.status == 0 else { fail("could not save the key to the keychain") }
    print(key.publicKey.rawRepresentation.base64EncodedString())
case "--public-key":
    guard let key = storedKey() else { fail("no update key; run with --generate first") }
    print(key.publicKey.rawRepresentation.base64EncodedString())
case let path?:
    guard let key = storedKey() else { fail("no update key; run with --generate first") }
    let url = URL(fileURLWithPath: path)
    guard let data = try? Data(contentsOf: url, options: .alwaysMapped) else { fail("cannot read \(path)") }
    let signature = try key.signature(for: data)
    let out = arguments.count > 1 ? URL(fileURLWithPath: arguments[1]) : url.appendingPathExtension("sig")
    try Data((signature.base64EncodedString() + "\n").utf8).write(to: out)
    print(out.path)
case nil:
    fail("usage: sign-update.swift --generate | --public-key | <file> [out.sig]")
}
