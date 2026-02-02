//
//  MainTabView.swift
//  psh
//

import SwiftUI

struct MainTabView: View {
    var body: some View {
        TabView {
            ContentView()
                .tabItem {
                    Label("Notifications", systemImage: "bell")
                }

            ComposeView()
                .tabItem {
                    Label("Compose", systemImage: "square.and.pencil")
                }

            StatsView()
                .tabItem {
                    Label("Stats", systemImage: "chart.bar")
                }
        }
    }
}

#Preview {
    MainTabView()
}
