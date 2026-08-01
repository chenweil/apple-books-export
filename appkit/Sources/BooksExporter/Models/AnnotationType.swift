import Foundation

enum AnnotationType: String, Codable, CaseIterable {
    case highlight
    case note
    case bookmark
    
    var displayName: String {
        switch self {
        case .highlight:
            return "高亮与标注"
        case .note:
            return "独立笔记"
        case .bookmark:
            return "书签"
        }
    }
    
    var sortOrder: Int {
        switch self {
        case .highlight: return 0
        case .note: return 1
        case .bookmark: return 2
        }
    }
}
