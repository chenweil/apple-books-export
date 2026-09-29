import Foundation

class BookService {
    private let rustCLIClient: RustCLIClient
    
    var isBusy = false
    var currentError: Error?

    init(rustCLIClient: RustCLIClient = .makeForCurrentApp()) {
        self.rustCLIClient = rustCLIClient
    }
    
    func listBooks() async -> [Book] {
        isBusy = true
        currentError = nil
        
        do {
            let books = try await rustCLIClient.listBooks()
            isBusy = false
            return books
        } catch {
            currentError = error
            isBusy = false
            return []
        }
    }
    
    func getAnnotations(for bookId: String) async -> [Annotation] {
        currentError = nil

        do {
            return try await rustCLIClient.annotations(for: bookId)
        } catch {
            currentError = error
            return []
        }
    }
    
    func exportToMarkdown(book: Book, annotations: [Annotation], outputURL: URL) async throws {
        isBusy = true
        defer { isBusy = false }

        if annotations.count == book.totalAnnotations {
            _ = try await rustCLIClient.export(
                assetID: book.id,
                outputDirectory: outputURL,
                format: "obsidian",
                overwrite: false
            )
            return
        }

        // The machine export contract currently exports a whole asset and
        // intentionally has no annotation-selection argument.  Preserve the
        // AppKit "current filter" action locally until that contract gains a
        // selection form; full-book exports above still use the canonical Rust
        // exporter and its overwrite/error semantics.
        let content = markdownContent(for: book, annotations: annotations)
        let fileURL = outputURL.appendingPathComponent("\(sanitizedTitle(book.title)).md")
        try content.write(to: fileURL, atomically: true, encoding: .utf8)
    }

    func markdownContent(for book: Book, annotations: [Annotation]) -> String {
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

        return content
    }
    
    private func sanitizedTitle(_ title: String) -> String {
        let invalidChars = CharacterSet(charactersIn: "/\\:*?\"<>|")
        return title.components(separatedBy: invalidChars).joined(separator: "-")
    }
    
    private func formatDate(_ date: Date?) -> String {
        guard let date else { return "未知时间" }
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd HH:mm"
        return formatter.string(from: date)
    }
}
