import Foundation

/// Helper for saving image data to the user's Downloads folder.
enum ImageDownload {
    static func saveToDownloads(data: Data, suggestedFileName: String) throws -> URL {
        let downloadsURL = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first!
        let fileURL = downloadsURL.appendingPathComponent(suggestedFileName)
        try data.write(to: fileURL, options: .atomic)
        return fileURL
    }
}

