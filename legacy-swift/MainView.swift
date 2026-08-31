import SwiftUI

struct MainView: View {
    var body: some View {
        NavigationView {
            VStack(spacing: 16) {
                Text("Consortium")
                    .font(.largeTitle)
                    .bold()

                Text("The app compiled successfully 🎉")
                    .foregroundColor(.secondary)

                NavigationLink("Go to Chat") {
                    ChatView()
                }

                NavigationLink("Settings") {
                    SettingsView()
                }
            }
            .padding()
        }
    }
}

#Preview {
    MainView()
}
