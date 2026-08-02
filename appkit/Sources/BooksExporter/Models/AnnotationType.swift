import Foundation

enum AnnotationType: String, Codable, CaseIterable {
    case highlight
    case note

    /// Markdown 导出时的分节标题。
    var displayName: String {
        switch self {
        case .highlight:
            return "高亮"
        case .note:
            return "笔记"
        }
    }

    /// 界面上的短标签(分段控件按钮放不下长标题)。
    var shortName: String {
        switch self {
        case .highlight: return "高亮"
        case .note: return "笔记"
        }
    }

    var sortOrder: Int {
        switch self {
        case .highlight: return 0
        case .note: return 1
        }
    }
}
