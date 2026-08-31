//
//  InputBar.swift
//  Consortium
//
//  Created by Brett Bazaar on 11/28/25.
//

import SwiftUI

/// A standalone reusable chat input bar.
struct InputBar: View {
    
    @Binding var text: String
    var onSend: () -> Void
    
    @FocusState private var focused: Bool
    
    var body: some View {
        HStack(spacing: 10) {
            
            TextField(Constants.placeholderInput, text: $text)
                .textFieldStyle(RoundedBorderTextFieldStyle())
                .focused($focused)
                .onSubmit(handleSend)
            
            Button(action: handleSend) {
                Image(systemName: "arrow.up.circle.fill")
                    .font(.system(size: 28))
                    .foregroundColor(text.isMeaningful ? .accentColor : .gray)
            }
            .buttonStyle(.borderless)
            .disabled(!text.isMeaningful)
        }
        .padding(12)
        .background(.ultraThinMaterial)
        .onAppear {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) {
                focused = true
            }
        }
    }
    
    private func handleSend() {
        guard text.isMeaningful else { return }
        onSend()
        text = ""
        focused = true
    }
}

#Preview {
    InputBar(text: .constant("Hello World")) { }
}
