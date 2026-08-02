import Foundation

enum AnnotationType: String, Codable, CaseIterable {
    case highlight
    case note
    case bookmark
    
    /// Markdown 导出时的分节标题。
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

    /// 界面上的短标签(分段控件按钮放不下长标题)。
    var shortName: String {
        switch self {
        case .highlight: return "高亮"
        case .note: return "笔记"
        case .bookmark: return "书签"
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
