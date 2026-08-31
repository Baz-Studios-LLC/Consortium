import SwiftUI

struct ContentView: View {
    @StateObject private var store = ConversationStore()
    @State private var sidebarOpen = false

    @State private var localEnabledModels: Set<String> = [
        "chatgpt", "gemini", "grok", "claude"
    ]
    @State private var showingSettings = false

    // OpenAI model identifiers
    private let openAIModelIdentifier = "gpt-5-mini"
    private let openAIModelSnapshotIdentifier = "gpt-5-mini-2025-08-07" // pinned snapshot (optional)

    // Resolve a model identifier for a given provider key used in the toolbar
    private func modelIdentifier(for key: String) -> String? {
        switch key {
        case "chatgpt":
            // Default to moving alias; use snapshot for reproducibility if desired
            return openAIModelIdentifier
            // return openAIModelSnapshotIdentifier
        case "gemini":
            return nil // TODO: plug in Gemini model tag when ready
        case "grok":
            return nil // TODO: plug in Grok model tag when ready
        case "claude":
            return nil // TODO: plug in Claude model tag when ready
        default:
            return nil
        }
    }

    // Indicates whether ChatGPT (OpenAI) is enabled in the toolbar
    private var isChatGPTEnabled: Bool {
        localEnabledModels.contains("chatgpt")
    }

    // Returns the current OpenAI model tag to use when sending messages
    private var currentOpenAIModelTag: String {
        modelIdentifier(for: "chatgpt") ?? openAIModelIdentifier
    }

    private let sidebarWidth: CGFloat = 260
    private let modelButtonWidth: CGFloat = 105

    var body: some View {
        ZStack {
            mainArea
            sidebarView
        }
        .toolbar {
            ToolbarItemGroup(placement: .navigation) {

                Button {
                    withAnimation { sidebarOpen.toggle() }
                } label: { Image(systemName: "sidebar.leading").font(.title2) }

                Button {
                    store.createConversation()
                } label: { Image(systemName: "square.and.pencil").font(.title2) }
            }

            ToolbarItem(placement: .principal) {
                modelsToolbar
                    .padding(.horizontal, 12)
                    .toolbarBackground(.hidden, for: .automatic)
                    .toolbarBackground(.hidden, for: .windowToolbar)
            }
        }
        .toolbarBackground(.hidden, for: .windowToolbar)
        .toolbarRole(.editor)
        .sheet(isPresented: $showingSettings) {
            InlineSettingsSheet()
        }
        .environmentObject(store)
    }

    // MARK: Main Chat Area

    private var mainArea: some View {
        NavigationStack {
            if let selected = store.selectedConversationID,
               let _ = store.conversations.first(where: { $0.id == selected }) {
                ChatView()
            } else {
                Text("Select or create a conversation")
                    .foregroundColor(.secondary)
                    .padding()
            }
        }
        .disabled(sidebarOpen)
    }

    // MARK: Sidebar

    private var sidebarView: some View {
        HStack(spacing: 0) {
            VStack(alignment: .leading) {

                Button {
                    store.createConversation()
                } label: {
                    Label("New Chat", systemImage: "plus")
                        .font(.headline)
                }
                .padding(.bottom, 10)

                List(selection: $store.selectedConversationID) {
                    ForEach(store.conversations) { conversation in
                        conversationRow(conversation)
                            .tag(conversation.id)
                    }
                    .onDelete(perform: store.deleteConversation)
                }
                .listStyle(.inset)

                Button {
                    showingSettings = true
                } label: {
                    Label("Settings", systemImage: "gear")
                        .font(.headline)
                }
                .padding(.top, 8)

                Spacer()
            }
            .padding()
            .frame(width: sidebarWidth)
            .background(.ultraThickMaterial)
            .offset(x: sidebarOpen ? 0 : -sidebarWidth)
            .animation(.easeInOut, value: sidebarOpen)

            Spacer()
        }
    }

