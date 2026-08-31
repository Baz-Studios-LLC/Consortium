//
//  FileStorage.swift
//  Consortium
//
//  Created by Brett Bazaar on 11/28/25.
//

import Foundation

/// A lightweight helper for saving and loading Codable data to disk.
/// This keeps the logic reusable for chat history, settings backups, or future features.
struct FileStorage {
    
    // MARK: - Save
    
    static func save<T: Codable>(_ value: T, to fileName: String) {
        do {
            let url = fileURL(for: fileName)
            let data = try JSONEncoder().encode(value)
            try data.write(to: url, options: .atomic)
        } catch {
            print("❌ FileStorage save error (\(fileName)): \(error.localizedDescription)")
        }
    }
    
    // MARK: - Load
    
    static func load<T: Codable>(_ fileName: String, as type: T.Type) -> T? {
        do {
            let url = fileURL(for: fileName)
            guard FileManager.default.fileExists(atPath: url.path) else { return nil }
            let data = try Data(contentsOf: url)
            return try JSONDecoder().decode(type, from: data)
        } catch {
            print("❌ FileStorage load error (\(fileName)): \(error.localizedDescription)")
            return nil
        }
    }
    
    // MARK: - File Path Generator
    
    private static func fileURL(for fileName: String) -> URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let folder = base.appendingPathComponent("Consortium", isDirectory: true)
        
        if !FileManager.default.fileExists(atPath: folder.path) {
            try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        }
        
        return folder.appendingPathComponent(fileName)
    }
}
