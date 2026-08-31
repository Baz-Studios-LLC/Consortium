import Foundation

struct ChatMessage: Identifiable, Equatable {
    enum Role: String, Codable {
        case user, chatgpt, gemini, grok, claude
    }

    let id: UUID
    let role: Role
    var text: String
    var image: ImageAttachment?

    init(id: UUID = UUID(), role: Role, text: String, image: ImageAttachment? = nil) {
        self.id = id
        self.role = role
        self.text = text
        self.image = image
    }
}

struct ImageAttachment: Equatable {
    enum Source {
        case url(URL)
        case data(Data)
    }

    let source: Source
}