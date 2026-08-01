import Foundation

struct Annotation: Identifiable, Codable, Hashable {
    let id: String
    let type: AnnotationType
    let chapterTitle: String
    let locationInfo: String
    let contentText: String?
    let noteText: String?
    let createdAt: Date
    
    var hasNote: Bool {
        return noteText != nil && !noteText!.isEmpty
    }

    var displayLocation: String {
        if !chapterTitle.isEmpty {
            return chapterTitle
        }
        if !locationInfo.isEmpty {
            return "位置: \(locationInfo)"
        }
        return "未知位置"
    }
}
