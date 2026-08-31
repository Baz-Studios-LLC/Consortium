import SwiftUI

struct APIKeysSheetView: View {
    @EnvironmentObject var settings: SettingsStore
    @EnvironmentObject var apiKeysVM: APIKeysViewModel

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("API Keys").font(.title2).bold()

            SecureField("OpenAI API Key", text: $apiKeysVM.openAIKey)
            SecureField("Google (Gemini) API Key", text: $apiKeysVM.geminiKey)
            SecureField("xAI (Grok) API Key", text: $apiKeysVM.grokKey)
            SecureField("Anthropic (Claude) API Key", text: $apiKeysVM.claudeKey)

            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                Button("Save") {
                    apiKeysVM.apply(to: settings)
                    dismiss()
                }
                .buttonStyle(.borderedProminent)
            }
        }
        .padding()
    }
}