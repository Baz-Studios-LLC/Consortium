import SwiftUI

struct ChatView: View {
    @EnvironmentObject var store: ConversationStore
    @State private var input: String = ""
    
    var body: some View {
        VStack {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 12) {
                    ForEach(currentMessages) { message in
                        ChatMessageRow(message: message)
                    }
                }
                .padding()
            }
            
            HStack {
                TextField("Type a message…", text: $input)
                    .textFieldStyle(.roundedBorder)
                    .submitLabel(.send)
                    .onSubmit { send() }
                
                Button("Send") { send() }
                    .buttonStyle(.borderedProminent)
            }
            .padding()
        }
        .navigationTitle("Chat")
    }
    
    private var currentConversationIndex: Int? {
        guard let id = store.selectedConversationID else { return nil }
        return store.conversations.firstIndex { $0.id == id }
    }
    
    private var currentMessages: [ChatMessage] {
        if let idx = currentConversationIndex {
            return store.conversations[idx].messages
        }
        return []
    }
    
    private func appendMessage(_ message: ChatMessage) {
        guard let idx = currentConversationIndex else { return }
        var conv = store.conversations[idx]
        conv.messages.append(message)
        store.updateConversation(conv)
    }
    
    private func send() {
        let trimmed = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }

        let userMsg = ChatMessage(role: .user, content: trimmed)
        appendMessage(userMsg)
        input = ""

        Task {
            let history = currentMessages + [userMsg]

            // Prepare services based on available API keys
            var services: [AIModelService] = []
            if let key = APIKeyManager.shared.load(for: .openAI), !key.isEmpty { services.append(OpenAIService(apiKey: key)) }
            if let key = APIKeyManager.shared.load(for: .claude), !key.isEmpty { services.append(ClaudeService(apiKey: key)) }
            if let key = APIKeyManager.shared.load(for: .gemini), !key.isEmpty { services.append(GeminiService(apiKey: key)) }
            if let key = APIKeyManager.shared.load(for: .grok), !key.isEmpty { services.append(GrokService(apiKey: key)) }

            // Call each service sequentially
            for service in services {
                if let reply = await service.generateResponse(history: history, userMessage: trimmed) {
                    await MainActor.run { appendMessage(reply) }
                }
            }
        }
    }
}

#Preview {
    ChatView()
        .environmentObject(ConversationStore())
}
