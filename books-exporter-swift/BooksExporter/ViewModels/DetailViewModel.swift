import Foundation

@Observable
class DetailViewModel {
    var selectedBook: Book?
    var annotations: [Annotation] = []
    var isLoading = false
    var isExporting = false
    var currentError: Error?
    var showPreview = false
    var exportDone = false
    
    private let bookService: BookService
    
    init(bookService: BookService) {
        self.bookService = bookService
    }
    
    func loadAnnotations(for book: Book) async {
        isLoading = true
        currentError = nil
        annotations = await bookService.getAnnotations(for: book.id)
        currentError = bookService.currentError
        isLoading = false
    }
    
    func onBookSelected(_ book: Book) async {
        selectedBook = book
        await loadAnnotations(for: book)
    }
    
    func exportSelectedBook(to url: URL) async {
        guard let book = selectedBook else { return }
        
        isExporting = true
        currentError = nil
        
        do {
            try await bookService.exportToMarkdown(
                book: book,
                annotations: annotations,
                outputURL: url
            )
            exportDone = true
        } catch {
            currentError = error
        }
        
        isExporting = false
    }
    
    func resetExportState() {
        exportDone = false
    }
}
