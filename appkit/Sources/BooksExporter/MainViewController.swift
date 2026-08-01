import AppKit

final class MainViewController: NSViewController {
    private let splitView = NSSplitView()
    private let bookListViewController = BookListViewController()
    private let bookDetailViewController = BookDetailViewController()
    private var didSetInitialPosition = false

    override func loadView() {
        splitView.isVertical = true
        splitView.dividerStyle = .thin

        view = splitView

        addChild(bookListViewController)
        addChild(bookDetailViewController)

        splitView.addSubview(bookListViewController.view)
        splitView.addSubview(bookDetailViewController.view)
    }

    override func viewDidAppear() {
        super.viewDidAppear()
        if !didSetInitialPosition && splitView.subviews.count == 2 {
            splitView.setPosition(420, ofDividerAt: 0)
            didSetInitialPosition = true
        }
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        bookListViewController.onBookSelected = { [weak self] book in
            self?.bookDetailViewController.show(book: book)
        }
        bookListViewController.onExportAllRequested = { [weak self] books in
            self?.exportAllBooks(books)
        }
    }

    private func exportAllBooks(_ books: [Book]) {
        guard !books.isEmpty else { return }
        guard let window = view.window else { return }

        let panel = NSOpenPanel()
        panel.title = "选择批量导出目录"
        panel.prompt = "导出全部"
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.beginSheetModal(for: window) { [weak self] response in
            guard response == .OK, let directoryURL = panel.url else { return }
            self?.runExportAllBooks(books, to: directoryURL)
        }
    }

    private func runExportAllBooks(_ books: [Book], to directoryURL: URL) {
        let bookService = BookService()
        Task { @MainActor [weak self] in
            guard let self else { return }
            var succeeded = 0
            var failed = 0
            var lastError: Error?

            for book in books {
                let annotations = await bookService.getAnnotations(for: book.id)
                do {
                    try await bookService.exportToMarkdown(book: book, annotations: annotations, outputURL: directoryURL)
                    succeeded += 1
                } catch {
                    failed += 1
                    lastError = error
                }
            }

            let message: String
            var details: String
            if failed == 0 {
                message = "导出完成"
                details = "共成功导出 \(succeeded) 本书到：\(directoryURL.path)"
            } else {
                message = "部分导出失败"
                details = "成功 \(succeeded) 本，失败 \(failed) 本。\n目录：\(directoryURL.path)"
                if let error = lastError {
                    details += "\n最后错误：\(error.localizedDescription)"
                }
            }

            showAlert(message: message, details: details)
        }
    }

    private func showAlert(message: String, details: String) {
        let alert = NSAlert()
        alert.messageText = message
        alert.informativeText = details
        alert.alertStyle = message == "导出完成" ? .informational : .warning

        if let window = view.window {
            alert.beginSheetModal(for: window)
        } else {
            alert.runModal()
        }
    }
}