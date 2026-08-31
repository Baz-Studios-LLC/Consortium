//
//  Extensions.swift
//  Consortium
//
//  Created by Brett Bazaar on 11/28/25.
//

import Foundation
import SwiftUI

// MARK: - Date Formatting
extension Date {
    /// Returns a human-readable timestamp (e.g., "2:34 PM")
    var formattedTimestamp: String {
        let formatter = DateFormatter()
        formatter.timeStyle = .short
        formatter.dateStyle = .none
        return formatter.string(from: self)
    }
}

// MARK: - String Helpers
extension String {
    /// Removes surrounding whitespace and newlines.
    var trimmed: String {
        trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Returns true if the string still contains meaningful text.
    var isMeaningful: Bool {
        !trimmed.isEmpty
    }
}

// MARK: - Optional Helpers
extension Optional where Wrapped == String {
    /// Safe optional shorthand for UI use.
    var safe: String {
        self ?? ""
    }
}

// MARK: - View Extensions
extension View {
    /// Applies a standard fade-in animation for chat messages.
    func chatAppearAnimation() -> some View {
        self.transition(.opacity.animation(.easeIn(duration: 0.2)))
    }
}
