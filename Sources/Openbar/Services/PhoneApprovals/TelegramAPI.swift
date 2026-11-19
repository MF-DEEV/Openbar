// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Foundation

/// The few calls of Telegram's Bot API that phone approvals make (#214):
/// HTTPS without cookies, cache or redirects. The bot's token is part of
/// every address, so neither the address nor the token is ever logged.
final class TelegramAPI: NSObject, URLSessionTaskDelegate, @unchecked Sendable {
    enum Failure: Error, Equatable {
        /// The token was revoked or never was one.
        case unauthorized
        /// Something else reads this bot's updates: a webhook, or Openbar on
        /// another computer with the same bot.
        case conflict
        /// Too many messages; Telegram says how long to wait.
        case tooMany(TimeInterval)
        case refused(Int, String)
        case network
    }

    static let defaultBase = "https://api.telegram.org"

    /// A stand-in Bot API for tests, or Telegram's own.
    static var base: String {
        var base = ProcessInfo.processInfo.environment["OPENBAR_TELEGRAM_API"]?
            .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        if base.isEmpty { base = defaultBase }
        while base.hasSuffix("/") { base.removeLast() }
        return base
    }

    /// A token as BotFather hands it out: the bot's number, a colon, a secret.
    static func validToken(_ token: String) -> Bool {
        token.range(of: #"^[0-9]{3,20}:[A-Za-z0-9_-]{30,100}$"#, options: .regularExpression) != nil
    }

    private let token: String
    private lazy var session: URLSession = {
        let config = URLSessionConfiguration.ephemeral
        config.httpCookieStorage = nil
        config.httpShouldSetCookies = false
        config.urlCache = nil
        config.requestCachePolicy = .reloadIgnoringLocalCacheData
        config.waitsForConnectivity = false
        return URLSession(configuration: config, delegate: self, delegateQueue: nil)
    }()

    init(token: String) {
        self.token = token
    }

    /// Calls one method; the result on success, else why it failed.
    func call(_ method: String, _ body: [String: Any] = [:], timeout: TimeInterval = 20) async throws -> Any {
        guard let url = URL(string: "\(Self.base)/bot\(token)/\(method)") else { throw Failure.unauthorized }
        var request = URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData, timeoutInterval: timeout)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("Openbar/\(AppInfo.version)", forHTTPHeaderField: "User-Agent")
        request.httpBody = try JSONSerialization.data(withJSONObject: body)
        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: request)
        } catch {
            throw Failure.network
        }
        let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
        if json?["ok"] as? Bool == true { return json?["result"] ?? NSNull() }
        let code = json?["error_code"] as? Int ?? (response as? HTTPURLResponse)?.statusCode ?? 0
        switch code {
        // A malformed token answers Not Found.
        case 401, 404: throw Failure.unauthorized
        case 409: throw Failure.conflict
        case 429:
            let wait = ((json?["parameters"] as? [String: Any])?["retry_after"] as? Int) ?? 5
            throw Failure.tooMany(TimeInterval(min(max(wait, 1), 600)))
        case 400..<500: throw Failure.refused(code, json?["description"] as? String ?? "")
        default: throw Failure.network
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest) async -> URLRequest? {
        nil
    }

    func close() {
        session.invalidateAndCancel()
    }
}
