import SwiftUI

/// Original Watch palette, shared by the paged cards and the detail view.
func behaviorColor(_ kind: String) -> Color {
    switch kind {
    case "running": return Color(red: 103 / 255, green: 232 / 255, blue: 249 / 255)
    case "asking": return Color(red: 174 / 255, green: 140 / 255, blue: 255 / 255)
    case "idle": return .white.opacity(0.7)
    default: return .orange
    }
}
