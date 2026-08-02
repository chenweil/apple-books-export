import Foundation

enum BookListSorter {
    static func sort(_ books: [Book], by column: BookColumn?, ascending: Bool) -> [Book] {
        guard let column else { return books }
        return books.sorted {
            switch column {
            case .book:
                return ascending
                    ? $0.title.localizedStandardCompare($1.title) == .orderedAscending
                    : $0.title.localizedStandardCompare($1.title) == .orderedDescending
            case .count:
                return ascending
                    ? $0.totalAnnotations < $1.totalAnnotations
                    : $0.totalAnnotations > $1.totalAnnotations
            }
        }
    }
}
