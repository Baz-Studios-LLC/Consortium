import Foundation
import Combine

final class SidebarViewModel: ObservableObject {
    @Published var enableChatGPT = true
    @Published var enableGemini  = true
    @Published var enableGrok    = true
    @Published var enableClaude  = true
}
