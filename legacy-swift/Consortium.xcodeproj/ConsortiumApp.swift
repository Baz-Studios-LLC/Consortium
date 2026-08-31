import SwiftUI

@main
struct MyApp: App {
    @StateObject private var appState = AppState()
    @StateObject private var userSettings = UserSettings()

    var body: some Scene {
        WindowGroup {
            MainView()
                .environmentObject(appState)
                .environmentObject(userSettings)
        }
    }
}

final class AppState: ObservableObject {
    // Add app-wide state properties here
}

final class UserSettings: ObservableObject {
    // Add user settings properties here
}

struct MainView: View {
    @EnvironmentObject var appState: AppState
    @EnvironmentObject var userSettings: UserSettings

    var body: some View {
        Text("Welcome to MyApp")
            .padding()
    }
}
