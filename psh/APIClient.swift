//
//  APIClient.swift
//  psh
//

import Foundation
#if canImport(UIKit)
import UIKit
#endif

struct RegisterRequest: Encodable {
    let deviceToken: String
    let installationId: String
    let environment: String
    let deviceName: String?
    let deviceType: String?
    let osVersion: String?
    let appVersion: String?

    enum CodingKeys: String, CodingKey {
        case deviceToken = "device_token"
        case installationId = "installation_id"
        case environment
        case deviceName = "device_name"
        case deviceType = "device_type"
        case osVersion = "os_version"
        case appVersion = "app_version"
    }
}

struct RegisterResponse: Decodable {
    let success: Bool
    let message: String
}

struct ServerPush: Decodable, Identifiable, Hashable {
    let id: Int64
    let deviceToken: String
    let apnsId: String?
    let title: String?
    let body: String?
    let payload: String?
    let sentAt: String

    enum CodingKeys: String, CodingKey {
        case id
        case deviceToken = "device_token"
        case apnsId = "apns_id"
        case title, body, payload
        case sentAt = "sent_at"
    }
}

struct PushesResponse: Decodable {
    let pushes: [ServerPush]
}

struct ServerPushDetail: Decodable {
    let id: Int64
    let apnsId: String?
    let title: String?
    let body: String?
    let payload: String?
    let sentAt: String
    let deviceToken: String
    let deviceName: String?
    let deviceType: String?
    let environment: String?

    enum CodingKeys: String, CodingKey {
        case id
        case apnsId = "apns_id"
        case title, body, payload
        case sentAt = "sent_at"
        case deviceToken = "device_token"
        case deviceName = "device_name"
        case deviceType = "device_type"
        case environment
    }
}

enum SoundConfig: Encodable {
    case simple(String)
    case critical(name: String, volume: Double?)

    func encode(to encoder: Encoder) throws {
        switch self {
        case .simple(let name):
            var container = encoder.singleValueContainer()
            try container.encode(name)
        case .critical(let name, let volume):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(name, forKey: .name)
            try container.encode(1, forKey: .critical)
            if let volume = volume {
                try container.encode(volume, forKey: .volume)
            }
        }
    }

    private enum CodingKeys: String, CodingKey {
        case name, critical, volume
    }
}

struct SendRequest: Encodable {
    var title: String?
    var subtitle: String?
    var body: String?
    var badge: Int?
    var sound: SoundConfig?
    var contentAvailable: Bool?
    var mutableContent: Bool?
    var category: String?
    var priority: Int?
    var collapseId: String?
    var expiration: Int?
    var interruptionLevel: String?
    var relevanceScore: Double?
    var data: [String: String]?

    enum CodingKeys: String, CodingKey {
        case title, subtitle, body, badge, sound, category, priority, expiration, data
        case contentAvailable = "content_available"
        case mutableContent = "mutable_content"
        case collapseId = "collapse_id"
        case interruptionLevel = "interruption_level"
        case relevanceScore = "relevance_score"
    }
}

struct SendResponse: Decodable {
    let success: Bool
    let sent: Int
    let failed: Int
}

struct StatsResponse: Decodable {
    let totalDevices: Int
    let sandboxDevices: Int
    let productionDevices: Int
    let totalPushes: Int

    enum CodingKeys: String, CodingKey {
        case totalDevices = "total_devices"
        case sandboxDevices = "sandbox_devices"
        case productionDevices = "production_devices"
        case totalPushes = "total_pushes"
    }
}

final class APIClient: Sendable {
    static let shared = APIClient()

    private static let deviceTokenKey = "deviceToken"
    private static let installationIdKey = "installationId"

    let baseURL: URL

    init(baseURL: URL = URL(string: "https://psh.fishmt.net")!) {
        self.baseURL = baseURL
    }

    var storedDeviceToken: String? {
        UserDefaults.standard.string(forKey: Self.deviceTokenKey)
    }

    var installationId: String {
        if let existing = UserDefaults.standard.string(forKey: Self.installationIdKey) {
            return existing
        }
        let newId = UUID().uuidString
        UserDefaults.standard.set(newId, forKey: Self.installationIdKey)
        return newId
    }

