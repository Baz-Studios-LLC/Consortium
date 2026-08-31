//
//  AppSettings.swift
//  Consortium
//
//  Created by Brett Bazaar on 11/28/25.
//

import Foundation
import SwiftUI

/// Global app configuration and preferences.
/// Stores enabled AI agents, whether auto-discussion is enabled,
/// and any additional toggles the user may configure.
class AppSettings: ObservableObject {

    // MARK: - AI Agent Toggles
    @AppStorage("enableChatGPT") var enableChatGPT: Bool = true
    @AppStorage("enableClaude") var enableClaude: Bool = true
    @AppStorage("enableGemini") var enableGemini: Bool = true
    @AppStorage("enableGrok") var enableGrok: Bool = true

    // MARK: - Conversation Behavior
    @AppStorage("autoDiscussionEnabled") var autoDiscussionEnabled: Bool = false
    @AppStorage("autoTurnCount") var autoTurnCount: Int = 3

    // MARK: - UI Preferences
    @AppStorage("showTimestamps") var showTimestamps: Bool = true
    @AppStorage("compactMode") var compactMode: Bool = false
    @AppStorage("darkModeEnabled") var darkModeEnabled: Bool = false // optional

    // MARK: - Initialization
    init() { }
    
    // MARK: - Convenience Access

    /// Returns a list of enabled models based on toggles.
    var enabledModels: [AIModelType] {
        AIModelType.allCases.filter { model in
            switch model {
            case .chatgpt: return enableChatGPT
            case .claude: return enableClaude
            case .gemini: return enableGemini
            case .grok: return enableGrok
            }
        }
    }

    /// Resets all settings to default values.
    func resetToDefaults() {
        enableChatGPT = true
        enableClaude = true
        enableGemini = true
        enableGrok = true

        autoDiscussionEnabled = false
        autoTurnCount = 3
        
        showTimestamps = true
        compactMode = false
        darkModeEnabled = false
    }
}
