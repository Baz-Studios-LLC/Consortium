import SwiftUI

struct MessageRowView: View {
    let message: ChatMessage
    let displayName: String

    private var name: String {
        switch message.role {
        case .user:   return displayName
        case .chatgpt: return "ChatGPT"
        case .gemini:  return "Gemini"
        case .grok:    return "Grok"
        case .claude:  return "Claude"
        }
    }

    private var alignment: Alignment {
        message.role == .user ? .trailing : .leading
    }

    var body: some View {
        HStack {
            if alignment == .leading {
                bubble
                Spacer()
            } else {
                Spacer()
                bubble
            }
        }
    }

    private var bubble: some View {
        VStack(alignment: alignment == .leading ? .leading : .trailing, spacing: 4) {
            Text(name).font(.caption).foregroundColor(.secondary)
            Text(message.text)
                .padding(10)
                .background(alignment == .leading ? Color.gray.opacity(0.15) : Color.blue.opacity(0.85))
                .foregroundColor(alignment == .leading ? .primary : .white)
                .cornerRadius(12)
        }
    }
}