//
//  Constants.swift
//  Consortium
//
//  Created by Brett Bazaar on 11/28/25.
//

import Foundation
import SwiftUI

/// Global constants used throughout the app.
/// This file prevents magic strings, repeated values, or duplicated logic.
enum Constants {
    
    // MARK: - App Info
    static let appName = "Consortium"
    static let maxMessageHistory = 20  // how many recent messages models see
    
    // MARK: - UI
    static let defaultAvatarSize: CGFloat = 34
    static let chatBubbleCornerRadius: CGFloat = 12
    static let animationSpeed: Double = 0.25
    
    // MARK: - System Prompts (Shared Behavior)
    /// Every AI model will append this to its own personality prompt.
    /// Ensures multi-agent awareness and conversational tone.
    static let sharedGroupInstruction =
    """
    You are part of a multi-agent conversation where other AI models may respond before or after you. 
    You must acknowledge the presence of other agents and may reference or reply to their messages. 
    Keep responses thoughtful, concise, and natural.
    """
    
    // MARK: - Placeholder Text
    static let placeholderInput = "Type here to speak to the consortium..."
    
    // MARK: - Error Messages
    static let missingAPIKeyMessage = "⚠️ API key not configured. Add it in settings."
}
