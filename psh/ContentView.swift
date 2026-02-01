//
//  ContentView.swift
//  psh
//

import SwiftUI

struct ContentView: View {
    @State private var pushes: [ServerPush] = []
    @State private var searchText = ""
    @State private var errorMessage: String?

    private var filteredPushes: [ServerPush] {
        if searchText.isEmpty {
            return pushes
        }
        return pushes.filter { push in
            (push.title?.localizedCaseInsensitiveContains(searchText) ?? false) ||
            (push.body?.localizedCaseInsensitiveContains(searchText) ?? false)
        }
    }

    var body: some View {
        NavigationStack {
            Group {
                if filteredPushes.isEmpty && searchText.isEmpty {
                    ContentUnavailableView(
                        "No Notifications",
                        systemImage: "bell.slash",
                        description: Text("Push notifications you receive will appear here.")
                    )
                } else if filteredPushes.isEmpty {
                    ContentUnavailableView.search(text: searchText)
                } else {
                    List(filteredPushes) { push in
                        NotificationRow(push: push)
                    }
                }
            }
            .navigationTitle("Notifications")
            .searchable(text: $searchText, prompt: "Search notifications")
            .task {
                await fetchPushes()
            }
            .onReceive(NotificationCenter.default.publisher(for: .pushNotificationReceived)) { _ in
                Task {
                    await fetchPushes()
                }
            }
            .refreshable {
                await fetchPushes()
            }
        }
    }

    private func fetchPushes() async {
        do {
            pushes = try await APIClient.shared.fetchPushes()
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}

struct NotificationRow: View {
    let push: ServerPush

    private var sentAtDate: Date? {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = formatter.date(from: push.sentAt) {
            return date
        }
        // Try without fractional seconds
        formatter.formatOptions = [.withInternetDateTime]
        if let date = formatter.date(from: push.sentAt) {
            return date
        }
        // Try SQLite default format
        let sqliteFormatter = DateFormatter()
        sqliteFormatter.dateFormat = "yyyy-MM-dd HH:mm:ss"
        sqliteFormatter.timeZone = TimeZone(identifier: "UTC")
        return sqliteFormatter.date(from: push.sentAt)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            if let title = push.title {
                Text(title)
                    .font(.headline)
            }
            if let body = push.body {
                Text(body)
                    .font(.body)
                    .foregroundStyle(push.title == nil ? .primary : .secondary)
            }
            HStack {
                Spacer()
                if let date = sentAtDate {
                    Text(date, style: .relative)
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                } else {
                    Text(push.sentAt)
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }
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
