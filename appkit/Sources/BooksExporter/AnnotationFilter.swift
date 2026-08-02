import Foundation

/// 详情页笔记列表的类型筛选。逻辑保持纯函数,跟 BookListSorter 一样
/// 可以脱离 UI 直接断言。
enum AnnotationFilter: Equatable {
    case all
    case type(AnnotationType)

    /// 分段控件的段顺序,与 AnnotationType.sortOrder 对齐。
    static let ordered: [AnnotationFilter] =
        [.all] + AnnotationType.allCases
            .sorted { $0.sortOrder < $1.sortOrder }
            .map { .type($0) }

    func title(for book: Book) -> String {
        switch self {
        case .all:
            return "全部 \(book.totalAnnotations)"
        case .type(let type):
            return "\(type.shortName) \(book.count(of: type))"
        }
    }

    /// 还没选书时的段标题 —— 此时没有任何计数可显示。
    var placeholderTitle: String {
        switch self {
        case .all: return "全部"
        case .type(let type): return type.shortName
        }
    }

    func apply(to annotations: [Annotation]) -> [Annotation] {
        switch self {
        case .all:
            return annotations
        case .type(let type):
            return annotations.filter { $0.type == type }
        }
    }
}
