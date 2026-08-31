import Foundation
import Combine
import SwiftUI

@MainActor
class ChatViewModel: ObservableObject {
    @Published var messages: [ChatMessage] = []
    @Published var input: String = ""
    
    func send() {
        let trimmed = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }

        // This ViewModel is no longer responsible for network calls.
        // ChatView + ConversationStore handle sending to services.
        let newMessage = ChatMessage(role: .user, content: trimmed)
        messages.append(newMessage)
        input = ""
    }
}
