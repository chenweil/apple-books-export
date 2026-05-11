import Foundation

@Observable
class BookListViewModel {
    var books: [Book] = []
    var isLoading = false
    var selectedBook: Book?
    var currentError: Error?
    
    private let bookService: BookService
    
    init(bookService: BookService) {
        self.bookService = bookService
    }
    
    func loadBooks() async {
        isLoading = true
        currentError = nil
        books = await bookService.listBooks()
        currentError = bookService.currentError
        isLoading = false
    }
    
    func selectBook(_ book: Book) {
        selectedBook = book
    }
}
