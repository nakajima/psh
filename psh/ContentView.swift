//
//  ContentView.swift
//  psh
//
//  Created by Pat Nakajima on 1/27/26.
//

import SwiftUI

struct ContentView: View {
    @State private var notifications: [PushNotification] = []
    @State private var searchText = ""
    @State private var errorMessage: String?

    var body: some View {
        NavigationStack {
            Group {
                if notifications.isEmpty && searchText.isEmpty {
                    ContentUnavailableView(
                        "No Notifications",
                        systemImage: "bell.slash",
                        description: Text("Push notifications you receive will appear here.")
                    )
                } else if notifications.isEmpty {
                    ContentUnavailableView.search(text: searchText)
                } else {
                    List(notifications) { notification in
                        NotificationRow(notification: notification)
                    }
                }
            }
            .navigationTitle("Notifications")
            .searchable(text: $searchText, prompt: "Search notifications")
            .onChange(of: searchText) {
                search()
            }
            .onAppear {
                loadNotifications()
            }
            .onReceive(NotificationCenter.default.publisher(for: .pushNotificationReceived)) { _ in
                loadNotifications()
            }
            .refreshable {
                loadNotifications()
            }
        }
    }

    private func loadNotifications() {
        do {
            if searchText.isEmpty {
                notifications = try AppDatabase.shared.allNotifications()
            } else {
                notifications = try AppDatabase.shared.search(searchText)
            }
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func search() {
        do {
            notifications = try AppDatabase.shared.search(searchText)
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}

struct NotificationRow: View {
    let notification: PushNotification

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            if let title = notification.title {
                Text(title)
                    .font(.headline)
            }
            if let subtitle = notification.subtitle {
                Text(subtitle)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            if let body = notification.body {
                Text(body)
                    .font(.body)
                    .foregroundStyle(notification.title == nil ? .primary : .secondary)
            }
            HStack {
                if let category = notification.category {
                    Text(category)
                        .font(.caption)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Color.accentColor.opacity(0.2))
                        .clipShape(Capsule())
                }
                if let badge = notification.badge {
                    Label("\(badge)", systemImage: "app.badge")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Text(notification.receivedAt, style: .relative)
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
        }
        .padding(.vertical, 4)
    }
}

extension Foundation.Notification.Name {
    static let pushNotificationReceived = Foundation.Notification.Name("pushNotificationReceived")
}

#Preview {
    ContentView()
}
