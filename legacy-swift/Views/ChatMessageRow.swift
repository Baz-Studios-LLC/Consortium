//
//  ChatMessageRow.swift
//  Consortium
//
//  Created by Brett Bazaar on 11/28/25.
//

import SwiftUI

/// A single chat bubble row in the conversation view.
/// Displays the avatar, name, timestamp, and message content.
struct ChatMessageRow: View {
    
    let message: ChatMessage
    
    var isUser: Bool {
        if case .user = message.role { return true }
        return false
    }
    
    private static let timestampFormatter: DateFormatter = {
        let f = DateFormatter()
        f.timeStyle = .short
        f.dateStyle = .none
        return f
    }()
    
    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            
            // Avatar (only on the leading side)
            if !isUser {
                Text(message.role.avatar)
                    .font(.system(size: 30))
                    .padding(.top, 2)
            }
            
            VStack(alignment: .leading, spacing: 4) {
                
                // Name + Timestamp Row
                HStack {
                    Text(message.role.displayName)
                        .font(.caption)
                        .foregroundColor(.secondary)
                    
                    Text(Self.timestampFormatter.string(from: message.timestamp))
                        .font(.caption2)
                        .foregroundColor(.secondary)
                }
                
                // Chat bubble
                Text(message.content)
                    .padding(10)
                    .background(isUser ? Color.blue.opacity(0.2) : Color.gray.opacity(0.15))
                    .cornerRadius(12)
            }
            
            // Avatar for user messages on trailing side
            if isUser {
                Text("👤")
                    .font(.system(size: 30))
                    .padding(.top, 2)
            }
        }
        .frame(maxWidth: .infinity, alignment: isUser ? .trailing : .leading)
        .padding(.vertical, 4)
        .padding(.horizontal, 8)
    }
}

#Preview {
    ChatMessageRow(message: ChatMessage(role: .model(AIModelType.chatgpt), content: "This is a test response from ChatGPT."))
}

