import Foundation

@Observable
class BookService {
    private let databaseService = DatabaseService()
    
    var isBusy = false
    var currentError: Error?
    
    func listBooks() async -> [Book] {
        isBusy = true
        currentError = nil
        
        do {
            let books = try databaseService.getBooks()
            isBusy = false
            return books
        } catch {
            currentError = error
            isBusy = false
            return []
        }
    }
    
    func getAnnotations(for bookId: String) async -> [Annotation] {
        do {
            return try databaseService.getAnnotations(for: bookId)
        } catch {
            currentError = error
            return []
        }
    }
    
    func exportToMarkdown(book: Book, annotations: [Annotation], outputURL: URL) async throws {
        isBusy = true
        
        let fileURL = outputURL.appendingPathComponent("\(sanitizedTitle(book.title)).md")
        
        var content = "# \(book.title)\n\n"
        content += "**作者**: \(book.author)\n\n"
        content += "**笔记数量**: \(book.totalAnnotations)\n\n"
        content += "---\n\n"
        
        let groupedAnnotations = Dictionary(grouping: annotations) { $0.type }
        
        for type in AnnotationType.allCases.sorted(by: { $0.sortOrder < $1.sortOrder }) {
            guard let typeAnnotations = groupedAnnotations[type], !typeAnnotations.isEmpty else {
                continue
            }
            
            content += "## \(type.displayName)\n\n"
            
            for (index, annotation) in typeAnnotations.enumerated() {
                content += "### \(index + 1). \(annotation.displayLocation)\n"
                content += "*\(formatDate(annotation.createdAt))*\n\n"
                
                if let text = annotation.contentText {
                    content += "> \(text)\n\n"
                }
                
                if let note = annotation.noteText, !note.isEmpty {
                    content += "**笔记**: \(note)\n\n"
                }
                
                content += "---\n\n"
            }
        }
        
        try content.write(to: fileURL, atomically: true, encoding: .utf8)
        isBusy = false
    }
    
    private func sanitizedTitle(_ title: String) -> String {
        let invalidChars = CharacterSet(charactersIn: "/\\:*?\"<>|")
        return title.components(separatedBy: invalidChars).joined(separator: "-")
    }
    
    private func formatDate(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd HH:mm"
        return formatter.string(from: date)
    }
}
