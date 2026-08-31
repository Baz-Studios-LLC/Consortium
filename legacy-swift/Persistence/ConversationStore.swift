import SwiftUI
import Foundation
import Combine

@MainActor
class ConversationStore: ObservableObject {
    @Published var conversations: [Conversation] = []
    @Published var selectedConversationID: UUID?
    @Published var renamingConversation: Conversation?
    @Published var isRenaming: Bool = false
    @Published var lastSelectedConversationID: UUID? {
        didSet { persistSelectedConversationID() }
    }

    private let saveKey = "stored_conversations"
    private let selectedKey = "selected_conversation_id"

    init() {
        load()
    }

    func createConversation() {
        let newConv = Conversation()
        conversations.insert(newConv, at: 0)
        sortConversations()
        selectedConversationID = newConv.id
        lastSelectedConversationID = newConv.id
        save()
    }

    func togglePin(_ conversation: Conversation) {
        guard let index = conversations.firstIndex(where: { $0.id == conversation.id }) else { return }
        conversations[index].isPinned.toggle()
        sortConversations()
        save()
    }

    private func sortConversations() {
        conversations.sort { lhs, rhs in
            if lhs.isPinned != rhs.isPinned {
                return lhs.isPinned && !rhs.isPinned
            }
            return lhs.title.localizedCaseInsensitiveCompare(rhs.title) == .orderedAscending
        }
    }

    func updateConversation(_ conversation: Conversation) {
        guard let index = conversations.firstIndex(where: { $0.id == conversation.id }) else { return }
        conversations[index] = conversation
        save()
    }

    func deleteConversation(at offsets: IndexSet) {
        // Capture IDs of conversations to delete
        let idsToDelete = offsets.compactMap { index in
            conversations.indices.contains(index) ? conversations[index].id : nil
        }

        conversations.remove(atOffsets: offsets)

        // If the selected conversation was deleted, update selection
        if let selectedID = selectedConversationID, idsToDelete.contains(selectedID) {
            // Prefer the first conversation if available, otherwise clear selection
            selectedConversationID = conversations.first?.id
            lastSelectedConversationID = selectedConversationID
        }

        save()
    }
    
    func beginRename(_ conversation: Conversation) {
        renamingConversation = conversation
        isRenaming = true
    }
    
    func commitRename(to newTitle: String) {
        guard let conversation = renamingConversation,
              let index = conversations.firstIndex(where: { $0.id == conversation.id }) else {
            isRenaming = false
            renamingConversation = nil
            return
        }
        conversations[index].title = newTitle
        sortConversations()
        save()
        isRenaming = false
        renamingConversation = nil
    }
    
    func cancelRenaming() {
        isRenaming = false
        renamingConversation = nil
    }

    private func save() {
        if let encoded = try? JSONEncoder().encode(conversations) {
            UserDefaults.standard.set(encoded, forKey: saveKey)
        }
    }
    
    private func persistSelectedConversationID() {
        if let id = lastSelectedConversationID {
            UserDefaults.standard.set(id.uuidString, forKey: selectedKey)
        } else {
            UserDefaults.standard.removeObject(forKey: selectedKey)
        }
    }

    private func restoreSelectedConversationID() {
        if let idString = UserDefaults.standard.string(forKey: selectedKey),
           let id = UUID(uuidString: idString),
           conversations.contains(where: { $0.id == id }) {
            selectedConversationID = id
            lastSelectedConversationID = id
        } else {
            selectedConversationID = conversations.first?.id
            lastSelectedConversationID = selectedConversationID
        }
    }

    private func load() {
        if let data = UserDefaults.standard.data(forKey: saveKey),
           let decoded = try? JSONDecoder().decode([Conversation].self, from: data) {
            conversations = decoded
            sortConversations()
            restoreSelectedConversationID()
        }
    }
}
