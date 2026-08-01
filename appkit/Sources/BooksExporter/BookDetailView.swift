import AppKit

final class BookDetailView: NSView, NSTableViewDataSource, NSTableViewDelegate {
    private let titleLabel = NSTextField(labelWithString: "选择一本书")
    private let authorLabel = NSTextField(labelWithString: "")
    private let statsLabel = NSTextField(labelWithString: "")
    private let statusLabel = NSTextField(labelWithString: "从左侧列表选择一本书")
    private let tableView = NSTableView()
    private let scrollView = NSScrollView()
    private let exportButton = NSButton(title: "导出 Markdown", target: nil, action: nil)
    private let copyButton = NSButton(title: "复制 Markdown", target: nil, action: nil)
    private var isExporting = false
    private var isCopying = false

    var onExportRequested: (() -> Void)?
    var onCopyRequested: (() -> Void)?

    var annotations: [Annotation] = [] {
        didSet {
            tableView.reloadData()
            statusLabel.stringValue = annotations.isEmpty ? "没有找到笔记" : "共 \(annotations.count) 条笔记"
            exportButton.isEnabled = !isExporting && !annotations.isEmpty
            copyButton.isEnabled = !isCopying && !annotations.isEmpty
        }
    }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        configureView()
    }

    required init?(coder: NSCoder) {
        fatalError("不支持从 Interface Builder 加载")
    }

    func show(book: Book) {
        titleLabel.stringValue = book.title
        authorLabel.stringValue = "作者：\(book.author)"
        statsLabel.stringValue = "高亮 \(book.highlightsCount)  ·  标注 \(book.annotationsCount)  ·  笔记 \(book.notesCount)  ·  书签 \(book.bookmarksCount)"
        annotations = []
        statusLabel.stringValue = "正在读取笔记…"
    }

    func setError(_ error: Error) {
        statusLabel.stringValue = "读取失败：\(error.localizedDescription)"
    }

    func setExporting(_ exporting: Bool) {
        isExporting = exporting
        exportButton.title = exporting ? "正在导出…" : "导出 Markdown"
        exportButton.isEnabled = !exporting && !annotations.isEmpty
    }

    func setCopying(_ copying: Bool) {
        isCopying = copying
        copyButton.title = copying ? "正在复制…" : "复制 Markdown"
        copyButton.isEnabled = !copying && !annotations.isEmpty
    }

    @objc private func exportRequested() {
        onExportRequested?()
    }

    @objc private func copyRequested() {
        onCopyRequested?()
    }

    private func configureView() {
        titleLabel.font = .systemFont(ofSize: 20, weight: .bold)
        titleLabel.lineBreakMode = .byTruncatingTail
        authorLabel.textColor = .secondaryLabelColor
        statsLabel.textColor = .secondaryLabelColor
        statusLabel.textColor = .secondaryLabelColor

        let annotationColumn = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("annotation"))
        annotationColumn.title = "笔记"
        tableView.addTableColumn(annotationColumn)
        tableView.headerView = nil
        tableView.delegate = self
        tableView.dataSource = self
        tableView.gridStyleMask = .solidHorizontalGridLineMask
        tableView.gridColor = .separatorColor
        tableView.rowHeight = 64

        scrollView.translatesAutoresizingMaskIntoConstraints = false
        scrollView.documentView = tableView
        scrollView.hasVerticalScroller = true
        scrollView.borderType = .bezelBorder

        exportButton.translatesAutoresizingMaskIntoConstraints = false
        exportButton.target = self
        exportButton.action = #selector(exportRequested)
        exportButton.isEnabled = false

        copyButton.translatesAutoresizingMaskIntoConstraints = false
        copyButton.target = self
        copyButton.action = #selector(copyRequested)
        copyButton.isEnabled = false

        let header = NSStackView(views: [titleLabel, authorLabel, statsLabel, statusLabel])
        header.orientation = .vertical
        header.alignment = .leading
        header.spacing = 6
        header.translatesAutoresizingMaskIntoConstraints = false

        let buttons = NSStackView(views: [exportButton, copyButton])
        buttons.orientation = .horizontal
        buttons.alignment = .trailing
        buttons.spacing = 8
        buttons.translatesAutoresizingMaskIntoConstraints = false

        addSubview(header)
        addSubview(scrollView)
        addSubview(buttons)

        NSLayoutConstraint.activate([
            header.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            header.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
            header.topAnchor.constraint(equalTo: topAnchor, constant: 16),
            scrollView.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            scrollView.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
            scrollView.topAnchor.constraint(equalTo: header.bottomAnchor, constant: 12),
            scrollView.bottomAnchor.constraint(equalTo: buttons.topAnchor, constant: -12),
            buttons.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
            buttons.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -16)
        ])
    }

    func numberOfRows(in tableView: NSTableView) -> Int {
        annotations.count
    }

    func tableView(_ tableView: NSTableView, heightOfRow row: Int) -> CGFloat {
        64
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let annotation = annotations[row]
        var text = annotation.displayLocation
        if let content = annotation.contentText, !content.isEmpty {
            text += "\n\(content)"
        }
        if let note = annotation.noteText, !note.isEmpty {
            text += "\n笔记：\(note)"
        }

        let label = NSTextField(wrappingLabelWithString: text)
        label.font = .systemFont(ofSize: 12)
        label.lineBreakMode = .byTruncatingTail
        label.maximumNumberOfLines = 3
        return label
    }
}
