import AppKit

final class BookDetailView: NSView, NSTableViewDataSource, NSTableViewDelegate {
    private let titleLabel = NSTextField(labelWithString: "选择一本书")
    private let authorLabel = NSTextField(labelWithString: "")
    private let typeFilter = NSSegmentedControl()
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

    private var book: Book?
    private var allAnnotations: [Annotation] = []
    private var filter: AnnotationFilter = .all

    /// 当前展示的(已按类型筛选的)笔记。导出与复制都以它为准 —— 所见即所导。
    private(set) var annotations: [Annotation] = [] {
        didSet {
            tableView.reloadData()
            statusLabel.stringValue = statusText()
            updateActionButtons()
        }
    }

    func setAnnotations(_ annotations: [Annotation]) {
        allAnnotations = annotations
        applyFilter()
    }

    private func applyFilter() {
        annotations = filter.apply(to: allAnnotations)
    }

    private func statusText() -> String {
        guard !allAnnotations.isEmpty else { return "没有找到笔记" }
        switch filter {
        case .all:
            return "共 \(annotations.count) 条笔记"
        case .type(let type):
            return annotations.isEmpty
                ? "这本书没有\(type.shortName)"
                : "\(annotations.count) 条\(type.shortName)"
        }
    }

    /// 按钮标题跟着当前展示的条数走 —— 筛选之后仍写「导出 Markdown」
    /// 会让人以为导的是全书,跟书单页的「导出当前 N 本」是同一类问题。
    private func updateActionButtons() {
        let count = annotations.count
        exportButton.title = isExporting
            ? "正在导出…"
            : (count == 0 ? "导出 Markdown" : "导出当前 \(count) 条")
        copyButton.title = isCopying
            ? "正在复制…"
            : (count == 0 ? "复制 Markdown" : "复制当前 \(count) 条")
        exportButton.isEnabled = !isExporting && count > 0
        copyButton.isEnabled = !isCopying && count > 0
    }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        configureView()
    }

    required init?(coder: NSCoder) {
        fatalError("不支持从 Interface Builder 加载")
    }

    func show(book: Book) {
        self.book = book
        titleLabel.stringValue = book.title
        authorLabel.stringValue = "作者：\(book.author)"

        // 换书重置筛选,否则新书会沿用上一本的筛选却没有任何提示。
        filter = .all
        typeFilter.selectedSegment = 0
        updateFilterTitles(for: book)

        allAnnotations = []
        annotations = []
        statusLabel.stringValue = "正在读取笔记…"
    }

    private func updateFilterTitles(for book: Book) {
        for (index, option) in AnnotationFilter.ordered.enumerated() {
            typeFilter.setLabel(option.title(for: book), forSegment: index)
            typeFilter.setEnabled(true, forSegment: index)
        }
    }

    @objc private func filterChanged() {
        let index = typeFilter.selectedSegment
        guard AnnotationFilter.ordered.indices.contains(index) else { return }
        filter = AnnotationFilter.ordered[index]
        applyFilter()
    }

    func setError(_ error: Error) {
        statusLabel.stringValue = "读取失败：\(error.localizedDescription)"
    }

    func setExporting(_ exporting: Bool) {
        isExporting = exporting
        updateActionButtons()
    }

    func setCopying(_ copying: Bool) {
        isCopying = copying
        updateActionButtons()
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
        statusLabel.textColor = .secondaryLabelColor

        typeFilter.segmentStyle = .automatic
        typeFilter.trackingMode = .selectOne
        typeFilter.segmentCount = AnnotationFilter.ordered.count
        typeFilter.font = .monospacedDigitSystemFont(ofSize: NSFont.systemFontSize, weight: .regular)
        typeFilter.target = self
        typeFilter.action = #selector(filterChanged)
        for (index, option) in AnnotationFilter.ordered.enumerated() {
            typeFilter.setLabel(option.placeholderTitle, forSegment: index)
        }
        typeFilter.selectedSegment = 0

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

        let header = NSStackView(views: [titleLabel, authorLabel, typeFilter, statusLabel])
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
