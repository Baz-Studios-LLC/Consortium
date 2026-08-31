import Foundation

struct BotConfig {
    let role: ChatMessage.Role
    let displayName: String
    let avatarSystemName: String  // SF Symbol for now

    static let chatGPT  = BotConfig(role: .chatgpt, displayName: "ChatGPT", avatarSystemName: "brain.head.profile")
    static let gemini   = BotConfig(role: .gemini,  displayName: "Gemini",  avatarSystemName: "sparkles")
    static let grok     = BotConfig(role: .grok,    displayName: "Grok",    avatarSystemName: "atom")
    static let claude   = BotConfig(role: .claude,  displayName: "Claude",  avatarSystemName: "cloud.sun.fill")
    static let user     = BotConfig(role: .user,    displayName: "You",     avatarSystemName: "person.fill")
}