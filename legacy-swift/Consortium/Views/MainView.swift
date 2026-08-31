import SwiftUI

struct MainView: View {
    @EnvironmentObject var settings: SettingsStore
    @EnvironmentObject var chatVM: ChatViewModel
    @EnvironmentObject var sidebarVM: SidebarViewModel
    @EnvironmentObject var apiKeysVM: APIKeysViewModel

    var body: some View {
        NavigationSplitView {
            SidebarView()
        } detail: {
            ChatView()
                .sheet(isPresented: $apiKeysVM.isPresenting) {
                    APIKeysSheetView()
                        .frame(minWidth: 400, minHeight: 300)
                }
        }
    }
}

