//
//  MessageRouter.swift
//  Consortium
//
//  Created by Brett Bazaar on 11/28/25.
//


//
//  MessageRouter.swift
//  Consortium
//
//  Created by Brett Bazaar on 11/28/25.
//

import Foundation

/// Determines which AI models should respond and in what order.
/// Supports sequential replies, avoiding repetitive turns by the same agent when possible.
class MessageRouter {
    
    /// Tracks the last responding model to avoid immediate repetition.
    private var lastResponder: AIModelType? = nil
    
    /// Returns a list of enabled models in the order they should respond.
    /// Logic:
    /// 1. Models enabled in settings may respond.
    /// 2. Avoids selecting the same model twice in a row.
    /// 3. Randomizes between remaining models if multiple options apply.
    func nextResponders(from enabledModels: [AIModelType]) -> [AIModelType] {
        guard !enabledModels.isEmpty else { return [] }
        
        // If only one model exists, allow it to respond even if repeated.
        if enabledModels.count == 1 {
            lastResponder = enabledModels.first
            return enabledModels
        }
        
        // Filter out last responder if possible
        let eligible = enabledModels.filter { $0 != lastResponder }
        
        let ordered: [AIModelType]
        
        if eligible.isEmpty {
            ordered = enabledModels.shuffled()
        } else {
            ordered = eligible.shuffled()
        }
        
        // Update last responder to the first chosen model
        lastResponder = ordered.first
        
        return ordered
    }
    
    /// Resets routing state (used when clearing conversation or switching models)
    func reset() {
        lastResponder = nil
    }
}
