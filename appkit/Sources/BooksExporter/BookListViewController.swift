import AppKit

final class BookListViewController: NSViewController {
    private let bookService = BookService()
    private let bookListView = BookListView()
    private var hasLoadedBooks = false
    private var hasShownPermissionAlert = false

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
        let window = view.window

        Task { @MainActor [weak self] in
            guard let self else { return }
            let books = await bookService.listBooks()
            bookListView.setBooks(books)
            if let error = bookService.currentError {
                let retry: Bool
                if let dbError = error as? DatabaseError,
                   case .openFailed = dbError,
                   !hasShownPermissionAlert {
                    hasShownPermissionAlert = true
                    retry = presentFullDiskAccessAlertIfNeeded(for: window)
                } else {
                    retry = false
                }
                if retry {
                    hasLoadedBooks = false
                    loadBooks()
                } else {
                    bookListView.setError(error)
                }
            }
        }
    }
}
