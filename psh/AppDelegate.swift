//
//  AppDelegate.swift
//  psh
//

#if canImport(UIKit)
import UIKit
import UserNotifications

final class AppDelegate: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        UNUserNotificationCenter.current().delegate = self
        registerForPushNotifications()
        return true
    }

    func registerForPushNotifications() {
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge, .timeSensitive, .criticalAlert]) { granted, error in
            if let error {
                print("Push notification authorization error: \(error)")
                return
            }
            guard granted else {
                print("Push notifications not authorized")
                return
            }
            Task { @MainActor in
                UIApplication.shared.registerForRemoteNotifications()
            }
        }
    }

    func application(
        _ application: UIApplication,
        didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data
    ) {
        Task {
            do {
                try await APIClient.shared.register(deviceToken: deviceToken)
                print("Device registered successfully")
            } catch {
                print("Failed to register device: \(error)")
            }
        }
    }

    func application(
        _ application: UIApplication,
        didFailToRegisterForRemoteNotificationsWithError error: Error
    ) {
        print("Failed to register for remote notifications: \(error)")
    }

    func application(
        _ application: UIApplication,
        didReceiveRemoteNotification userInfo: [AnyHashable: Any],
        fetchCompletionHandler completionHandler: @escaping (UIBackgroundFetchResult) -> Void
    ) {
        saveNotification(from: userInfo)
        completionHandler(.newData)
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification
    ) async -> UNNotificationPresentationOptions {
        saveNotification(from: notification.request.content.userInfo)
        return [.banner, .sound, .badge]
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse
    ) async {
        saveNotification(from: response.notification.request.content.userInfo)
    }

    private func saveNotification(from userInfo: [AnyHashable: Any]) {
        let aps = userInfo["aps"] as? [String: Any]
        let alert = aps?["alert"] as? [String: Any]

        var title: String?
        var subtitle: String?
        var body: String?

        if let alertDict = alert {
            title = alertDict["title"] as? String
            subtitle = alertDict["subtitle"] as? String
            body = alertDict["body"] as? String
        } else if let alertString = aps?["alert"] as? String {
            body = alertString
        }

        let category = aps?["category"] as? String
        let badge = aps?["badge"] as? Int

        var customData: String?
        var filteredUserInfo = userInfo
        filteredUserInfo.removeValue(forKey: "aps")
        if !filteredUserInfo.isEmpty,
           let jsonData = try? JSONSerialization.data(withJSONObject: filteredUserInfo),
           let jsonString = String(data: jsonData, encoding: .utf8) {
            customData = jsonString
        }

        var pushNotification = PushNotification(
            title: title,
            subtitle: subtitle,
            body: body,
            category: category,
            badge: badge,
            data: customData,
            receivedAt: Date()
        )

        do {
            try AppDatabase.shared.save(&pushNotification)
            NotificationCenter.default.post(name: .pushNotificationReceived, object: nil)
        } catch {
            print("Failed to save notification: \(error)")
        }
    }
}

#elseif os(macOS)
import AppKit
import UserNotifications

final class AppDelegate: NSObject, NSApplicationDelegate, UNUserNotificationCenterDelegate {
    func applicationDidFinishLaunching(_ notification: Foundation.Notification) {
        UNUserNotificationCenter.current().delegate = self
        registerForPushNotifications()
    }

    func registerForPushNotifications() {
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge, .timeSensitive, .criticalAlert]) { granted, error in
            if let error {
                print("Push notification authorization error: \(error)")
                return
            }
            guard granted else {
                print("Push notifications not authorized")
                return
            }
            Task { @MainActor in
                NSApplication.shared.registerForRemoteNotifications()
            }
        }
    }

    func application(_ application: NSApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        Task {
            do {
                try await APIClient.shared.register(deviceToken: deviceToken)
                print("Device registered successfully")
            } catch {
                print("Failed to register device: \(error)")
            }
        }
    }

    func application(_ application: NSApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {
        print("Failed to register for remote notifications: \(error)")
    }

    func application(_ application: NSApplication, didReceiveRemoteNotification userInfo: [String: Any]) {
        saveNotification(from: userInfo)
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification
    ) async -> UNNotificationPresentationOptions {
        saveNotification(from: notification.request.content.userInfo)
        return [.banner, .sound, .badge]
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse
    ) async {
        saveNotification(from: response.notification.request.content.userInfo)
    }

    private func saveNotification(from userInfo: [AnyHashable: Any]) {
        let aps = userInfo["aps"] as? [String: Any]
        let alert = aps?["alert"] as? [String: Any]

        var title: String?
        var subtitle: String?
        var body: String?

        if let alertDict = alert {
            title = alertDict["title"] as? String
            subtitle = alertDict["subtitle"] as? String
            body = alertDict["body"] as? String
        } else if let alertString = aps?["alert"] as? String {
            body = alertString
        }

        let category = aps?["category"] as? String
        let badge = aps?["badge"] as? Int

        var customData: String?
        var filteredUserInfo = userInfo
        filteredUserInfo.removeValue(forKey: "aps")
        if !filteredUserInfo.isEmpty,
           let jsonData = try? JSONSerialization.data(withJSONObject: filteredUserInfo),
           let jsonString = String(data: jsonData, encoding: .utf8) {
            customData = jsonString
        }

        var pushNotification = PushNotification(
            title: title,
            subtitle: subtitle,
            body: body,
            category: category,
            badge: badge,
            data: customData,
            receivedAt: Date()
        )

        do {
            try AppDatabase.shared.save(&pushNotification)
            NotificationCenter.default.post(name: .pushNotificationReceived, object: nil)
        } catch {
            print("Failed to save notification: \(error)")
        }
    }
}
#endif
