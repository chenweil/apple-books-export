import AppKit

final class BookDetailViewController: NSViewController {
    private let bookService = BookService()
    private let detailView = BookDetailView()
    private var selectedBook: Book?
    private var loadTask: Task<Void, Never>?
    private var exportTask: Task<Void, Never>?
    private var copyTask: Task<Void, Never>?

    deinit {
        loadTask?.cancel()
        exportTask?.cancel()
        copyTask?.cancel()
    }

    override func loadView() {
        detailView.onExportRequested = { [weak self] annotations in
            self?.chooseExportDirectory(for: annotations)
        }
        detailView.onCopyRequested = { [weak self] annotations in
            self?.copySelectedBook(annotations)
        }
        detailView.onCardRequested = { [weak self] annotation in
            self?.openShareCardEditor(for: annotation)
        }
        view = detailView
    }

    func show(book: Book) {
        selectedBook = book
        loadTask?.cancel()
        detailView.show(book: book)

        loadTask = Task { @MainActor [weak self] in
            guard let self else { return }
            let annotations = await bookService.getAnnotations(for: book.id)
            guard !Task.isCancelled else { return }
            detailView.setAnnotations(annotations)
            if let error = bookService.currentError {
                detailView.setError(error)
            }
        }
    }

    func show(book: Book, annotations: [Annotation]) {
        selectedBook = book
        loadTask?.cancel()
        detailView.show(book: book)
        detailView.setAnnotations(annotations)
    }

    func refresh(using books: [Book]) {
        guard let selectedBook,
              let refreshedBook = books.first(where: { $0.id == selectedBook.id }) else {
            return
        }
        show(book: refreshedBook)
    }

    private func chooseExportDirectory(for annotations: [Annotation]) {
        guard let window = view.window, !annotations.isEmpty else { return }

        let panel = NSOpenPanel()
        panel.title = "选择导出目录（\(annotations.count) 条）"
        panel.prompt = "导出"
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.beginSheetModal(for: window) { [weak self] response in
            guard response == .OK, let directoryURL = panel.url else { return }
            self?.exportSelectedBook(annotations, to: directoryURL)
        }
    }

    private func exportSelectedBook(_ annotations: [Annotation], to directoryURL: URL) {
        guard let book = selectedBook, !annotations.isEmpty else { return }

        exportTask?.cancel()
        detailView.setExporting(true)

        exportTask = Task { @MainActor [weak self] in
            guard let self else { return }
            defer { detailView.setExporting(false) }

            do {
                try await bookService.exportToMarkdown(
                    book: book,
                    annotations: annotations,
                    outputURL: directoryURL
                )
                guard !Task.isCancelled else { return }
                showAlert(
                    message: "导出完成",
                    details: "已导出 \(annotations.count) 条到：\(directoryURL.path)"
                )
            } catch {
                showAlert(message: "导出失败", details: error.localizedDescription)
            }
        }
    }

    private func copySelectedBook(_ annotations: [Annotation]) {
        guard let book = selectedBook, !annotations.isEmpty else { return }

        copyTask?.cancel()
        detailView.setCopying(true)
        let content = bookService.markdownContent(for: book, annotations: annotations)

        copyTask = Task { @MainActor [weak self] in
            guard let self else { return }
            defer { detailView.setCopying(false) }

            let pasteboard = NSPasteboard.general
            pasteboard.clearContents()
            pasteboard.setString(content, forType: .string)
            guard !Task.isCancelled else { return }
            showAlert(message: "已复制", details: "已复制 \(annotations.count) 条 Markdown 到剪贴板")
        }
    }

    private func openShareCardEditor(for annotation: Annotation) {
        guard let book = selectedBook else { return }
        let editor = ShareCardEditorViewController(book: book, annotation: annotation)
        presentAsSheet(editor)
    }

    private func showAlert(message: String, details: String) {
        let alert = NSAlert()
        alert.messageText = message
        alert.informativeText = details
        alert.alertStyle = message == "导出完成" || message == "已复制" ? .informational : .warning

        if let window = view.window {
            alert.beginSheetModal(for: window)
        } else {
            alert.runModal()
        }
    }
}
