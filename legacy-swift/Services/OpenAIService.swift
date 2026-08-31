//
//  OpenAIService.swift
//  Consortium
//
//  Created by Brett Bazaar on 11/28/25.
//


import Foundation

/// Handles OpenAI ChatGPT responses using the shared AIModelService protocol.
class OpenAIService: AIModelService {
    
    let modelType: AIModelType = .chatgpt
    private var apiKey: String
    
    init(apiKey: String) {
        self.apiKey = apiKey
    }
    
    func updateAPIKey(_ newKey: String) {
        self.apiKey = newKey
    }
    
    func generateResponse(history: [ChatMessage], userMessage: String) async -> ChatMessage? {
        
        guard !apiKey.isEmpty else {
            return ChatMessage(role: .model(AIModelType.chatgpt), content: "⚠️ OpenAI API key missing. Add it in Settings.")
        }
        
        // Format conversation
        var messages: [[String: String]] = []
        
        let systemPrompt = modelType.systemPrompt + "\n" + Constants.sharedGroupInstruction
        
        messages.append(["role": "system", "content": systemPrompt])
        
        for message in history {
            let role: String
            switch message.role {
            case .user:
                role = "user"
            case .model:
                role = "assistant"
            }
            messages.append(["role": role, "content": message.content])
        }
        
        messages.append(["role": "user", "content": userMessage])
        
        let body: [String: Any] = [
            // Use snapshot if you want stable behavior: gpt-5-mini-2025-08-07
            "model": "gpt-5-mini",
            "messages": messages,
            "max_tokens": 500
        ]
        
        guard let url = URL(string: "https://api.openai.com/v1/chat/completions") else {
            return ChatMessage(role: .model(AIModelType.chatgpt), content: "❌ Invalid OpenAI URL.")
        }
        
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        
        do {
            request.httpBody = try JSONSerialization.data(withJSONObject: body, options: [])
        } catch {
            return ChatMessage(role: .model(AIModelType.chatgpt), content: "❌ Failed to encode request.")
        }
        
        do {
            let (data, _) = try await URLSession.shared.data(for: request)
            
            if let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let choices = json["choices"] as? [[String: Any]],
               let message = choices.first?["message"] as? [String: Any],
               let text = message["content"] as? String {
                
                return ChatMessage(role: .model(AIModelType.chatgpt), content: text)
            }
            
            return ChatMessage(role: .model(AIModelType.chatgpt), content: "❌ OpenAI returned an unexpected response format.")
            
        } catch {
            return ChatMessage(role: .model(AIModelType.chatgpt), content: "❌ Request error: \(error.localizedDescription)")
        }
    }
}

