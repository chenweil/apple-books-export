import AppKit

final class BookListView: NSView, NSTableViewDataSource, NSTableViewDelegate, NSSearchFieldDelegate {
    private let statusLabel = NSTextField(labelWithString: "正在读取 Apple Books…")
    private let searchField = NSSearchField()
    private let tableView = NSTableView()
    private let scrollView = NSScrollView()
    private let exportAllButton = NSButton(title: "导出 Markdown", target: nil, action: nil)

    var onSelectionChanged: ((Book?) -> Void)?
    var onExportAllRequested: (() -> Void)?

    var books: [Book] = [] {
        didSet {
            tableView.reloadData()
            statusLabel.stringValue = statusText()
            updateExportAllButton()
        }
    }

    private var allBooks: [Book] = []
    private var sortColumn: BookColumn? = nil {
        didSet { UserDefaults.standard.set(sortColumn?.rawValue, forKey: BookListView.sortColumnKey) }
    }
    private var sortAscending: Bool = true {
        didSet { UserDefaults.standard.set(sortAscending, forKey: BookListView.sortAscendingKey) }
    }
    private var isLoading = false {
        didSet {
            searchField.isEnabled = !isLoading
            tableView.isEnabled = !isLoading
            statusLabel.stringValue = isLoading ? "正在读取 Apple Books…" : statusText()
            updateExportAllButton()
        }
    }

    /// 按钮导出的是当前(可能已过滤的)结果,标题必须跟着结果数走,
    /// 否则搜索后仍写「全部」就是在误导一次不可撤销的批量写盘。
    private func updateExportAllButton() {
        exportAllButton.title = books.isEmpty ? "导出 Markdown" : "导出当前 \(books.count) 本"
        exportAllButton.isEnabled = !isLoading && !books.isEmpty
    }

    static let sortColumnKey = "appkit.sortColumn"
    static let sortAscendingKey = "appkit.sortAscending"

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        if let saved = UserDefaults.standard.string(forKey: BookListView.sortColumnKey),
           let column = BookColumn(rawValue: saved) {
            sortColumn = column
            sortAscending = UserDefaults.standard.bool(forKey: BookListView.sortAscendingKey)
        }
        configureView()

        // 排序偏好是从 UserDefaults 恢复的,这里补画一次,
        // 否则启动后数据已排序但表头没有任何状态。
        syncSortHeader(announce: false)
    }

    required init?(coder: NSCoder) {
        fatalError("不支持从 Interface Builder 加载")
    }

    func setLoading(_ loading: Bool) {
        isLoading = loading
    }

    func setError(_ error: Error) {
        statusLabel.stringValue = "读取失败：\(error.localizedDescription)"
    }

    func setBooks(_ books: [Book]) {
        allBooks = books
        applyFilter()
    }

    private func statusText() -> String {
        let query = searchField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        if query.isEmpty {
            return books.isEmpty
                ? "没有带笔记的书籍。在 Apple Books 里高亮或写下笔记后再回到这里。"
                : "共 \(books.count) 本书"
        }
        return books.isEmpty
            ? "没有匹配「\(query)」的书 · 清空搜索框查看全部"
            : "找到 \(books.count) 本书"
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
        guard let clicked = BookColumn(rawValue: tableColumn.identifier.rawValue) else { return }

        if sortColumn == clicked {
            if sortAscending {
                sortAscending = false
            } else {
                sortColumn = nil
                sortAscending = true
            }
        } else {
            sortColumn = clicked
            sortAscending = true
        }
        syncSortHeader(announce: true)
        tableView.deselectAll(nil)
        onSelectionChanged?(nil)
        applyFilter()
    }

    /// 三态循环是手写的,所以 AXSortDirection 也必须手写 —— NSTableView 原生的
    /// sortDescriptorPrototype 表头只有升/降两态,套上去会丢掉「无排序」这一态。
    /// VoiceOver 读的是表头 proxy 的 AXSortDirection;指示图标只是视觉通道,
    /// 它的 accessibilityDescription 不会被 AX 客户端读到(实测为 nil)。
    private func syncSortHeader(announce: Bool) {
        for column in tableView.tableColumns {
            let isSorted = BookColumn(rawValue: column.identifier.rawValue) == sortColumn

            if isSorted {
                let symbol = sortAscending ? "arrow.up" : "arrow.down"
                tableView.setIndicatorImage(
                    NSImage(systemSymbolName: symbol, accessibilityDescription: sortDescription),
                    in: column
                )
                column.headerCell.setAccessibilitySortDirection(sortAscending ? .ascending : .descending)
            } else {
                tableView.setIndicatorImage(nil, in: column)
                column.headerCell.setAccessibilitySortDirection(.unknown)
            }
        }

        // AXSortDirection 只有主动查询才读得到。用户点完表头焦点还停在表头上,
        // 不播报的话状态变化是无声的。
        guard announce else { return }
        NSAccessibility.post(
            element: NSApp as Any,
            notification: .announcementRequested,
            userInfo: [
                .announcement: sortColumn == nil ? "已取消排序" : "\(sortColumn!.title),\(sortDescription)",
                .priority: NSAccessibilityPriorityLevel.medium.rawValue
            ]
        )
    }

    private var sortDescription: String {
        sortAscending ? "已按升序排序" : "已按降序排序"
    }

    private func configureView() {
        statusLabel.translatesAutoresizingMaskIntoConstraints = false
        statusLabel.textColor = .secondaryLabelColor

        searchField.translatesAutoresizingMaskIntoConstraints = false
        searchField.delegate = self
        searchField.placeholderString = "搜索书名或作者"

        let bookColumn = NSTableColumn(identifier: NSUserInterfaceItemIdentifier(BookColumn.book.rawValue))
        bookColumn.title = BookColumn.book.title
        bookColumn.width = 240
        bookColumn.resizingMask = .autoresizingMask

        let countColumn = NSTableColumn(identifier: NSUserInterfaceItemIdentifier(BookColumn.count.rawValue))
        countColumn.title = BookColumn.count.title
        countColumn.width = 80

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

        exportAllButton.translatesAutoresizingMaskIntoConstraints = false
        exportAllButton.target = self
        exportAllButton.action = #selector(exportAllRequested)
        exportAllButton.isEnabled = false

        addSubview(statusLabel)
        addSubview(searchField)
        addSubview(scrollView)
        addSubview(exportAllButton)

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
            scrollView.bottomAnchor.constraint(equalTo: exportAllButton.topAnchor, constant: -12),
            exportAllButton.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            exportAllButton.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
            exportAllButton.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -16)
        ])
    }

    @objc private func exportAllRequested() {
        onExportAllRequested?()
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
        guard let identifier = tableColumn?.identifier,
              let column = BookColumn(rawValue: identifier.rawValue) else { return nil }

        let cell = tableView.makeView(withIdentifier: identifier, owner: self) as? BookCellView
            ?? BookCellView(identifier: identifier, tabularDigits: column == .count)

        switch column {
        case .book: cell.configure(text: book.title)
        case .count: cell.configure(text: book.displayTotalCount)
        }
        return cell
    }
}