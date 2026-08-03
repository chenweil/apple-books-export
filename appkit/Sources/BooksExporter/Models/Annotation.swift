import Foundation

public struct Annotation: Identifiable, Codable, Hashable {
    public let id: String
    public let type: AnnotationType
    public let chapterTitle: String
    public let locationInfo: String
    public let contentText: String?
    public let noteText: String?
    public let createdAt: Date

    public init(
        id: String,
        type: AnnotationType,
        chapterTitle: String,
        locationInfo: String,
        contentText: String?,
        noteText: String?,
        createdAt: Date
    ) {
        self.id = id
        self.type = type
        self.chapterTitle = chapterTitle
        self.locationInfo = locationInfo
        self.contentText = contentText
        self.noteText = noteText
        self.createdAt = createdAt
    }
    
    public var hasNote: Bool {
        return noteText != nil && !noteText!.isEmpty
    }

    public var displayLocation: String {
        if !chapterTitle.isEmpty {
            return chapterTitle
        }
        if !locationInfo.isEmpty {
            return "位置: \(locationInfo)"
        }
        return "未知位置"
    }
}
