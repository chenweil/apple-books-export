import Foundation

public struct Book: Identifiable, Codable, Hashable {
    public let id: String              // ADLR 或 ASIN
    public let title: String
    public let author: String
    public let totalAnnotations: Int
    public let highlightsCount: Int
    public let notesCount: Int

    public init(
        id: String,
        title: String,
        author: String,
        totalAnnotations: Int,
        highlightsCount: Int,
        notesCount: Int
    ) {
        self.id = id
        self.title = title
        self.author = author
        self.totalAnnotations = totalAnnotations
        self.highlightsCount = highlightsCount
        self.notesCount = notesCount
    }

    public var displayTotalCount: String {
        if totalAnnotations > 0 {
            return "\(totalAnnotations)条笔记"
        } else {
            return "无笔记"
        }
    }

    public func count(of type: AnnotationType) -> Int {
        switch type {
        case .highlight: return highlightsCount
        case .note: return notesCount
        }
    }
}
