import SwiftUI

struct SidebarView: View {
    @EnvironmentObject var settings: SettingsStore
    @EnvironmentObject var sidebarVM: SidebarViewModel
    @EnvironmentObject var apiKeysVM: APIKeysViewModel

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            // Logo / title placeholder
            VStack {
                Text("🤖")
                    .font(.system(size: 40))
                Text("Consortium")
                    .font(.title2)
                    .bold()
            }
            .frame(maxWidth: .infinity)
            .padding(.bottom, 8)

            Divider()

            // Display name
            VStack(alignment: .leading, spacing: 4) {
                Text("Display Name").font(.headline)
                TextField("Display Name", text: $settings.displayName)
            }

            Divider()

            // Active panelists
            VStack(alignment: .leading, spacing: 4) {
                Text("Active Panelists").font(.headline)
                Toggle("ChatGPT", isOn: $sidebarVM.enableChatGPT)
                Toggle("Gemini",  isOn: $sidebarVM.enableGemini)
                Toggle("Grok",    isOn: $sidebarVM.enableGrok)
                Toggle("Claude",  isOn: $sidebarVM.enableClaude)
            }

            Divider()

            // Auto mode
            VStack(alignment: .leading, spacing: 4) {
                Text("Auto Mode").font(.headline)
                Toggle("Enable Auto Mode", isOn: $settings.autoModeEnabled)
            }

            Divider()

            // API keys
            VStack(alignment: .leading, spacing: 4) {
                Text("API Keys").font(.headline)
                Button("Set API Keys") {
                    apiKeysVM.load(from: settings)
                    apiKeysVM.isPresenting = true
                }
                .buttonStyle(.borderedProminent)
            }

            Spacer()
        }
        .padding()
    }
}