    private func conversationRow(_ conversation: Conversation) -> some View {
        HStack {
            Image(systemName: "message.fill")
            Text(conversation.title)
        }
        .padding(6)
        .background(
            store.selectedConversationID == conversation.id ?
                Color.accentColor.opacity(0.2) : .clear
        )
        .cornerRadius(6)
        .contextMenu {
            Button(conversation.isPinned ? "Unpin" : "Pin") {
                store.togglePin(conversation)
            }

            Button("Rename") {
                store.beginRename(conversation)
            }

            Divider()

            Button(role: .destructive) {
                if let idx = store.conversations.firstIndex(where: { $0.id == conversation.id }) {
                    store.deleteConversation(at: IndexSet(integer: idx))
                }
            } label: {
                Text("Delete")
            }
        }
    }

    // MARK: Model Toggle Toolbar

    private var modelsToolbar: some View {

        let availableModels: [ModelOption] = [
            ModelOption(key: "chatgpt", name: "ChatGPT"),
            ModelOption(key: "gemini", name: "Gemini"),
            ModelOption(key: "grok",   name: "Grok"),
            ModelOption(key: "claude", name: "Claude")
        ]

        // Example: retrieve the OpenAI model tag when "chatgpt" is enabled
        // let currentOpenAIModel = modelIdentifier(for: "chatgpt") // -> "gpt-5-mini"

        // Example integration in your send flow (inside ConversationStore or ChatView):
        // if isChatGPTEnabled {
        //     let service = OpenAIChatService()
        //     let messages = /* map your conversation to [OpenAIChatService.Message] */
        //     Task { let reply = try? await service.send(messages: messages, model: currentOpenAIModelTag) }
        // }

        return HStack(spacing: 10) {
            ForEach(availableModels, id: \.key) { model in
                let isOn = self.localEnabledModels.contains(model.key)

                Button {
                    withAnimation(.easeInOut(duration: 0.15)) {
                        if isOn { self.localEnabledModels.remove(model.key) }
                        else { self.localEnabledModels.insert(model.key) }
                    }
                } label: {
                    HStack(spacing: 6) {
                        Image(model.key)
                            .resizable()
                            .renderingMode(.template)
                            .foregroundColor(isOn ? Color.accentColor : Color.primary.opacity(0.6))
                            .frame(width: 14, height: 14)

                        Text(model.name)
                            .font(.caption)
                            .fontWeight(isOn ? .semibold : .regular)

                        if isOn {
                            Image(systemName: "checkmark.circle.fill")
                                .font(.system(size: 12))
                                .foregroundStyle(Color.accentColor)
                        }
                    }
                    .frame(width: modelButtonWidth)
                    .padding(.vertical, 6)
                    .background(
                        RoundedRectangle(cornerRadius: 10)
                            .fill(.secondary.opacity(isOn ? 0.2 : 0.1))
                    )
                }
                .buttonStyle(.plain)
            }
        }
    }
}

struct ModelOption: Identifiable {
    let id = UUID()
    let key: String
    let name: String
}

private struct InlineSettingsSheet: View {
    @Environment(\.dismiss) private var dismiss
    @State private var openAIKey: String = APIKeyManager.shared.load(for: .openAI) ?? ""
    @State private var claudeKey: String = APIKeyManager.shared.load(for: .claude) ?? ""
    @State private var geminiKey: String = APIKeyManager.shared.load(for: .gemini) ?? ""
    @State private var grokKey: String = APIKeyManager.shared.load(for: .grok) ?? ""

    var body: some View {
        NavigationStack {
            Form {
                Section(header: Text("API Keys")) {
                    SecureField("OpenAI Key", text: $openAIKey)
                    SecureField("Claude Key", text: $claudeKey)
                    SecureField("Gemini Key", text: $geminiKey)
                    SecureField("Grok Key", text: $grokKey)
                }

                Section {
                    Button("Save") {
                        APIKeyManager.shared.save(key: openAIKey, for: .openAI)
                        APIKeyManager.shared.save(key: claudeKey, for: .claude)
                        APIKeyManager.shared.save(key: geminiKey, for: .gemini)
                        APIKeyManager.shared.save(key: grokKey, for: .grok)
                        dismiss()
                    }
                    Button(role: .destructive) {
                        APIKeyManager.shared.resetAll()
                        openAIKey = ""; claudeKey = ""; geminiKey = ""; grokKey = ""
                    } label: {
                        Text("Reset All Keys")
                    }
                }
            }
            .navigationTitle("Settings")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close") { dismiss() }
                }
            }
            .padding()
        }
    }
}

#Preview { ContentView() }
