//
//  GeminiService.swift
//  Consortium
//
//  Created by Brett Bazaar on 11/28/25.
//

import Foundation

/// Handles Google Gemini API requests using the shared AIModelService protocol.
class GeminiService: AIModelService {
    
    let modelType: AIModelType = .gemini
    private var apiKey: String
    
    init(apiKey: String) {
        self.apiKey = apiKey
    }
    
    func updateAPIKey(_ newKey: String) {
        self.apiKey = newKey
    }
    
    func generateResponse(history: [ChatMessage], userMessage: String) async -> ChatMessage? {
        
        guard !apiKey.isEmpty else {
            return ChatMessage(role: .model(.gemini), content: "⚠️ Gemini API key missing. Add it in settings.")
        }
        
        let key = apiKey.trimmingCharacters(in: .whitespacesAndNewlines)
        
        // Convert message history into a conversational transcript
        let transcript = history.map { msg in
            let speaker: String
            switch msg.role {
            case .user: speaker = "User"
            case .model(let t): speaker = t.displayName
            }
            return "\(speaker): \(msg.content)"
        }.joined(separator: "\n") + "\nUser: \(userMessage)"
        
        let systemPrompt = modelType.systemPrompt + "\n" + Constants.sharedGroupInstruction
        
        let requestBody = GeminiRequest(
            contents: [
                GeminiContent(
                    role: "user",
                    parts: [
                        GeminiPart(text: "\(systemPrompt)\n\nTranscript:\n\(transcript)")
                    ]
                )
            ]
        )
        
        guard let url = URL(string: "https://generativelanguage.googleapis.com/v1beta/models/gemini-1.5-flash:generateContent?key=\(key)") else {
            return ChatMessage(role: .model(.gemini), content: "❌ Invalid Gemini URL.")
        }
        
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        
        do {
            request.httpBody = try JSONEncoder().encode(requestBody)
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        } catch {
            return ChatMessage(role: .model(.gemini), content: "❌ Failed to encode Gemini request.")
        }
        
        do {
            let (data, response) = try await URLSession.shared.data(for: request)
            if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
                let body = String(data: data, encoding: .utf8) ?? "<no body>"
                return ChatMessage(role: .model(.gemini), content: "❌ Gemini HTTP \(http.statusCode): \(body)")
            }
            
            if let response = try? JSONDecoder().decode(GeminiResponse.self, from: data),
               let text = response.candidates.first?.content.parts.first?.text {
                return ChatMessage(role: .model(.gemini), content: text)
            } else {
                return ChatMessage(role: .model(.gemini), content: "❌ Gemini returned an unexpected response format.")
            }
            
        } catch {
            return ChatMessage(role: .model(.gemini), content: "❌ Gemini request error: \(error.localizedDescription)")
        }
    }
}

/// MARK: - Gemini Request Models

struct GeminiRequest: Codable {
    let contents: [GeminiContent]
}

struct GeminiContent: Codable {
    let role: String
    let parts: [GeminiPart]
}

struct GeminiPart: Codable {
    let text: String
}

/// MARK: - Gemini Response Models

struct GeminiResponse: Codable {
    let candidates: [GeminiCandidate]
}

struct GeminiCandidate: Codable {
    let content: GeminiContentResponse
}

struct GeminiContentResponse: Codable {
    let parts: [GeminiPart]
}
