//
//  AIModelType.swift
//  Consortium
//
//  Created by Brett Bazaar on 11/28/25.
//

import Foundation
import SwiftUI

/// Represents each AI system available in the app.
/// Each case is treated as a unique persona in the shared conversation.
enum AIModelType: String, CaseIterable, Identifiable, Codable {
    case chatgpt
    case claude
    case gemini
    case grok

    var id: String { rawValue }

    /// The visible name shown in UI
    var displayName: String {
        switch self {
        case .chatgpt: return "ChatGPT"
        case .claude: return "Claude"
        case .gemini: return "Gemini"
        case .grok: return "Grok"
        }
    }

    /// Emoji identity used for quick visual association.
    var avatar: String {
        switch self {
        case .chatgpt: return "🟢"
        case .claude: return "🟡"
        case .gemini: return "🔵"
        case .grok: return "🟣"
        }
    }

    /// System prompt that each model will use when generating a response.
    /// This ensures each AI knows it's part of a multi-agent discussion.
    var systemPrompt: String {
        "You are \(displayName), an AI participant in a group conversation alongside other AI systems. You may respond to the user or reference previous messages from other agents. Maintain your own identity and reasoning style."
    }
}
