import Foundation

struct Conversation: Identifiable, Codable {
    var id: UUID
    var title: String
    var messages: [ChatMessage]
    var isPinned: Bool

    init(title: String = "New Chat", messages: [ChatMessage] = [], isPinned: Bool = false) {
        self.id = UUID()
        self.title = title
        self.messages = messages
        self.isPinned = isPinned
    }
}
