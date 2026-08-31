import Foundation

/// Common interface for AI model services (OpenAI, Claude, Gemini, Grok).
protocol AIModelService {
    var modelType: AIModelType { get }
    func generateResponse(history: [ChatMessage], userMessage: String) async -> ChatMessage?
}
