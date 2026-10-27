import Foundation

/// Stable failures do not include source data, credentials or filesystem paths.
enum WidgetSnapshotError: Error { case invalid, unavailable, tooLarge }

/// Fixed JSON codecs shared by the platform bridge and WidgetKit shell.
enum SharedJSON {
    static let encoder = JSONEncoder()
    static let sortedEncoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }()
    static let decoder = JSONDecoder()
}
