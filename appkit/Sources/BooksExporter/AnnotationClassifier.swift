import Foundation

/// Apple Books 不把「笔记」存成独立对象 —— 笔记就是给高亮加的批注,
/// 两者同在一行(ZANNOTATIONSELECTEDTEXT 是原文,ZANNOTATIONNOTE 是批注)。
///
/// ZANNOTATIONTYPE 与内容并不对应:实测全库 type 1 有 105 条、type 3 有 379 条
/// 三个文本字段全为空,但它们都带高亮样式、近半数带选区范围,
/// 是取词失败的高亮而非书签。旧代码把 type 1 当「独立笔记」,
/// 于是导出里出现一批只有位置、没有正文的空条目。
///
/// 因此按内容分类,而不是按类型字段。书籍计数与逐条映射都走这里,避免两处漂移。
enum AnnotationClassifier {
    /// 返回 nil 表示这条没有任何可展示的内容,应当丢弃。
    static func classify(hasNote: Bool, hasSelectedText: Bool) -> AnnotationType? {
        if hasNote { return .note }
        if hasSelectedText { return .highlight }
        return nil
    }
}
