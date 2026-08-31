import Foundation

/// Role of a message in the multi-agent chat.
/// Supports the human user and AI agents identified by AIModelType.
enum ChatRole: Codable {
    case user
    case model(AIModelType)

    private enum CodingKeys: String, CodingKey { case kind, value }

    // Codable conformance
    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let kind = try container.decode(String.self, forKey: .kind)
        switch kind {
        case "user":
            self = .user
        case "model":
            let raw = try container.decode(String.self, forKey: .value)
            guard let t = AIModelType(rawValue: raw) else { self = .user; return }
            self = .model(t)
        default:
            self = .user
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .user:
            try container.encode("user", forKey: .kind)
        case .model(let t):
            try container.encode("model", forKey: .kind)
            try container.encode(t.rawValue, forKey: .value)
        }
    }

    var displayName: String {
        switch self {
        case .user: return "User"
        case .model(let t): return t.displayName
        }
    }

    var avatar: String {
        switch self {
        case .user: return "👤"
        case .model(let t):
            switch t {
            case .chatgpt: return "🟢"
            case .claude: return "🟡"
            case .gemini: return "🔵"
            case .grok: return "🟣"
            }
        }
    }
}

/// Unified message type used across UI and services.
struct ChatMessage: Identifiable, Codable {
    let id: UUID
    var role: ChatRole
    var content: String
    var timestamp: Date

    init(id: UUID = UUID(), role: ChatRole, content: String, timestamp: Date = Date()) {
        self.id = id
        self.role = role
        self.content = content
        self.timestamp = timestamp
    }
}
