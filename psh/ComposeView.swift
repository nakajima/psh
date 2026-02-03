//
//  ComposeView.swift
//  psh
//

import SwiftUI

struct ComposeView: View {
    // Alert section
    @State private var title = ""
    @State private var subtitle = ""
    @State private var bodyText = ""

    // Sound section
    @State private var soundEnabled = false
    @State private var soundName = "default"
    @State private var isCritical = false
    @State private var criticalVolume: Double = 1.0

    // Behavior section
    @State private var badgeText = ""
    @State private var contentAvailable = false
    @State private var mutableContent = false
    @State private var category = ""
    @State private var interruptionLevel = "active"
    @State private var useRelevanceScore = false
    @State private var relevanceScore: Double = 0.5

    // Delivery section
    @State private var priorityText = ""
    @State private var collapseId = ""
    @State private var expirationText = ""

    // Custom data
    @State private var customData: [(key: String, value: String)] = []

    // UI state
    @State private var isSending = false
    @State private var showingResult = false
    @State private var resultMessage = ""
    @State private var resultSuccess = false

    var body: some View {
        NavigationStack {
            Form {
                Section("Alert") {
                    TextField("Title", text: $title)
                    TextField("Subtitle", text: $subtitle)
                    TextField("Body", text: $bodyText, axis: .vertical)
                        .lineLimit(3...6)
                }

                Section("Sound") {
                    Toggle("Enable Sound", isOn: $soundEnabled)
                    if soundEnabled {
                        TextField("Sound Name", text: $soundName)
                        Toggle("Critical Alert", isOn: $isCritical)
                        if isCritical {
                            VStack(alignment: .leading) {
                                Text("Volume: \(criticalVolume, specifier: "%.1f")")
                                Slider(value: $criticalVolume, in: 0...1, step: 0.1)
                            }
                        }
                    }
                }

                Section("Behavior") {
                    TextField("Badge (number)", text: $badgeText)
                        #if os(iOS)
                        .keyboardType(.numberPad)
                        #endif
                    Toggle("Content Available", isOn: $contentAvailable)
                    Toggle("Mutable Content", isOn: $mutableContent)
                    TextField("Category", text: $category)
                    Picker("Interruption Level", selection: $interruptionLevel) {
                        Text("Passive").tag("passive")
                        Text("Active").tag("active")
                        Text("Time Sensitive").tag("time-sensitive")
                        Text("Critical").tag("critical")
                    }
                    Toggle("Relevance Score", isOn: $useRelevanceScore)
                    if useRelevanceScore {
                        VStack(alignment: .leading) {
                            Text("Score: \(relevanceScore, specifier: "%.2f")")
                            Slider(value: $relevanceScore, in: 0...1, step: 0.05)
                        }
                    }
                }

                Section("Delivery") {
                    TextField("Priority (1-10)", text: $priorityText)
                        #if os(iOS)
                        .keyboardType(.numberPad)
                        #endif
                    TextField("Collapse ID", text: $collapseId)
                    TextField("Expiration (seconds)", text: $expirationText)
                        #if os(iOS)
                        .keyboardType(.numberPad)
                        #endif
                }

                Section {
                    ForEach(customData.indices, id: \.self) { index in
                        HStack {
                            TextField("Key", text: Binding(
                                get: { customData[index].key },
                                set: { customData[index].key = $0 }
                            ))
                            TextField("Value", text: Binding(
                                get: { customData[index].value },
                                set: { customData[index].value = $0 }
                            ))
                            Button(role: .destructive) {
                                customData.remove(at: index)
                            } label: {
                                Image(systemName: "minus.circle.fill")
                            }
                            .buttonStyle(.borderless)
                        }
                    }
                    Button {
                        customData.append((key: "", value: ""))
                    } label: {
                        Label("Add Custom Field", systemImage: "plus.circle")
                    }
                } header: {
                    Text("Custom Data")
                }

                Section {
                    Button {
                        Task { await send() }
                    } label: {
                        HStack {
                            Spacer()
                            if isSending {
                                ProgressView()
                            } else {
                                Text("Send Notification")
                            }
                            Spacer()
                        }
                    }
                    .disabled(isSending || (title.isEmpty && bodyText.isEmpty))
                }
            }
            .formStyle(.grouped)
            .navigationTitle("Compose")
            #if os(macOS)
            .contentMargins(.horizontal, 80, for: .scrollContent)
            #endif
            .alert(resultSuccess ? "Success" : "Error", isPresented: $showingResult) {
                Button("OK", role: .cancel) {}
            } message: {
                Text(resultMessage)
            }
        }
    }

    private func send() async {
        isSending = true
        defer { isSending = false }

        var request = SendRequest()

        if !title.isEmpty { request.title = title }
        if !subtitle.isEmpty { request.subtitle = subtitle }
        if !bodyText.isEmpty { request.body = bodyText }

        if let badge = Int(badgeText), badge >= 0 {
            request.badge = badge
        }

        if soundEnabled {
            if isCritical {
                request.sound = .critical(name: soundName, volume: criticalVolume)
            } else {
                request.sound = .simple(soundName)
            }
        }

        if contentAvailable { request.contentAvailable = true }
        if mutableContent { request.mutableContent = true }
        if !category.isEmpty { request.category = category }

        if let priority = Int(priorityText), priority >= 1, priority <= 10 {
            request.priority = priority
        }

        if !collapseId.isEmpty { request.collapseId = collapseId }
        if interruptionLevel != "active" { request.interruptionLevel = interruptionLevel }
        if useRelevanceScore { request.relevanceScore = relevanceScore }

        if let expiration = Int(expirationText), expiration > 0 {
            request.expiration = expiration
        }

        let dataDict = customData.filter { !$0.key.isEmpty && !$0.value.isEmpty }
        if !dataDict.isEmpty {
            request.data = Dictionary(uniqueKeysWithValues: dataDict.map { ($0.key, $0.value) })
        }

        do {
            let response = try await APIClient.shared.sendNotification(request)
            resultSuccess = response.success
            resultMessage = "Sent: \(response.sent), Failed: \(response.failed)"
            showingResult = true

            if response.success {
                clearForm()
            }
        } catch {
            resultSuccess = false
            resultMessage = error.localizedDescription
            showingResult = true
        }
    }

    private func clearForm() {
        title = ""
        subtitle = ""
        bodyText = ""
        soundEnabled = false
        soundName = "default"
        isCritical = false
        criticalVolume = 1.0
        badgeText = ""
        contentAvailable = false
        mutableContent = false
        category = ""
        priorityText = ""
        collapseId = ""
        expirationText = ""
        interruptionLevel = "active"
        useRelevanceScore = false
        relevanceScore = 0.5
        customData = []
    }
}

#Preview {
    ComposeView()
}
