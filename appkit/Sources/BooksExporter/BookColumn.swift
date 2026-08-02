import Foundation

/// 书单表格的列。此前 "book"/"count" 以裸字符串散落在表格列标识、
/// cell 取值、排序器和持久化里,任何一处笔误都只会在运行时静默失效。
enum BookColumn: String, CaseIterable {
    case book
    case count

    var title: String {
        switch self {
        case .book: return "书名"
        case .count: return "笔记"
        }
    }
}
