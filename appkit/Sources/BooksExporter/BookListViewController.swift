import AppKit

final class BookListViewController: NSViewController {
    private let bookService = BookService()
    private let bookListView = BookListView()
    private var hasLoadedBooks = false

    override func loadView() {
        view = bookListView
    }

    override func viewDidAppear() {
        super.viewDidAppear()
        guard !hasLoadedBooks else { return }
        hasLoadedBooks = true
        loadBooks()
    }

    private func loadBooks() {
        bookListView.setLoading(true)

        Task { @MainActor [weak self] in
            guard let self else { return }
            let books = await bookService.listBooks()
            bookListView.books = books
            if let error = bookService.currentError {
                bookListView.setError(error)
            }
        }
    }
}
