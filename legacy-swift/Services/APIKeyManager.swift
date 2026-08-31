//
//  APIKeyManager.swift
//  Consortium
//
//  Created by Brett Bazaar on 11/28/25.
//

import Foundation

/// Handles persistent storage and retrieval of API keys.
/// Keys are stored securely in the user's Keychain.
class APIKeyManager {
    
    static let shared = APIKeyManager()
    
    private init() {}
    
    // MARK: - Key Identifiers
    enum KeyType: String, CaseIterable {
        case openAI = "openai_api_key"
        case claude = "claude_api_key"
        case gemini = "gemini_api_key"
        case grok   = "grok_api_key"
    }
    
    // MARK: - Public API
    
    func save(key: String, for type: KeyType) {
        saveToKeychain(key, account: type.rawValue)
    }
    
    func load(for type: KeyType) -> String? {
        readFromKeychain(account: type.rawValue)
    }
    
    func remove(for type: KeyType) {
        deleteFromKeychain(account: type.rawValue)
    }
    
    func resetAll() {
        KeyType.allCases.forEach { deleteFromKeychain(account: $0.rawValue) }
    }
    
    // MARK: - Keychain Access
    
    private func saveToKeychain(_ value: String, account: String) {
        guard let data = value.data(using: .utf8) else { return }

        let query: [String: Any] = [
            kSecClass as String       : kSecClassGenericPassword,
            kSecAttrAccount as String : account,
            kSecValueData as String   : data
        ]

        SecItemDelete(query as CFDictionary) // Remove old key first
        SecItemAdd(query as CFDictionary, nil)
    }
    
    private func readFromKeychain(account: String) -> String? {
        let query: [String: Any] = [
            kSecClass as String       : kSecClassGenericPassword,
            kSecAttrAccount as String : account,
            kSecReturnData as String  : true,
            kSecMatchLimit as String  : kSecMatchLimitOne
        ]

        var result: AnyObject?
        SecItemCopyMatching(query as CFDictionary, &result)

        guard let data = result as? Data,
              let value = String(data: data, encoding: .utf8)
        else { return nil }

        return value
    }
    
    private func deleteFromKeychain(account: String) {
        let query: [String: Any] = [
            kSecClass as String       : kSecClassGenericPassword,
            kSecAttrAccount as String : account
        ]
        
        SecItemDelete(query as CFDictionary)
    }
}
