import AppKit

final class BookListViewController: NSViewController {
    private let bookService = BookService()
    private let bookListView = BookListView()
    private var hasLoadedBooks = false

    var onBookSelected: ((Book) -> Void)?

    override func loadView() {
        view = bookListView
    }

    override func viewDidAppear() {
        super.viewDidAppear()
        bookListView.onSelectionChanged = { [weak self] book in
            guard let book else { return }
            self?.onBookSelected?(book)
        }

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
