import Foundation
import SwiftUI

class APIKeysViewModel: ObservableObject {
    @Published var openAIKey: String = APIKeyManager.shared.load(for: .openAI) ?? ""
    @Published var claudeKey: String = APIKeyManager.shared.load(for: .claude) ?? ""
    @Published var googleKey: String = APIKeyManager.shared.load(for: .gemini) ?? ""
    @Published var grokKey: String = APIKeyManager.shared.load(for: .grok) ?? ""

    @Published private(set) var hasChanges: Bool = false

    private var initialState: (open: String, claude: String, google: String, grok: String)

    init() {
        initialState = (openAIKey, claudeKey, googleKey, grokKey)
        updateHasChanges()
    }

    func save() {
        APIKeyManager.shared.save(key: openAIKey, for: .openAI)
        APIKeyManager.shared.save(key: claudeKey, for: .claude)
        APIKeyManager.shared.save(key: googleKey, for: .gemini)
        APIKeyManager.shared.save(key: grokKey, for: .grok)
        initialState = (openAIKey, claudeKey, googleKey, grokKey)
        updateHasChanges()
    }

    func reset() {
        openAIKey = ""
        claudeKey = ""
        googleKey = ""
        grokKey = ""
        updateHasChanges()
    }

    private func updateHasChanges() {
        hasChanges = (openAIKey, claudeKey, googleKey, grokKey) != initialState
    }
}
