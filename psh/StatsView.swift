//
//  StatsView.swift
//  psh
//

import SwiftUI

struct StatsView: View {
    @State private var stats: StatsResponse?
    @State private var isLoading = false
    @State private var errorMessage: String?

    var body: some View {
        NavigationStack {
            Group {
                if isLoading && stats == nil {
                    ProgressView("Loading stats...")
                } else if let stats = stats {
                    List {
                        Section("Devices") {
                            StatRow(label: "Total Devices", value: stats.totalDevices)
                            StatRow(label: "Sandbox", value: stats.sandboxDevices)
                            StatRow(label: "Production", value: stats.productionDevices)
                        }
                        Section("Pushes") {
                            StatRow(label: "Total Sent", value: stats.totalPushes)
                        }
                    }
                } else if let error = errorMessage {
                    ContentUnavailableView(
                        "Failed to Load",
                        systemImage: "exclamationmark.triangle",
                        description: Text(error)
                    )
                } else {
                    ContentUnavailableView(
                        "No Stats",
                        systemImage: "chart.bar",
                        description: Text("Pull to refresh")
                    )
                }
            }
            .navigationTitle("Stats")
            .task {
                await fetchStats()
            }
            .refreshable {
                await fetchStats()
            }
        }
    }

    private func fetchStats() async {
        isLoading = true
        defer { isLoading = false }

        do {
            stats = try await APIClient.shared.fetchStats()
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}

struct StatRow: View {
    let label: String
    let value: Int

    var body: some View {
        HStack {
            Text(label)
            Spacer()
            Text("\(value)")
                .foregroundStyle(.secondary)
        }
    }
}

#Preview {
    StatsView()
}
