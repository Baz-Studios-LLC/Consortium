//
//  GrokService.swift
//  Consortium
//
//  Created by Brett Bazaar on 11/28/25.
//

import Foundation

/// Handles xAI Grok API requests using the shared AIModelService protocol.
class GrokService: AIModelService {
    
    let modelType: AIModelType = .grok
    private var apiKey: String
    
    init(apiKey: String) {
        self.apiKey = apiKey
    }
    
    func updateAPIKey(_ newKey: String) {
        self.apiKey = newKey
    }
    
    func generateResponse(history: [ChatMessage], userMessage: String) async -> ChatMessage? {
        
        guard !apiKey.isEmpty else {
            return ChatMessage(role: .model(.grok), content: "⚠️ Grok API key missing. Add it in settings.")
        }
        
        let key = apiKey.trimmingCharacters(in: .whitespacesAndNewlines)
        
        let formattedHistory = history.map { msg in
            let speaker: String
            switch msg.role {
            case .user: speaker = "User"
            case .model(let t): speaker = t.displayName
            }
            return "\(speaker): \(msg.content)"
        }.joined(separator: "\n") + "\nUser: \(userMessage)"
        
        let systemPrompt = modelType.systemPrompt + "\n" + Constants.sharedGroupInstruction
        
        let requestBody: [String: Any] = [
            "model": "grok-4-1-fast-non-reasoning",
            "messages": [
                ["role": "system", "content": systemPrompt],
                ["role": "user", "content": formattedHistory]
            ],
            "max_tokens": 400
        ]
        
        guard let url = URL(string: "https://api.x.ai/v1/chat/completions") else {
            return ChatMessage(role: .model(.grok), content: "❌ Invalid Grok URL.")
        }
        
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("Bearer \(key)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        
        do {
            request.httpBody = try JSONSerialization.data(withJSONObject: requestBody, options: [])
        } catch {
            return ChatMessage(role: .model(.grok), content: "❌ Failed to encode Grok request.")
        }
        
        do {
            let (data, response) = try await URLSession.shared.data(for: request)
            
            if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
                let body = String(data: data, encoding: .utf8) ?? "<no body>"
                return ChatMessage(role: .model(.grok), content: "❌ Grok HTTP \(http.statusCode): \(body)")
            }
            
            if let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let choices = json["choices"] as? [[String: Any]],
               let message = choices.first?["message"] as? [String: Any],
               let text = message["content"] as? String {
                
                return ChatMessage(role: .model(.grok), content: text)
            }
            
            return ChatMessage(role: .model(.grok), content: "❌ Grok returned an unexpected response format.")
            
        } catch {
            return ChatMessage(role: .model(.grok),
                               content: "❌ Network error: \(error.localizedDescription)")
        }
    }
}

