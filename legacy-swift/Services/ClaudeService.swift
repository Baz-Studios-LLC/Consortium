//
//  ClaudeService.swift
//  Consortium
//
//  Created by Brett Bazaar on 11/28/25.
//

import Foundation

/// Handles Anthropic Claude responses using the shared AIModelService protocol.
class ClaudeService: AIModelService {
    
    let modelType: AIModelType = .claude
    private var apiKey: String
    
    init(apiKey: String) {
        self.apiKey = apiKey
    }
    
    func updateAPIKey(_ newKey: String) {
        self.apiKey = newKey
    }
    
    func generateResponse(history: [ChatMessage], userMessage: String) async -> ChatMessage? {
        
        guard !apiKey.isEmpty else {
            return ChatMessage(role: .model(.claude),
                               content: "⚠️ Claude API key missing. Add it in settings.")
        }
        
        let key = apiKey.trimmingCharacters(in: .whitespacesAndNewlines)
        
        // Build conversation format Claude expects
        let formattedConversation = history.map { msg in
            switch msg.role {
            case .user:
                return "User: \(msg.content)"
            case .model(let t):
                return "\(t.displayName): \(msg.content)"
            }
        }.joined(separator: "\n") + "\nUser: \(userMessage)"
        
        let systemPrompt = modelType.systemPrompt + "\n" + Constants.sharedGroupInstruction
        
        // Compose request payload
        let payload: [String: Any] = [
            "model": "claude-3-5-sonnet-latest",
            "messages": [
                ["role": "system", "content": systemPrompt],
                ["role": "user", "content": formattedConversation]
            ],
            "max_tokens": 500
        ]
        
        guard let url = URL(string: "https://api.anthropic.com/v1/messages") else { return nil }
        
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue(key, forHTTPHeaderField: "x-api-key")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        
        do {
            request.httpBody = try JSONSerialization.data(withJSONObject: payload, options: [])
        } catch {
            return ChatMessage(role: .model(.claude), content: "❌ Failed to encode request.")
        }
        
        do {
            let (data, response) = try await URLSession.shared.data(for: request)
            if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
                let body = String(data: data, encoding: .utf8) ?? "<no body>"
                return ChatMessage(role: .model(.claude), content: "❌ Claude HTTP \(http.statusCode): \(body)")
            }
            
            if let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let content = json["content"] as? [[String: Any]],
               let text = content.first?["text"] as? String {
                
                return ChatMessage(role: .model(.claude), content: text)
            }
            
            return ChatMessage(role: .model(.claude), content: "❌ Claude returned an unexpected response format.")
            
        } catch {
            return ChatMessage(role: .model(.claude),
                               content: "❌ Network error: \(error.localizedDescription)")
        }
    }
}
