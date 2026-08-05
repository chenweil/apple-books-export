import AppKit

final class ShareCardEditorViewController: NSViewController, NSTextViewDelegate {
    private static let alternativePreviewSize = NSSize(width: 72, height: 96)
    private static let previewSize = NSSize(width: 360, height: 480)
    private static let thumbnailSize = NSSize(width: 72, height: 96)
    private static let themeThumbnailSize = NSSize(width: 64, height: 86)
    private static let minimumContentSize = NSSize(width: 940, height: 720)
    private static let maximumContentSize = NSSize(width: 1280, height: 960)
    private static let textChangeDebounceInterval: TimeInterval = 0.2

    private let book: Book
    private let annotation: Annotation
    private let service = ShareCardService()
    private var card: ShareCard
    private var alternativeCards: [ShareCard] = []
    private var renderedPageResults: [ShareCardRenderedPage] = []
    private var selectedPageIndex = 0
    private var textChangeTimer: Timer?

    private let previewImageView = NSImageView()
    private let pageLabel = NSTextField(labelWithString: "")
    private let thumbnailScrollView = NSScrollView()
    private let thumbnailStack = NSStackView()
    private let textView = NSTextView()
    private let textScrollView = NSScrollView()
    private let noteCheckbox = NSButton(checkboxWithTitle: "添加笔记", target: nil, action: nil)
    private let themeScrollView = NSScrollView()
    private let themeGridStack = NSStackView()
    private var themeButtons: [NSButton] = []
    private let fontPopup = NSPopUpButton(frame: .zero, pullsDown: false)
    private let sizeModePopup = NSPopUpButton(frame: .zero, pullsDown: false)
    private let fontSizePopup = NSPopUpButton(frame: .zero, pullsDown: false)
    private let horizontalAlignmentControl = NSSegmentedControl()
    private let verticalAlignmentControl = NSSegmentedControl()
    private let changeButton = NSButton(title: "换一换", target: nil, action: nil)
    private let alternativeStack = NSStackView()
    private let folderCheckbox = NSButton(checkboxWithTitle: "保存后打开文件夹", target: nil, action: nil)
    private let copyButton = NSButton(title: "复制图片", target: nil, action: nil)
    private let copyMenuButton = NSPopUpButton(frame: .zero, pullsDown: false)
    private let saveButton = NSButton(title: "保存 PNG", target: nil, action: nil)
    private let airDropButton = NSButton(title: "AirDrop", target: nil, action: nil)
    private let closeButton = NSButton(title: "完成", target: nil, action: nil)
    private let statusLabel = NSTextField(labelWithString: "")
    private let copyHandler: ([NSImage]) -> Bool

    init(
        book: Book,
        annotation: Annotation,
        copyHandler: @escaping ([NSImage]) -> Bool = { images in
            let pasteboard = NSPasteboard.general
            pasteboard.clearContents()
            return pasteboard.writeObjects(images)
        }
    ) {
        self.book = book
        self.annotation = annotation
        self.card = ShareCardService().makeCard(for: book, annotation: annotation)
        self.copyHandler = copyHandler
        super.init(nibName: nil, bundle: nil)
        preferredContentSize = NSSize(width: 980, height: 720)
    }

    required init?(coder: NSCoder) {
        fatalError("不支持从 Interface Builder 加载")
    }

    deinit {
        textChangeTimer?.invalidate()
    }

    override func viewDidAppear() {
        super.viewDidAppear()
        guard let window = view.window else { return }
        window.contentMinSize = Self.minimumContentSize
        window.contentMaxSize = Self.maximumContentSize
    }

    override func loadView() {
        view = makeView()
        textView.string = card.primaryText
        noteCheckbox.isHidden = annotation.contentText?.isEmpty ?? true
        noteCheckbox.isEnabled = !(annotation.noteText?.isEmpty ?? true)
        noteCheckbox.state = .off
        folderCheckbox.state = ShareCardPreferences.openExportFolder ? .on : .off
        reloadThemeGrid()
        syncTypographyControls()
        updatePreview()
    }

