//
//  OpenAIChatService.swift
//  Consortium
//
//  Created by Assistant on 11/28/25.
//

import Foundation

/// Simple client for OpenAI Chat Completions API (non-streaming).
/// Uses APIKeyManager to retrieve the OpenAI API key.
final class OpenAIChatService {
    // MARK: - Types

    struct Message: Codable {
        let role: String   // "system" | "user" | "assistant"
        let content: String
    }

    private struct ChatRequest: Codable {
        let model: String
        let messages: [Message]
        // Add optional params as needed (e.g., temperature, max_tokens)
    }

    private struct ChatResponse: Codable {
        struct Choice: Codable {
            struct Message: Codable {
                let role: String
                let content: String
            }
            let index: Int
            let message: Message
            let finish_reason: String?
        }
        let choices: [Choice]
    }

    // MARK: - Properties

    private let session: URLSession

    init(session: URLSession = .shared) {
        self.session = session
    }

    // MARK: - Public API

    /// Sends a chat completion request and returns the assistant's reply text.
    /// - Parameters:
    ///   - messages: Conversation messages in order.
    ///   - model: OpenAI model identifier. Defaults to "gpt-5-mini".
    func send(messages: [Message], model: String = "gpt-5-mini") async throws -> String {
        guard let apiKey = APIKeyManager.shared.load(for: .openAI), !apiKey.isEmpty else {
            throw NSError(domain: "OpenAIChatService", code: 1, userInfo: [NSLocalizedDescriptionKey: "Missing OpenAI API key. Set it in Settings."])
        }

        var request = URLRequest(url: URL(string: "https://api.openai.com/v1/chat/completions")!)
        request.httpMethod = "POST"
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")

        let body = ChatRequest(model: model, messages: messages)
        request.httpBody = try JSONEncoder().encode(body)

        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw NSError(domain: "OpenAIChatService", code: 2, userInfo: [NSLocalizedDescriptionKey: "Invalid response."])
        }
        guard (200..<300).contains(http.statusCode) else {
            let details = String(data: data, encoding: .utf8) ?? "<no body>"
            throw NSError(domain: "OpenAIChatService", code: http.statusCode, userInfo: [NSLocalizedDescriptionKey: "OpenAI error (\(http.statusCode)): \(details)"])
        }

        let decoded = try JSONDecoder().decode(ChatResponse.self, from: data)
        return decoded.choices.first?.message.content ?? ""
    }
}
