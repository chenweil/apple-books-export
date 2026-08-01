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
    private let contentWidthLimit: CGFloat = 720

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
        statsLabel.font = .monospacedDigitSystemFont(ofSize: NSFont.systemFontSize, weight: .regular)
        statusLabel.textColor = .secondaryLabelColor

        let annotationColumn = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("annotation"))
        annotationColumn.title = "笔记"
        tableView.addTableColumn(annotationColumn)
        tableView.headerView = nil
        tableView.delegate = self
        tableView.dataSource = self
        tableView.gridStyleMask = .solidHorizontalGridLineMask
        tableView.gridColor = .separatorColor
        tableView.usesAutomaticRowHeights = true

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

        // 主操作靠右,符合 macOS 惯例;水平 stack 的 alignment 必须是 Y 轴属性,
        // 设成 .trailing 会把两个按钮压到同一 x 上纵向堆叠。
        let buttons = NSStackView(views: [copyButton, exportButton])
        buttons.orientation = .horizontal
        buttons.alignment = .centerY
        buttons.spacing = 8
        buttons.translatesAutoresizingMaskIntoConstraints = false

        let content = NSView()
        content.translatesAutoresizingMaskIntoConstraints = false
        addSubview(content)
        content.addSubview(header)
        content.addSubview(scrollView)
        addSubview(buttons)

        // 原先 leading/trailing 都用 == 再叠加 width <= 720,三者互斥,
        // 上限被静默丢弃(实测 747pt)。改成居中 + 两侧至少 16pt + 上限。
        let preferredWidth = content.widthAnchor.constraint(equalToConstant: contentWidthLimit)
        preferredWidth.priority = .defaultHigh

        NSLayoutConstraint.activate([
            content.centerXAnchor.constraint(equalTo: centerXAnchor),
            content.leadingAnchor.constraint(greaterThanOrEqualTo: leadingAnchor, constant: 16),
            content.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -16),
            content.widthAnchor.constraint(lessThanOrEqualToConstant: contentWidthLimit),
            preferredWidth,
            content.topAnchor.constraint(equalTo: topAnchor, constant: 16),
            content.bottomAnchor.constraint(equalTo: buttons.topAnchor, constant: -12),

            header.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            header.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            header.topAnchor.constraint(equalTo: content.topAnchor),

            scrollView.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            scrollView.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            scrollView.topAnchor.constraint(equalTo: header.bottomAnchor, constant: 12),
            scrollView.bottomAnchor.constraint(equalTo: content.bottomAnchor),

            buttons.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            buttons.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -16)
        ])
    }

    func numberOfRows(in tableView: NSTableView) -> Int {
        annotations.count
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let cell = tableView.makeView(withIdentifier: AnnotationCellView.reuseIdentifier, owner: self) as? AnnotationCellView
            ?? AnnotationCellView(frame: .zero)
        cell.updateLayoutWidth(tableColumn?.width ?? tableView.bounds.width)
        cell.configure(with: annotations[row])
        return cell
    }
}
