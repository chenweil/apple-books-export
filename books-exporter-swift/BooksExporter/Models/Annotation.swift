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
}
