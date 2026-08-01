import AppKit

final class BookListView: NSView, NSTableViewDataSource, NSTableViewDelegate {
    private let statusLabel = NSTextField(labelWithString: "正在读取 Apple Books…")
    private let tableView = NSTableView()
    private let scrollView = NSScrollView()

    var onSelectionChanged: ((Book?) -> Void)?

    var books: [Book] = [] {
        didSet {
            tableView.reloadData()
            statusLabel.stringValue = books.isEmpty ? "没有找到带笔记的书籍" : "共 \(books.count) 本书"
        }
    }

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

    private func configureView() {
        statusLabel.translatesAutoresizingMaskIntoConstraints = false
        statusLabel.textColor = .secondaryLabelColor

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
        addSubview(scrollView)

        NSLayoutConstraint.activate([
            statusLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            statusLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
            statusLabel.topAnchor.constraint(equalTo: topAnchor, constant: 16),
            scrollView.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            scrollView.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
            scrollView.topAnchor.constraint(equalTo: statusLabel.bottomAnchor, constant: 10),
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
