//
//  SidebarView.swift
//  Consortium
//
//  Created by Brett Bazaar on 11/28/25.
//

import SwiftUI

/// Sidebar navigation for macOS layout.
/// Currently supports: Chat, Settings, and future sections like History and About.
struct SidebarView: View {
    
    @EnvironmentObject var chatViewModel: ChatViewModel
    @EnvironmentObject var settings: AppSettings
    
    enum SidebarSection: String, CaseIterable, Identifiable {
        case chat = "Chat"
        case settings = "Settings"
        case history = "Conversation History"
        case about = "About"

        var id: String { rawValue }
        
        var icon: String {
            switch self {
            case .chat: return "message.fill"
            case .settings: return "gearshape.fill"
            case .history: return "clock.fill"
            case .about: return "info.circle.fill"
            }
        }
    }
    
    @State private var selection: SidebarSection? = .chat
    
    var body: some View {
        List(selection: $selection) {
            ForEach(SidebarSection.allCases) { section in
                Label(section.rawValue, systemImage: section.icon)
                    .tag(section)
            }
        }
        .listStyle(.sidebar)
        .navigationTitle("Consortium")
        .frame(minWidth: 220)
    }
}

#Preview {
    SidebarView()
        .environmentObject(ChatViewModel())
        .environmentObject(AppSettings())
}
