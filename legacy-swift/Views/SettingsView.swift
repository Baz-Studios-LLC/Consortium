import SwiftUI

struct SettingsView: View {
    @EnvironmentObject var settings: AppSettings
    @EnvironmentObject var apiKeys: APIKeysViewModel

    var body: some View {
        Form {
            Section(header: Text("Active AI Models")) {
                Toggle("ChatGPT", isOn: $settings.enableChatGPT)
                Toggle("Claude", isOn: $settings.enableClaude)
                Toggle("Gemini", isOn: $settings.enableGemini)
                Toggle("Grok", isOn: $settings.enableGrok)
            }

            Section(header: Text("API Keys")) {
                SecureField("OpenAI Key", text: $apiKeys.openAIKey)
                SecureField("Claude Key", text: $apiKeys.claudeKey)
                SecureField("Gemini Key", text: $apiKeys.googleKey)
                SecureField("Grok Key", text: $apiKeys.grokKey)
                Button("Save Keys") { apiKeys.save() }
                    .disabled(!apiKeys.hasChanges)
            }

            Section(header: Text("Conversation Settings")) {
                Toggle("Auto Discussion Mode", isOn: $settings.autoDiscussionEnabled)
                Stepper("Auto-turn count: \(settings.autoTurnCount)", value: $settings.autoTurnCount, in: 1...10)
            }

            Section(header: Text("Appearance")) {
                Toggle("Show Timestamps", isOn: $settings.showTimestamps)
                Toggle("Compact Chat Style", isOn: $settings.compactMode)
                Toggle("Dark Mode Override", isOn: $settings.darkModeEnabled)
            }

            Section {
                Button(role: .destructive) {
                    settings.resetToDefaults()
                    apiKeys.reset()
                } label: {
                    Text("Reset All Settings")
                }
            }
        }
        .navigationTitle("Settings")
        .padding()
    }
}

#Preview {
    SettingsView()
        .environmentObject(AppSettings())
        .environmentObject(APIKeysViewModel())
}
