//
//  SettingsViewModel.swift
//  Consortium
//
//  Created by Brett Bazaar on 11/28/25.
//

import Foundation
import SwiftUI

/// ViewModel holder for UI-bound settings and state.
/// This may expand later for profile configurations, themes, or app metadata.
class SettingsViewModel: ObservableObject {

    @Published var showAPIKeySection: Bool = true
    @Published var showAdvancedOptions: Bool = false
    @Published var selectedTheme: Theme = .system
    
    enum Theme: String, CaseIterable, Identifiable {
        case light = "Light"
        case dark = "Dark"
        case system = "System"
        
        var id: String { rawValue }
    }
    
    /// Placeholder for future user meta (app version, build info, onboarding state, etc.)
    var appVersion: String {
        Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "Unknown"
    }
    
    var buildNumber: String {
        Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "?"
    }
    
    func toggleAdvancedOptions() {
        withAnimation {
            showAdvancedOptions.toggle()
        }
    }
}
