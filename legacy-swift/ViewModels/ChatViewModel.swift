//
//  MainView.swift
//  Consortium
//
//  Created by Brett Bazaar on 11/28/25.
//

import SwiftUI

/// The root layout of the app, connecting the sidebar and content views.
struct MainView: View {
    
    @State private var selection: Section = .chat
    
    enum Section: String, CaseIterable, Identifiable {
        case chat = "Chat"
        case settings = "Settings"
        
        var id: String { rawValue }
        
        @ViewBuilder
        var destinationView: some View {
            switch self {
            case .chat:
                ChatView()
            case .settings:
                SettingsView()
            }
        }
        
        var icon: String {
            switch self {
            case .chat: return "message.fill"
            case .settings: return "gear"
            }
        }
    }
    
    var body: some View {
        NavigationSplitView {
            List(Section.allCases, selection: $selection) { section in
                Label(section.rawValue, systemImage: section.icon)
                    .tag(section)
            }
            .navigationTitle("Consortium")
        } detail: {
            selection.destinationView
        }
    }
}

#Preview {
    MainView()
        .environmentObject(ChatViewModel())
        .environmentObject(AppSettings())
        .environmentObject(APIKeysViewModel())
}
