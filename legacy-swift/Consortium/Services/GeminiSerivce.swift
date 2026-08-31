import Foundation

/// Service wrapper for Google Gemini – currently stubbed.
/// Replace the stubbed methods with real Gemini API calls when ready.
final class GeminiService {
    private let apiKeyProvider: () -> String

    init(apiKeyProvider: @escaping () -> String) {
        self.apiKeyProvider = apiKeyProvider
    }

    /// Generate a chat response from Gemini.
    func chatCompletion(messages: [ChatMessage], userName: String) async throws -> String {
        // TODO: Implement real call to Gemini chat endpoint.
        return "Gemini stub reply"
    }

    /// Generate an image (e.g., Gemini 2.5 Flash image). Returns a URL if available.
    func generateImage(prompt: String) async throws -> URL? {
        // TODO: Implement real call to Gemini image generation endpoint.
        return nil
    }

    /// Decide if Gemini should respond in auto mode.
    func shouldRespond(messages: [ChatMessage], lastSpeaker: ChatMessage.Role, userName: String) async -> Bool {
        // TODO: Implement decision logic or API call mirroring Python's should_respond_gemini.
        return true
    }
}

