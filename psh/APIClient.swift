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
    let environment: String
    let deviceName: String?
    let deviceType: String?
    let osVersion: String?
    let appVersion: String?

    enum CodingKeys: String, CodingKey {
        case deviceToken = "device_token"
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

struct ServerPush: Decodable, Identifiable {
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

final class APIClient: Sendable {
    static let shared = APIClient()

    private static let deviceTokenKey = "deviceToken"

    let baseURL: URL

    init(baseURL: URL = URL(string: "https://psh.fishmt.net")!) {
        self.baseURL = baseURL
    }

    var storedDeviceToken: String? {
        UserDefaults.standard.string(forKey: Self.deviceTokenKey)
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
        guard let deviceToken = storedDeviceToken else {
            throw APIError.noDeviceToken
        }

        var components = URLComponents(url: baseURL.appendingPathComponent("pushes"), resolvingAgainstBaseURL: false)!
        components.queryItems = [URLQueryItem(name: "device_token", value: deviceToken)]

        let (data, response) = try await URLSession.shared.data(from: components.url!)

        guard let httpResponse = response as? HTTPURLResponse, httpResponse.statusCode == 200 else {
            throw APIError.fetchFailed
        }

        return try JSONDecoder().decode(PushesResponse.self, from: data).pushes
    }
}

enum APIError: Error, LocalizedError {
    case registrationFailed
    case fetchFailed
    case noDeviceToken
    case serverError(String)

    var errorDescription: String? {
        switch self {
        case .registrationFailed:
            return "Failed to register device"
        case .fetchFailed:
            return "Failed to fetch pushes"
        case .noDeviceToken:
            return "Device not registered"
        case .serverError(let message):
            return message
        }
    }
}
