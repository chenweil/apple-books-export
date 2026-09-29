import AppKit

final class BookListViewController: NSViewController {
    private let bookService = BookService()
    private let bookListView = BookListView()
    private var hasLoadedBooks = false
    private var hasShownPermissionAlert = false
    private var loadTask: Task<Void, Never>?

    var onBookSelected: ((Book) -> Void)?
    var onExportAllRequested: (([Book]) -> Void)?
    var onBooksLoaded: (([Book]) -> Void)?

    deinit {
        loadTask?.cancel()
    }

    override func loadView() {
        view = bookListView
    }

    override func viewDidAppear() {
        super.viewDidAppear()
        bookListView.onSelectionChanged = { [weak self] book in
            guard let book else { return }
            self?.onBookSelected?(book)
        }
        bookListView.onExportAllRequested = { [weak self] in
            guard let self else { return }
            self.onExportAllRequested?(self.bookListView.books)
        }

        guard !hasLoadedBooks else { return }
        reload()
    }

    func reload() {
        hasLoadedBooks = true
        loadBooks()
    }

    private func loadBooks() {
        loadTask?.cancel()
        bookListView.setLoading(true)
        let window = view.window

        loadTask = Task { @MainActor [weak self] in
            guard let self else { return }
            let books = await bookService.listBooks()
            guard !Task.isCancelled else { return }
            bookListView.setBooks(books)
            onBooksLoaded?(books)
            bookListView.setLoading(false)
            if let error = bookService.currentError {
                let retry: Bool
                if let dbError = error as? DatabaseError,
                   case .openFailed = dbError,
                   !hasShownPermissionAlert {
                    hasShownPermissionAlert = true
                    retry = presentFullDiskAccessAlertIfNeeded(for: window)
                } else if let rustError = error as? RustCLIError,
                          rustError.stableCode == "FULL_DISK_ACCESS_REQUIRED",
                          !hasShownPermissionAlert {
                    hasShownPermissionAlert = true
                    retry = presentFullDiskAccessAlertIfNeeded(for: window)
                } else {
                    retry = false
                }
                if retry {
                    reload()
                } else {
                    bookListView.setError(error)
                }
            }
        }
    }
}
