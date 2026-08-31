import SwiftUI
import Combine

// NOTE: Disabled duplicate @main to resolve build conflict. Keep only one app entry point.
// @main
struct ConsortiumApp: App {
    @StateObject private var settingsStore = SettingsStore()
    @StateObject private var chatViewModel = ChatViewModel()
    @StateObject private var sidebarViewModel = SidebarViewModel()
    @StateObject private var apiKeysViewModel = APIKeysViewModel()

    var body: some Scene {
        WindowGroup {
            MainView()
                .environmentObject(settingsStore)
                .environmentObject(chatViewModel)
                .environmentObject(sidebarViewModel)
                .environmentObject(apiKeysViewModel)
        }
    }
}

