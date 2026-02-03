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
                        NavigationLink(value: push) {
                            NotificationRow(push: push)
                        }
                    }
                }
            }
            .navigationTitle("Notifications")
            .navigationDestination(for: ServerPush.self) { push in
                PushDetailView(pushId: push.id)
            }
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
        parseDate(push.sentAt)
    }

    private var hasPayload: Bool {
        guard let payload = push.payload else { return false }
        return !payload.isEmpty && payload != "null"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .top) {
                if let title = push.title {
                    Text(title)
                        .font(.headline)
                        .lineLimit(1)
                }
                Spacer()
                if let date = sentAtDate {
                    Text(date, style: .relative)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            if let body = push.body {
                Text(body)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            if hasPayload {
                Label("Custom data", systemImage: "doc.text")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
        }
        .padding(.vertical, 2)
    }
}

func parseDate(_ string: String) -> Date? {
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    if let date = formatter.date(from: string) {
        return date
    }
    formatter.formatOptions = [.withInternetDateTime]
    if let date = formatter.date(from: string) {
        return date
    }
    let sqliteFormatter = DateFormatter()
    sqliteFormatter.dateFormat = "yyyy-MM-dd HH:mm:ss"
    sqliteFormatter.timeZone = TimeZone(identifier: "UTC")
    return sqliteFormatter.date(from: string)
}

struct PushDetailView: View {
    let pushId: Int64
    @State private var detail: ServerPushDetail?
    @State private var isLoading = true
    @State private var errorMessage: String?

    var body: some View {
        Group {
            if isLoading {
                ProgressView()
            } else if let detail {
                List {
                    if detail.title != nil || detail.body != nil {
                        Section("Content") {
                            if let title = detail.title {
                                LabeledContent("Title", value: title)
                            }
                            if let body = detail.body {
                                LabeledContent("Body", value: body)
                            }
                        }
                    }

                    if let payload = detail.payload,
                       !payload.isEmpty,
                       payload != "null",
                       let data = payload.data(using: .utf8),
                       let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                       !json.isEmpty {
                        Section("Custom Data") {
                            ForEach(json.keys.sorted(), id: \.self) { key in
                                LabeledContent(key) {
                                    Text(stringValue(json[key]))
                                        .foregroundStyle(.secondary)
                                }
                            }
                        }
                    }

                    Section("Delivery") {
                        if let date = parseDate(detail.sentAt) {
                            LabeledContent("Sent") {
                                Text(date, format: .dateTime)
                            }
                        }
                        if let apnsId = detail.apnsId {
                            LabeledContent("APNS ID") {
                                Text(apnsId)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }

                    Section("Device") {
                        if let name = detail.deviceName {
                            LabeledContent("Name", value: name)
                        }
                        if let type = detail.deviceType {
                            LabeledContent("Type", value: type)
                        }
                        if let env = detail.environment {
                            LabeledContent("Environment", value: env.capitalized)
                        }
                    }
                }
            } else if let error = errorMessage {
                ContentUnavailableView(
                    "Error",
                    systemImage: "exclamationmark.triangle",
                    description: Text(error)
                )
            }
        }
        .navigationTitle("Push Details")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .task {
            await loadDetail()
        }
    }

    private func loadDetail() async {
        do {
            detail = try await APIClient.shared.fetchPushDetail(id: pushId)
        } catch {
            errorMessage = error.localizedDescription
        }
        isLoading = false
    }

    private func stringValue(_ value: Any?) -> String {
        guard let value else { return "null" }
        if let string = value as? String { return string }
        if let number = value as? NSNumber { return number.stringValue }
        if let bool = value as? Bool { return bool ? "true" : "false" }
        if let data = try? JSONSerialization.data(withJSONObject: value, options: [.fragmentsAllowed]),
           let string = String(data: data, encoding: .utf8) {
            return string
        }
        return String(describing: value)
    }
}

extension Foundation.Notification.Name {
    static let pushNotificationReceived = Foundation.Notification.Name("pushNotificationReceived")
}

#Preview {
    ContentView()
}
