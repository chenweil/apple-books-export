import AppKit

final class BookDetailViewController: NSViewController {
    private let bookService = BookService()
    private let detailView = BookDetailView()
    private var loadTask: Task<Void, Never>?

    deinit {
        loadTask?.cancel()
    }

    override func loadView() {
        view = detailView
    }

    func show(book: Book) {
        loadTask?.cancel()
        detailView.show(book: book)

        loadTask = Task { @MainActor [weak self] in
            guard let self else { return }
            let annotations = await bookService.getAnnotations(for: book.id)
            guard !Task.isCancelled else { return }
            detailView.annotations = annotations
            if let error = bookService.currentError {
                detailView.setError(error)
            }
        }
    }
}
