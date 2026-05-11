import Foundation

struct Book: Identifiable, Codable, Hashable {
    let id: String              // ADLR 或 ASIN
    let title: String
    let author: String
    let totalAnnotations: Int
    let highlightsCount: Int
    let annotationsCount: Int
    let notesCount: Int
    let bookmarksCount: Int
    
    var displayTotalCount: String {
        if totalAnnotations > 0 {
            return "\(totalAnnotations)条笔记"
        } else {
            return "无笔记"
        }
    }
}