    private func makeView() -> NSView {
        let root = NSView()
        root.translatesAutoresizingMaskIntoConstraints = false

        previewImageView.translatesAutoresizingMaskIntoConstraints = false
        previewImageView.imageScaling = .scaleProportionallyUpOrDown
        previewImageView.imageAlignment = .alignCenter
        previewImageView.wantsLayer = true
        previewImageView.layer?.backgroundColor = NSColor.controlBackgroundColor.cgColor
        previewImageView.setAccessibilityLabel("分享卡片预览")
        previewImageView.identifier = NSUserInterfaceItemIdentifier("share-card-preview")

        pageLabel.alignment = .center
        pageLabel.textColor = .secondaryLabelColor
        pageLabel.setAccessibilityLabel("当前卡片页码")
        pageLabel.identifier = NSUserInterfaceItemIdentifier("share-card-page-label")

        thumbnailScrollView.translatesAutoresizingMaskIntoConstraints = false
        thumbnailScrollView.documentView = thumbnailStack
        thumbnailScrollView.hasHorizontalScroller = true
        thumbnailScrollView.hasVerticalScroller = false
        thumbnailScrollView.borderType = .bezelBorder
        thumbnailScrollView.drawsBackground = true
        thumbnailStack.orientation = .horizontal
        thumbnailStack.alignment = .centerY
        thumbnailStack.spacing = 8
        thumbnailStack.translatesAutoresizingMaskIntoConstraints = false

        textView.isRichText = false
        textView.isEditable = true
        textView.isSelectable = true
        textView.font = .systemFont(ofSize: 15)
        textView.textContainerInset = NSSize(width: 8, height: 8)
        textView.delegate = self
        textView.minSize = NSSize(width: 0, height: 0)
        textView.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        textView.textContainer?.widthTracksTextView = true

        textScrollView.translatesAutoresizingMaskIntoConstraints = false
        textScrollView.documentView = textView
        textScrollView.hasVerticalScroller = true
        textScrollView.borderType = .bezelBorder

        noteCheckbox.target = self
        noteCheckbox.action = #selector(noteChanged)

        themeScrollView.translatesAutoresizingMaskIntoConstraints = false
        themeScrollView.hasVerticalScroller = true
        themeScrollView.hasHorizontalScroller = false
        themeScrollView.autohidesScrollers = true
        themeScrollView.borderType = .bezelBorder
        themeScrollView.drawsBackground = true
        themeScrollView.documentView = themeGridStack
        themeGridStack.orientation = .vertical
        themeGridStack.alignment = .leading
        themeGridStack.spacing = 8
        themeGridStack.translatesAutoresizingMaskIntoConstraints = false

        fontPopup.addItems(withTitles: ShareCardFont.allCases.map { $0.displayName })
        fontPopup.selectItem(at: ShareCardFont.allCases.firstIndex(of: card.font) ?? 0)
        fontPopup.target = self
        fontPopup.action = #selector(fontChanged)
        fontPopup.identifier = NSUserInterfaceItemIdentifier("share-card-font")

        sizeModePopup.addItems(withTitles: ShareCardFontSizeMode.allCases.map { $0.displayName })
        sizeModePopup.target = self
        sizeModePopup.action = #selector(sizeModeChanged)
        sizeModePopup.identifier = NSUserInterfaceItemIdentifier("share-card-size-mode")

        fontSizePopup.addItems(withTitles: [42, 48, 56, 64, 72].map(String.init))
        fontSizePopup.target = self
        fontSizePopup.action = #selector(fontSizeChanged)
        fontSizePopup.identifier = NSUserInterfaceItemIdentifier("share-card-font-size")

        horizontalAlignmentControl.segmentCount = ShareCardHorizontalAlignment.allCases.count
        ShareCardHorizontalAlignment.allCases.enumerated().forEach { index, alignment in
            horizontalAlignmentControl.setLabel(alignment.displayName, forSegment: index)
        }
        horizontalAlignmentControl.trackingMode = .selectOne
        horizontalAlignmentControl.target = self
        horizontalAlignmentControl.action = #selector(horizontalAlignmentChanged)
        horizontalAlignmentControl.identifier = NSUserInterfaceItemIdentifier("share-card-horizontal-alignment")
        horizontalAlignmentControl.setAccessibilityLabel("正文水平对齐")

        verticalAlignmentControl.segmentCount = ShareCardVerticalAlignment.allCases.count
        ShareCardVerticalAlignment.allCases.enumerated().forEach { index, alignment in
            verticalAlignmentControl.setLabel(alignment.displayName, forSegment: index)
        }
        verticalAlignmentControl.trackingMode = .selectOne
        verticalAlignmentControl.target = self
        verticalAlignmentControl.action = #selector(verticalAlignmentChanged)
        verticalAlignmentControl.identifier = NSUserInterfaceItemIdentifier("share-card-vertical-alignment")
        verticalAlignmentControl.setAccessibilityLabel("正文垂直对齐")

        changeButton.image = NSImage(
            systemSymbolName: "arrow.triangle.2.circlepath",
            accessibilityDescription: "换一换"
        )
        changeButton.imagePosition = .imageLeading
        changeButton.target = self
        changeButton.action = #selector(changeItUp)

        alternativeStack.orientation = .horizontal
        alternativeStack.alignment = .centerY
        alternativeStack.distribution = .equalSpacing
        alternativeStack.spacing = 8
        alternativeStack.isHidden = true

        folderCheckbox.target = self
        folderCheckbox.action = #selector(folderPreferenceChanged)

        copyButton.image = NSImage(
            systemSymbolName: "doc.on.doc",
            accessibilityDescription: "复制图片"
        )
        copyButton.imagePosition = .imageLeading
        copyButton.target = self
        copyButton.action = #selector(copyRequested)
        copyButton.isEnabled = false

        copyMenuButton.addItem(withTitle: "复制全部页面")
        copyMenuButton.target = self
        copyMenuButton.action = #selector(copyAllRequested)
        copyMenuButton.identifier = NSUserInterfaceItemIdentifier("share-card-copy-menu")
        copyMenuButton.toolTip = "复制全部页面"
        copyMenuButton.isEnabled = false

        saveButton.image = NSImage(
            systemSymbolName: "square.and.arrow.down",
            accessibilityDescription: "保存 PNG"
        )
        saveButton.imagePosition = .imageLeading
        saveButton.keyEquivalent = "\r"
        saveButton.target = self
        saveButton.action = #selector(saveRequested)
        saveButton.isEnabled = false

        airDropButton.image = NSImage(
            systemSymbolName: "square.and.arrow.up",
            accessibilityDescription: "AirDrop"
        )
        airDropButton.imagePosition = .imageLeading
        airDropButton.target = self
        airDropButton.action = #selector(airDropRequested)
        airDropButton.isEnabled = false
        airDropButton.toolTip = "通过 AirDrop 发送当前页"

        closeButton.target = self
        closeButton.action = #selector(closeRequested)

        statusLabel.textColor = .secondaryLabelColor
        statusLabel.lineBreakMode = .byTruncatingTail

        let themeLabel = NSTextField(labelWithString: "主题")
        let themeRow = NSStackView(views: [themeLabel, themeScrollView])
        themeRow.orientation = .horizontal
        themeRow.alignment = .centerY
        themeRow.spacing = 8
        themeLabel.widthAnchor.constraint(equalToConstant: 40).isActive = true

        let fontLabel = NSTextField(labelWithString: "字体")
        let fontRow = NSStackView(views: [fontLabel, fontPopup])
        fontRow.orientation = .horizontal
        fontRow.alignment = .centerY
        fontRow.spacing = 8

        let sizeModeLabel = NSTextField(labelWithString: "字号")
        let sizeRow = NSStackView(views: [sizeModeLabel, sizeModePopup, fontSizePopup])
        sizeRow.orientation = .horizontal
        sizeRow.alignment = .centerY
        sizeRow.spacing = 8

        let horizontalLabel = NSTextField(labelWithString: "水平")
        let horizontalRow = NSStackView(views: [horizontalLabel, horizontalAlignmentControl])
        horizontalRow.orientation = .horizontal
        horizontalRow.alignment = .centerY
        horizontalRow.spacing = 8

        let verticalLabel = NSTextField(labelWithString: "垂直")
        let verticalRow = NSStackView(views: [verticalLabel, verticalAlignmentControl])
        verticalRow.orientation = .horizontal
        verticalRow.alignment = .centerY
        verticalRow.spacing = 8

        let controlsTitle = NSTextField(labelWithString: "卡片文字")
        controlsTitle.font = .systemFont(ofSize: 17, weight: .semibold)

        let actionRow = NSStackView(views: [copyButton, copyMenuButton, saveButton, airDropButton, closeButton])
        actionRow.orientation = .horizontal
        actionRow.alignment = .centerY
        actionRow.spacing = 8

        let controls = NSStackView(
            views: [controlsTitle, textScrollView, noteCheckbox, themeRow, fontRow, changeButton,
                    sizeRow, horizontalRow, verticalRow, alternativeStack, folderCheckbox,
                    statusLabel, actionRow]
        )
        controls.orientation = .vertical
        controls.alignment = .leading
        controls.spacing = 8
        controls.translatesAutoresizingMaskIntoConstraints = false

        let previewColumn = NSStackView(views: [previewImageView, pageLabel, thumbnailScrollView])
        previewColumn.orientation = .vertical
        previewColumn.alignment = .centerX
        previewColumn.spacing = 8
        previewColumn.translatesAutoresizingMaskIntoConstraints = false

        let content = NSStackView(views: [previewColumn, controls])
        content.orientation = .horizontal
        content.alignment = .top
        content.spacing = 24
        content.translatesAutoresizingMaskIntoConstraints = false

        root.addSubview(content)
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 24),
            content.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -24),
            content.topAnchor.constraint(equalTo: root.topAnchor, constant: 24),
            content.bottomAnchor.constraint(equalTo: root.bottomAnchor, constant: -24),

            previewColumn.widthAnchor.constraint(equalToConstant: Self.previewSize.width),
            previewImageView.widthAnchor.constraint(equalToConstant: Self.previewSize.width),
            previewImageView.heightAnchor.constraint(equalTo: previewImageView.widthAnchor, multiplier: 4.0 / 3.0),
            previewImageView.heightAnchor.constraint(equalToConstant: Self.previewSize.height),
            pageLabel.widthAnchor.constraint(equalToConstant: Self.previewSize.width),
            thumbnailScrollView.widthAnchor.constraint(equalToConstant: Self.previewSize.width),
            thumbnailScrollView.heightAnchor.constraint(equalToConstant: 112),
            thumbnailStack.heightAnchor.constraint(equalToConstant: 104),

            controls.widthAnchor.constraint(greaterThanOrEqualToConstant: 400),
            textScrollView.heightAnchor.constraint(equalToConstant: 140),
            textScrollView.widthAnchor.constraint(equalTo: controls.widthAnchor),
            themeRow.widthAnchor.constraint(equalTo: controls.widthAnchor),
            themeScrollView.widthAnchor.constraint(equalTo: controls.widthAnchor, constant: -48),
            themeScrollView.heightAnchor.constraint(equalToConstant: 126),
            themeGridStack.widthAnchor.constraint(equalTo: themeScrollView.contentView.widthAnchor),
            fontRow.widthAnchor.constraint(equalTo: controls.widthAnchor),
            sizeRow.widthAnchor.constraint(equalTo: controls.widthAnchor),
            horizontalRow.widthAnchor.constraint(equalTo: controls.widthAnchor),
            verticalRow.widthAnchor.constraint(equalTo: controls.widthAnchor),
            alternativeStack.widthAnchor.constraint(equalTo: controls.widthAnchor),
            alternativeStack.heightAnchor.constraint(equalToConstant: 104),
            statusLabel.widthAnchor.constraint(equalTo: controls.widthAnchor),
            actionRow.widthAnchor.constraint(equalTo: controls.widthAnchor)
        ])

        return root
    }

    @objc private func noteChanged() {
        updateCardFromEditor(debouncePreview: false)
    }

    @objc private func themeSelected(_ sender: NSButton) {
        guard ShareCardTheme.allCases.indices.contains(sender.tag) else { return }
        cancelPendingPreviewUpdate()
        let theme = ShareCardTheme.allCases[sender.tag]
        clearAlternatives()
        card = service.makeCard(
            for: book,
            annotation: annotation,
            includeNote: noteCheckbox.state == .on,
            textOverride: textView.string,
            theme: theme,
            typography: card.typography
        )
        syncThemeSelection()
        invalidateExport()
        updatePreview()
    }

    @objc private func changeItUp() {
        clearAlternatives()
        alternativeCards = service.alternativeCards(for: card)

        for (index, alternative) in alternativeCards.enumerated() {
            do {
                let previewContainer = NSView()
                previewContainer.translatesAutoresizingMaskIntoConstraints = false
                previewContainer.setContentHuggingPriority(.required, for: .horizontal)
                previewContainer.setContentHuggingPriority(.required, for: .vertical)
                previewContainer.setContentCompressionResistancePriority(.required, for: .horizontal)
                previewContainer.setContentCompressionResistancePriority(.required, for: .vertical)

                let button = NSButton(image: try service.previewImage(for: alternative), target: self, action: #selector(alternativeSelected(_:)))
                button.tag = index
                button.imageScaling = .scaleProportionallyUpOrDown
                button.bezelStyle = .regularSquare
                button.toolTip = "选择候选卡片 \(index + 1)"
                button.setAccessibilityLabel("候选卡片 \(index + 1)")
                button.translatesAutoresizingMaskIntoConstraints = false
                previewContainer.addSubview(button)
                NSLayoutConstraint.activate([
                    previewContainer.widthAnchor.constraint(equalToConstant: Self.alternativePreviewSize.width),
                    previewContainer.heightAnchor.constraint(equalToConstant: Self.alternativePreviewSize.height),
                    button.leadingAnchor.constraint(equalTo: previewContainer.leadingAnchor),
                    button.trailingAnchor.constraint(equalTo: previewContainer.trailingAnchor),
                    button.topAnchor.constraint(equalTo: previewContainer.topAnchor),
                    button.bottomAnchor.constraint(equalTo: previewContainer.bottomAnchor)
                ])
                alternativeStack.addArrangedSubview(previewContainer)
            } catch {
                statusLabel.stringValue = "无法生成候选卡片"
                return
            }
        }
        alternativeStack.isHidden = alternativeCards.isEmpty
    }

    @objc private func fontChanged() {
        guard ShareCardFont.allCases.indices.contains(fontPopup.indexOfSelectedItem) else { return }
        let font = ShareCardFont.allCases[fontPopup.indexOfSelectedItem]
        clearAlternatives()
        updateTypography(updatedTypography(font: font))
    }

    @objc private func sizeModeChanged() {
        guard ShareCardFontSizeMode.allCases.indices.contains(sizeModePopup.indexOfSelectedItem) else { return }
        clearAlternatives()
        updateTypography(updatedTypography(
            sizeMode: ShareCardFontSizeMode.allCases[sizeModePopup.indexOfSelectedItem]
        ))
    }

    @objc private func fontSizeChanged() {
        guard let fontSize = Double(fontSizePopup.titleOfSelectedItem ?? "") else { return }
        clearAlternatives()
        updateTypography(updatedTypography(fontSize: CGFloat(fontSize)))
    }

    @objc private func horizontalAlignmentChanged() {
        guard ShareCardHorizontalAlignment.allCases.indices.contains(horizontalAlignmentControl.selectedSegment) else {
            return
        }
        clearAlternatives()
        updateTypography(updatedTypography(
            horizontalAlignment: ShareCardHorizontalAlignment.allCases[horizontalAlignmentControl.selectedSegment]
        ))
    }

    @objc private func verticalAlignmentChanged() {
        guard ShareCardVerticalAlignment.allCases.indices.contains(verticalAlignmentControl.selectedSegment) else {
            return
        }
        clearAlternatives()
        updateTypography(updatedTypography(
            verticalAlignment: ShareCardVerticalAlignment.allCases[verticalAlignmentControl.selectedSegment]
        ))
    }

    @objc private func alternativeSelected(_ sender: NSButton) {
        guard alternativeCards.indices.contains(sender.tag) else { return }
        cancelPendingPreviewUpdate()
        card = alternativeCards[sender.tag]
        textView.string = card.primaryText
        syncThemeSelection()
        fontPopup.selectItem(at: ShareCardFont.allCases.firstIndex(of: card.font) ?? 0)
        syncTypographyControls()
        invalidateExport()
        updatePreview()
    }

    @objc private func folderPreferenceChanged() {
        UserDefaults.standard.set(
            folderCheckbox.state == .on,
            forKey: ShareCardPreferences.openExportFolderKey
        )
    }

    @objc private func saveRequested() {
        guard !card.primaryText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            statusLabel.stringValue = "请输入卡片文字"
            return
        }
        guard !renderedPageResults.isEmpty else {
            statusLabel.stringValue = "预览正在更新"
            return
        }
        guard let window = view.window else { return }

        let panel = NSOpenPanel()
        panel.title = "选择保存目录"
        panel.prompt = "保存"
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.beginSheetModal(for: window) { [weak self] response in
            guard response == .OK, let directoryURL = panel.url else { return }
            self?.saveCard(to: directoryURL)
        }
    }

    private func saveCard(to directoryURL: URL) {
        saveButton.isEnabled = false
        defer { saveButton.isEnabled = true }

        do {
            let result = try service.export(card, renderedPages: renderedPageResults, to: directoryURL)
            let count = result.files.count
            statusLabel.stringValue = count == 1 ? "已保存" : "已保存 \(count) 张卡片"
        } catch {
            statusLabel.stringValue = "保存失败：\(error.localizedDescription)"
        }
    }

    @objc private func copyRequested() {
        guard renderedPageResults.indices.contains(selectedPageIndex) else {
            statusLabel.stringValue = "复制失败"
            return
        }
        let image = renderedPageResults[selectedPageIndex].image
        guard copyHandler([image]) else {
            statusLabel.stringValue = "复制失败"
            return
        }
        statusLabel.stringValue = renderedPageResults.count == 1
            ? "已复制图片"
            : "已复制第 \(selectedPageIndex + 1) 页"
    }

    @objc private func copyAllRequested() {
        guard !renderedPageResults.isEmpty else {
            statusLabel.stringValue = "复制失败"
            return
        }
        let images = renderedPageResults.map(\.image)
        guard copyHandler(images) else {
            statusLabel.stringValue = "复制失败"
            return
        }
        statusLabel.stringValue = "已复制 \(images.count) 张图片"
    }

    @objc private func airDropRequested() {
        guard renderedPageResults.indices.contains(selectedPageIndex),
              let airDrop = NSSharingService(named: .sendViaAirDrop) else {
            statusLabel.stringValue = "AirDrop 当前不可用"
            return
        }
        do {
            let file = try service.temporaryPNGURL(
                for: card,
                renderedPage: renderedPageResults[selectedPageIndex],
                pageCount: renderedPageResults.count
            )
            guard airDrop.canPerform(withItems: [file]) else {
                statusLabel.stringValue = "AirDrop 当前不可用"
                return
            }
            airDrop.perform(withItems: [file])
        } catch {
            statusLabel.stringValue = "AirDrop 失败：\(error.localizedDescription)"
        }
    }

    @objc private func closeRequested() {
        dismiss(self)
    }

    func textDidChange(_ notification: Notification) {
        updateCardFromEditor(debouncePreview: true)
    }

    private func updateCardFromEditor(debouncePreview: Bool) {
        let template = card.template
        clearAlternatives()
        let refreshed = service.makeCard(
            for: book,
            annotation: annotation,
            includeNote: noteCheckbox.state == .on,
            textOverride: textView.string,
            theme: template.theme,
            typography: card.typography
        )
        card = ShareCard(
            bookTitle: refreshed.bookTitle,
            author: refreshed.author,
            primaryText: refreshed.primaryText,
            supplementaryNote: refreshed.supplementaryNote,
            template: template,
            typography: card.typography
        )
        invalidateExport()
        if debouncePreview {
            schedulePreviewUpdate()
        } else {
            cancelPendingPreviewUpdate()
            updatePreview()
        }
    }

    private func updatedTypography(
        font: ShareCardFont? = nil,
        sizeMode: ShareCardFontSizeMode? = nil,
        fontSize: CGFloat? = nil,
        horizontalAlignment: ShareCardHorizontalAlignment? = nil,
        verticalAlignment: ShareCardVerticalAlignment? = nil
    ) -> ShareCardTypography {
        let current = card.typography
        return ShareCardTypography(
            font: font ?? current.font,
            sizeMode: sizeMode ?? current.sizeMode,
            fontSize: fontSize ?? current.fontSize,
            horizontalAlignment: horizontalAlignment ?? current.horizontalAlignment,
            verticalAlignment: verticalAlignment ?? current.verticalAlignment
        )
    }

    private func updateTypography(_ typography: ShareCardTypography) {
        cancelPendingPreviewUpdate()
        card = ShareCard(
            bookTitle: card.bookTitle,
            author: card.author,
            primaryText: card.primaryText,
            supplementaryNote: card.supplementaryNote,
            template: card.template,
            typography: typography
        )
        syncTypographyControls()
        invalidateExport()
        updatePreview()
    }

    private func syncTypographyControls() {
        sizeModePopup.selectItem(at: ShareCardFontSizeMode.allCases.firstIndex(of: card.typography.sizeMode) ?? 0)
        let fontSizeIndex = [42, 48, 56, 64, 72].firstIndex { CGFloat($0) == card.typography.fontSize } ?? 2
        fontSizePopup.selectItem(at: fontSizeIndex)
        fontSizePopup.isEnabled = card.typography.sizeMode == .fixed
        horizontalAlignmentControl.selectedSegment =
            ShareCardHorizontalAlignment.allCases.firstIndex(of: card.typography.horizontalAlignment) ?? 0
        verticalAlignmentControl.selectedSegment =
            ShareCardVerticalAlignment.allCases.firstIndex(of: card.typography.verticalAlignment) ?? 1
    }

    private func reloadThemeGrid() {
        themeButtons = []
        themeGridStack.arrangedSubviews.forEach {
            themeGridStack.removeArrangedSubview($0)
            $0.removeFromSuperview()
        }

        let themes = ShareCardTheme.allCases
        let columns = 4
        for start in stride(from: 0, to: themes.count, by: columns) {
            let row = NSStackView()
            row.orientation = .horizontal
            row.alignment = .centerY
            row.spacing = 8
            row.translatesAutoresizingMaskIntoConstraints = false

            for index in start..<min(start + columns, themes.count) {
                let theme = themes[index]
                let button: NSButton
                if let background = service.backgroundImage(for: theme) {
                    button = NSButton(
                        image: background,
                        target: self,
                        action: #selector(themeSelected(_:))
                    )
                } else {
                    button = NSButton(title: theme.displayName, target: self, action: #selector(themeSelected(_:)))
                }
                button.tag = index
                button.setButtonType(.toggle)
                button.state = theme == card.theme ? .on : .off
                button.imageScaling = .scaleProportionallyUpOrDown
                button.bezelStyle = .regularSquare
                button.toolTip = theme.displayName
                button.setAccessibilityLabel("主题 \(theme.displayName)")
                button.identifier = NSUserInterfaceItemIdentifier("share-card-theme-\(theme.rawValue)")
                button.translatesAutoresizingMaskIntoConstraints = false
                NSLayoutConstraint.activate([
                    button.widthAnchor.constraint(equalToConstant: Self.themeThumbnailSize.width),
                    button.heightAnchor.constraint(equalToConstant: Self.themeThumbnailSize.height)
                ])

                let nameLabel = NSTextField(labelWithString: theme.displayName)
                nameLabel.alignment = .center
                nameLabel.font = .systemFont(ofSize: 11)
                nameLabel.lineBreakMode = .byTruncatingTail
                nameLabel.setAccessibilityLabel("主题名称 \(theme.displayName)")
                nameLabel.toolTip = theme.displayName

                let item = NSStackView(views: [button, nameLabel])
                item.orientation = .vertical
                item.alignment = .centerX
                item.spacing = 4
                item.translatesAutoresizingMaskIntoConstraints = false
                item.widthAnchor.constraint(equalToConstant: Self.themeThumbnailSize.width).isActive = true
                row.addArrangedSubview(item)
                themeButtons.append(button)
            }
            themeGridStack.addArrangedSubview(row)
            row.widthAnchor.constraint(equalTo: themeGridStack.widthAnchor).isActive = true
        }
    }

    private func syncThemeSelection() {
        for button in themeButtons {
            button.state = button.tag == ShareCardTheme.allCases.firstIndex(of: card.theme)
                ? .on
                : .off
        }
    }

    private func clearAlternatives() {
        alternativeCards = []
        alternativeStack.arrangedSubviews.forEach {
            alternativeStack.removeArrangedSubview($0)
            $0.removeFromSuperview()
        }
        alternativeStack.isHidden = true
    }

    private func invalidateExport() {
        renderedPageResults = []
        thumbnailStack.arrangedSubviews.forEach {
            thumbnailStack.removeArrangedSubview($0)
            $0.removeFromSuperview()
        }
        pageLabel.stringValue = ""
        previewImageView.image = nil
        copyButton.isEnabled = false
        copyMenuButton.isEnabled = false
        saveButton.isEnabled = false
        airDropButton.isEnabled = false
        statusLabel.stringValue = ""
    }

    @objc private func pageSelected(_ sender: NSButton) {
        guard renderedPageResults.indices.contains(sender.tag) else { return }
        selectedPageIndex = sender.tag
        updateSelectedPagePreview()
    }

    private func updateAirDropAvailability() {
        guard renderedPageResults.indices.contains(selectedPageIndex),
              let airDrop = NSSharingService(named: .sendViaAirDrop) else {
            airDropButton.isEnabled = false
            return
        }
        airDropButton.isEnabled = airDrop.canPerform(withItems: [renderedPageResults[selectedPageIndex].image])
    }

    private func updatePreview() {
        do {
            renderedPageResults = try service.render(for: card)
        } catch {
            renderedPageResults = []
            statusLabel.stringValue = "无法生成页面预览"
        }
        if renderedPageResults.isEmpty {
            selectedPageIndex = 0
            thumbnailStack.arrangedSubviews.forEach {
                thumbnailStack.removeArrangedSubview($0)
                $0.removeFromSuperview()
            }
            pageLabel.stringValue = ""
            previewImageView.image = nil
            copyButton.isEnabled = false
            copyMenuButton.isEnabled = false
            saveButton.isEnabled = false
            airDropButton.isEnabled = false
            if card.primaryText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                statusLabel.stringValue = "请输入卡片文字"
            }
            return
        }

        selectedPageIndex = min(selectedPageIndex, renderedPageResults.count - 1)
        pageLabel.stringValue = "第 \(selectedPageIndex + 1) / \(renderedPageResults.count) 页"
        reloadThumbnails()
        updateSelectedPagePreview()
    }

    private func reloadThumbnails() {
        thumbnailStack.arrangedSubviews.forEach {
            thumbnailStack.removeArrangedSubview($0)
            $0.removeFromSuperview()
        }

        guard !renderedPageResults.isEmpty else {
            statusLabel.stringValue = "无法生成页面预览"
            return
        }
        for (index, renderedPage) in renderedPageResults.enumerated() {
            let button = NSButton(
                image: thumbnailImage(from: renderedPage.image),
                target: self,
                action: #selector(pageSelected(_:))
            )
            button.tag = index
            button.setButtonType(.toggle)
            button.state = index == selectedPageIndex ? .on : .off
            button.imageScaling = .scaleProportionallyUpOrDown
            button.bezelStyle = .regularSquare
            button.toolTip = "查看第 \(index + 1) 页"
            button.setAccessibilityLabel("第 \(index + 1) 页")
            button.identifier = NSUserInterfaceItemIdentifier("share-card-page-\(index)")
            button.translatesAutoresizingMaskIntoConstraints = false
            thumbnailStack.addArrangedSubview(button)
            NSLayoutConstraint.activate([
                button.widthAnchor.constraint(equalToConstant: Self.thumbnailSize.width),
                button.heightAnchor.constraint(equalToConstant: Self.thumbnailSize.height)
            ])
        }
    }

    private func thumbnailImage(from image: NSImage) -> NSImage {
        guard let bitmap = NSBitmapImageRep(
            bitmapDataPlanes: nil,
            pixelsWide: Int(Self.thumbnailSize.width),
            pixelsHigh: Int(Self.thumbnailSize.height),
            bitsPerSample: 8,
            samplesPerPixel: 4,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: .deviceRGB,
            bitmapFormat: [],
            bytesPerRow: 0,
            bitsPerPixel: 0
        ), let graphicsContext = NSGraphicsContext(bitmapImageRep: bitmap) else {
            return NSImage(size: Self.thumbnailSize)
        }

        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = graphicsContext
        graphicsContext.imageInterpolation = .high
        NSColor.clear.setFill()
        NSRect(origin: .zero, size: Self.thumbnailSize).fill()
        image.draw(
            in: NSRect(origin: .zero, size: Self.thumbnailSize),
            from: NSRect(origin: .zero, size: image.size),
            operation: .copy,
            fraction: 1
        )
        NSGraphicsContext.restoreGraphicsState()

        let thumbnail = NSImage(size: Self.thumbnailSize)
        thumbnail.addRepresentation(bitmap)
        return thumbnail
    }

    private func updateSelectedPagePreview() {
        do {
            guard renderedPageResults.indices.contains(selectedPageIndex) else {
                throw ShareCardExportError.emptyCardText
            }
            previewImageView.image = renderedPageResults[selectedPageIndex].image
            copyButton.isEnabled = true
            copyMenuButton.isEnabled = renderedPageResults.count > 1
            saveButton.isEnabled = true
            updateAirDropAvailability()
            pageLabel.stringValue = "第 \(selectedPageIndex + 1) / \(renderedPageResults.count) 页"
            for (index, view) in thumbnailStack.arrangedSubviews.enumerated() {
                (view as? NSButton)?.state = index == selectedPageIndex ? .on : .off
            }
            if statusLabel.stringValue.hasPrefix("无法") || statusLabel.stringValue.hasPrefix("保存失败") {
                statusLabel.stringValue = ""
            }
        } catch {
            previewImageView.image = nil
            copyButton.isEnabled = false
            copyMenuButton.isEnabled = false
            saveButton.isEnabled = false
            airDropButton.isEnabled = false
            statusLabel.stringValue = card.primaryText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                ? "请输入卡片文字"
                : "无法生成页面预览"
        }
    }

    private func schedulePreviewUpdate() {
        textChangeTimer?.invalidate()
        let timer = Timer(
            timeInterval: Self.textChangeDebounceInterval,
            repeats: false
        ) { [weak self] _ in
            guard let self else { return }
            self.textChangeTimer = nil
            self.updatePreview()
        }
        textChangeTimer = timer
        RunLoop.main.add(timer, forMode: .common)
    }

    private func cancelPendingPreviewUpdate() {
        textChangeTimer?.invalidate()
        textChangeTimer = nil
    }
}
