import AppKit

final class BookListView: NSView, NSTableViewDataSource, NSTableViewDelegate, NSSearchFieldDelegate {
    private let statusLabel = NSTextField(labelWithString: "正在读取 Apple Books…")
    private let searchField = NSSearchField()
    private let tableView = NSTableView()
    private let scrollView = NSScrollView()

    var onSelectionChanged: ((Book?) -> Void)?

    var books: [Book] = [] {
        didSet {
            tableView.reloadData()
            statusLabel.stringValue = statusText()
        }
    }

    private var allBooks: [Book] = []
    private var sortColumn: String?
    private var sortAscending: Bool = true

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        configureView()
    }

    required init?(coder: NSCoder) {
        fatalError("不支持从 Interface Builder 加载")
    }

    func setLoading(_ loading: Bool) {
        statusLabel.stringValue = loading ? "正在读取 Apple Books…" : statusLabel.stringValue
    }

    func setError(_ error: Error) {
        statusLabel.stringValue = "读取失败：\(error.localizedDescription)"
    }

    func setBooks(_ books: [Book]) {
        allBooks = books
        applyFilter()
    }

    private func statusText() -> String {
        let trimmedQuery = searchField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmedQuery.isEmpty {
            return books.isEmpty ? "没有找到带笔记的书籍" : "共 \(books.count) 本书"
        }
        return "找到 \(books.count) 本书"
    }

    private func applyFilter() {
        let raw = searchField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let filtered = raw.isEmpty
            ? allBooks
            : allBooks.filter {
                $0.title.lowercased().contains(raw) || $0.author.lowercased().contains(raw)
            }
        books = BookListSorter.sort(filtered, by: sortColumn, ascending: sortAscending)
        if tableView.selectedRow >= books.count {
            tableView.deselectAll(nil)
            onSelectionChanged?(nil)
        }
    }

    func controlTextDidChange(_ obj: Notification) {
        applyFilter()
    }

    func tableView(_ tableView: NSTableView, didClick tableColumn: NSTableColumn) {
        let id = tableColumn.identifier.rawValue
        if sortColumn == id {
            sortAscending.toggle()
        } else {
            sortColumn = id
            sortAscending = true
        }
        updateSortIndicators()
        tableView.deselectAll(nil)
        onSelectionChanged?(nil)
        applyFilter()
    }

    private func updateSortIndicators() {
        for column in tableView.tableColumns {
            let id = column.identifier.rawValue
            if id == sortColumn {
                let symbol = sortAscending ? "arrow.up" : "arrow.down"
                tableView.setIndicatorImage(NSImage(systemSymbolName: symbol, accessibilityDescription: nil), in: column)
            } else {
                tableView.setIndicatorImage(nil, in: column)
            }
        }
    }

    private func configureView() {
        statusLabel.translatesAutoresizingMaskIntoConstraints = false
        statusLabel.textColor = .secondaryLabelColor

        searchField.translatesAutoresizingMaskIntoConstraints = false
        searchField.delegate = self
        searchField.placeholderString = "搜索书名或作者"

        let bookColumn = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("book"))
        bookColumn.title = "书名"
        bookColumn.width = 240

        let countColumn = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("count"))
        countColumn.title = "笔记"
        countColumn.width = 70

        tableView.addTableColumn(bookColumn)
        tableView.addTableColumn(countColumn)
        tableView.headerView = NSTableHeaderView()
        tableView.delegate = self
        tableView.dataSource = self
        tableView.usesAlternatingRowBackgroundColors = true
        tableView.rowHeight = 28

        scrollView.translatesAutoresizingMaskIntoConstraints = false
        scrollView.documentView = tableView
        scrollView.hasVerticalScroller = true
        scrollView.borderType = .bezelBorder

        addSubview(statusLabel)
        addSubview(searchField)
        addSubview(scrollView)

        NSLayoutConstraint.activate([
            statusLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            statusLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
            statusLabel.topAnchor.constraint(equalTo: topAnchor, constant: 16),
            searchField.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            searchField.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
            searchField.topAnchor.constraint(equalTo: statusLabel.bottomAnchor, constant: 8),
            scrollView.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            scrollView.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
            scrollView.topAnchor.constraint(equalTo: searchField.bottomAnchor, constant: 8),
            scrollView.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -16)
        ])
    }

    func numberOfRows(in tableView: NSTableView) -> Int {
        books.count
    }

    func tableViewSelectionDidChange(_ notification: Notification) {
        let row = tableView.selectedRow
        onSelectionChanged?(books.indices.contains(row) ? books[row] : nil)
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let book = books[row]
        let text: String

        switch tableColumn?.identifier.rawValue {
        case "book": text = book.title
        case "count": text = book.displayTotalCount
        default: text = ""
        }

        return NSTextField(labelWithString: text)
    }
}