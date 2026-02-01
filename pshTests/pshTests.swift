//
//  pshTests.swift
//  pshTests
//

import Testing
import Foundation
@testable import psh

struct PushNotificationTests {
    @Test func testNotificationCreation() async throws {
        let notification = PushNotification(
            title: "Test Title",
            subtitle: "Test Subtitle",
            body: "Test Body",
            category: "test",
            badge: 5,
            data: "{\"key\": \"value\"}",
            receivedAt: Date()
        )

        #expect(notification.title == "Test Title")
        #expect(notification.subtitle == "Test Subtitle")
        #expect(notification.body == "Test Body")
        #expect(notification.category == "test")
        #expect(notification.badge == 5)
        #expect(notification.data == "{\"key\": \"value\"}")
        #expect(notification.id == nil)
    }

    @Test func testNotificationWithMinimalFields() async throws {
        let notification = PushNotification(receivedAt: Date())

        #expect(notification.title == nil)
        #expect(notification.subtitle == nil)
        #expect(notification.body == nil)
        #expect(notification.category == nil)
        #expect(notification.badge == nil)
        #expect(notification.data == nil)
    }
}

struct APIClientTests {
    @Test func testRegisterRequestEncoding() async throws {
        let request = RegisterRequest(
            deviceToken: "abc123",
            environment: "sandbox",
            deviceName: "Test iPhone",
            deviceType: "iPhone",
            osVersion: "18.0",
            appVersion: "1.0"
        )

        let encoder = JSONEncoder()
        let data = try encoder.encode(request)
        let json = try JSONSerialization.jsonObject(with: data) as! [String: Any]

        #expect(json["device_token"] as? String == "abc123")
        #expect(json["environment"] as? String == "sandbox")
        #expect(json["device_name"] as? String == "Test iPhone")
        #expect(json["device_type"] as? String == "iPhone")
        #expect(json["os_version"] as? String == "18.0")
        #expect(json["app_version"] as? String == "1.0")
    }

    @Test func testRegisterResponseDecoding() async throws {
        let json = """
        {"success": true, "message": "Device registered successfully"}
        """
        let data = json.data(using: .utf8)!
        let response = try JSONDecoder().decode(RegisterResponse.self, from: data)

        #expect(response.success == true)
        #expect(response.message == "Device registered successfully")
    }
}
