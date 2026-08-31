import SwiftUI

struct ChatView: View {
    @EnvironmentObject var settings: SettingsStore
    @EnvironmentObject var chatVM: ChatViewModel

    var body: some View {
        VStack(spacing: 0) {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 12) {
                    ForEach(chatVM.messages) { msg in
                        MessageRowView(message: msg, displayName: settings.displayName)
                    }
                }
                .padding()
            }

            Divider()

            HStack {
                TextField("Address the consortium…", text: $chatVM.currentInput, axis: .vertical)
                    .textFieldStyle(.roundedBorder)
                    .lineLimit(1...5)
                Button("Send") {
                    chatVM.sendUserMessage(displayName: settings.displayName)
                }
                .buttonStyle(.borderedProminent)
                .disabled(chatVM.currentInput.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            .padding()
        }
    }
}