    func register(deviceToken: Data) async throws {
        let tokenString = deviceToken.map { String(format: "%02x", $0) }.joined()
        UserDefaults.standard.set(tokenString, forKey: Self.deviceTokenKey)

        #if DEBUG
        let environment = "sandbox"
        #else
        let environment = "production"
        #endif

        var deviceName: String?
        var deviceType: String?

        #if canImport(UIKit) && !os(watchOS)
        await MainActor.run {
            deviceName = UIDevice.current.name
            deviceType = UIDevice.current.model
        }
        #elseif os(macOS)
        deviceName = Host.current().localizedName
        deviceType = "Mac"
        #endif

        let osVersion = ProcessInfo.processInfo.operatingSystemVersionString
        let appVersion = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String

        let request = RegisterRequest(
            deviceToken: tokenString,
            installationId: installationId,
            environment: environment,
            deviceName: deviceName,
            deviceType: deviceType,
            osVersion: osVersion,
            appVersion: appVersion
        )

        var urlRequest = URLRequest(url: baseURL.appendingPathComponent("register"))
        urlRequest.httpMethod = "POST"
        urlRequest.setValue("application/json", forHTTPHeaderField: "Content-Type")
        urlRequest.httpBody = try JSONEncoder().encode(request)

        let (data, response) = try await URLSession.shared.data(for: urlRequest)

        guard let httpResponse = response as? HTTPURLResponse, httpResponse.statusCode == 200 else {
            throw APIError.registrationFailed
        }

        let result = try JSONDecoder().decode(RegisterResponse.self, from: data)
        if !result.success {
            throw APIError.serverError(result.message)
        }
    }

    func fetchPushes() async throws -> [ServerPush] {
        var components = URLComponents(url: baseURL.appendingPathComponent("pushes"), resolvingAgainstBaseURL: false)!
        components.queryItems = [URLQueryItem(name: "installation_id", value: installationId)]

        let (data, response) = try await URLSession.shared.data(from: components.url!)

        guard let httpResponse = response as? HTTPURLResponse, httpResponse.statusCode == 200 else {
            throw APIError.fetchFailed
        }

        return try JSONDecoder().decode(PushesResponse.self, from: data).pushes
    }

    func fetchPushDetail(id: Int64) async throws -> ServerPushDetail {
        let url = baseURL.appendingPathComponent("pushes/\(id)")
        let (data, response) = try await URLSession.shared.data(from: url)

        guard let httpResponse = response as? HTTPURLResponse, httpResponse.statusCode == 200 else {
            throw APIError.fetchFailed
        }

        return try JSONDecoder().decode(ServerPushDetail.self, from: data)
    }

    func sendNotification(_ request: SendRequest) async throws -> SendResponse {
        var urlRequest = URLRequest(url: baseURL.appendingPathComponent("send"))
        urlRequest.httpMethod = "POST"
        urlRequest.setValue("application/json", forHTTPHeaderField: "Content-Type")

        let encoder = JSONEncoder()
        urlRequest.httpBody = try encoder.encode(request)

        let (data, response) = try await URLSession.shared.data(for: urlRequest)

        guard let httpResponse = response as? HTTPURLResponse, httpResponse.statusCode == 200 else {
            throw APIError.sendFailed
        }

        return try JSONDecoder().decode(SendResponse.self, from: data)
    }

    func fetchStats() async throws -> StatsResponse {
        let (data, response) = try await URLSession.shared.data(from: baseURL.appendingPathComponent("stats"))

        guard let httpResponse = response as? HTTPURLResponse, httpResponse.statusCode == 200 else {
            throw APIError.fetchFailed
        }

        return try JSONDecoder().decode(StatsResponse.self, from: data)
    }
}

enum APIError: Error, LocalizedError {
    case registrationFailed
    case fetchFailed
    case sendFailed
    case serverError(String)

    var errorDescription: String? {
        switch self {
        case .registrationFailed:
            return "Failed to register device"
        case .fetchFailed:
            return "Failed to fetch data"
        case .sendFailed:
            return "Failed to send notification"
        case .serverError(let message):
            return message
        }
    }
}
