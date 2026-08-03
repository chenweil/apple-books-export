import AppKit

final class ShareCardEditorViewController: NSViewController, NSTextViewDelegate, NSSharingServicePickerDelegate {
    private static let alternativePreviewSize = NSSize(width: 72, height: 96)

    private let book: Book
    private let annotation: Annotation
    private let service = ShareCardService()
    private var card: ShareCard
    private var alternativeCards: [ShareCard] = []
    private var sharePicker: NSSharingServicePicker?

    private let previewImageView = NSImageView()
    private let textView = NSTextView()
    private let textScrollView = NSScrollView()
    private let noteCheckbox = NSButton(checkboxWithTitle: "添加笔记", target: nil, action: nil)
    private let themePopup = NSPopUpButton(frame: .zero, pullsDown: false)
    private let changeButton = NSButton(title: "换一换", target: nil, action: nil)
    private let alternativeStack = NSStackView()
    private let folderCheckbox = NSButton(checkboxWithTitle: "保存后打开文件夹", target: nil, action: nil)
    private let copyButton = NSButton(title: "复制图片", target: nil, action: nil)
    private let saveButton = NSButton(title: "保存 PNG", target: nil, action: nil)
    private let shareButton = NSButton(title: "分享", target: nil, action: nil)
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
        preferredContentSize = NSSize(width: 980, height: 700)
    }

    required init?(coder: NSCoder) {
        fatalError("不支持从 Interface Builder 加载")
    }

    override func loadView() {
        view = makeView()
        textView.string = card.primaryText
        noteCheckbox.isHidden = annotation.contentText?.isEmpty ?? true
        noteCheckbox.isEnabled = !(annotation.noteText?.isEmpty ?? true)
        noteCheckbox.state = .off
        folderCheckbox.state = ShareCardPreferences.openExportFolder ? .on : .off
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

        themePopup.addItems(withTitles: ShareCardTheme.allCases.map { $0.displayName })
        themePopup.selectItem(at: ShareCardTheme.allCases.firstIndex(of: card.theme) ?? 0)
        themePopup.target = self
        themePopup.action = #selector(themeChanged)

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

        saveButton.image = NSImage(
            systemSymbolName: "square.and.arrow.down",
            accessibilityDescription: "保存 PNG"
        )
        saveButton.imagePosition = .imageLeading
        saveButton.keyEquivalent = "\r"
        saveButton.target = self
        saveButton.action = #selector(saveRequested)

        shareButton.image = NSImage(
            systemSymbolName: "square.and.arrow.up",
            accessibilityDescription: "分享"
        )
        shareButton.imagePosition = .imageLeading
        shareButton.target = self
        shareButton.action = #selector(shareRequested)
        shareButton.isEnabled = false
        shareButton.toolTip = "打开 macOS 分享面板（AirDrop 等可用服务）"

        airDropButton.target = self
        airDropButton.action = #selector(airDropRequested)
        airDropButton.isHidden = true
        airDropButton.isEnabled = false

        closeButton.target = self
        closeButton.action = #selector(closeRequested)

        statusLabel.textColor = .secondaryLabelColor
        statusLabel.lineBreakMode = .byTruncatingTail

        let themeLabel = NSTextField(labelWithString: "主题")
        let themeRow = NSStackView(views: [themeLabel, themePopup])
        themeRow.orientation = .horizontal
        themeRow.alignment = .centerY
        themeRow.spacing = 8

        let controlsTitle = NSTextField(labelWithString: "卡片文字")
        controlsTitle.font = .systemFont(ofSize: 17, weight: .semibold)

        let actionRow = NSStackView(views: [copyButton, saveButton, shareButton, airDropButton, closeButton])
        actionRow.orientation = .horizontal
        actionRow.alignment = .centerY
        actionRow.spacing = 8

        let controls = NSStackView(
            views: [controlsTitle, textScrollView, noteCheckbox, themeRow, changeButton,
                    alternativeStack, folderCheckbox, statusLabel, actionRow]
        )
        controls.orientation = .vertical
        controls.alignment = .leading
        controls.spacing = 12
        controls.translatesAutoresizingMaskIntoConstraints = false

        let content = NSStackView(views: [previewImageView, controls])
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

            previewImageView.widthAnchor.constraint(equalToConstant: 420),
            previewImageView.heightAnchor.constraint(equalTo: previewImageView.widthAnchor, multiplier: 4.0 / 3.0),
            previewImageView.heightAnchor.constraint(lessThanOrEqualTo: content.heightAnchor),

            controls.widthAnchor.constraint(greaterThanOrEqualToConstant: 360),
            textScrollView.heightAnchor.constraint(equalToConstant: 160),
            textScrollView.widthAnchor.constraint(equalTo: controls.widthAnchor),
            themeRow.widthAnchor.constraint(equalTo: controls.widthAnchor),
            alternativeStack.widthAnchor.constraint(equalTo: controls.widthAnchor),
            alternativeStack.heightAnchor.constraint(equalToConstant: 112),
            statusLabel.widthAnchor.constraint(equalTo: controls.widthAnchor),
            actionRow.widthAnchor.constraint(equalTo: controls.widthAnchor)
        ])

        return root
    }

    @objc private func noteChanged() {
        updateCardFromEditor()
    }

    @objc private func themeChanged() {
        guard ShareCardTheme.allCases.indices.contains(themePopup.indexOfSelectedItem) else { return }
        let theme = ShareCardTheme.allCases[themePopup.indexOfSelectedItem]
        card = service.makeCard(
            for: book,
            annotation: annotation,
            includeNote: noteCheckbox.state == .on,
            textOverride: textView.string,
            theme: theme
        )
        invalidateExport()
        updatePreview()
    }

    @objc private func changeItUp() {
        alternativeCards = service.alternativeCards(for: card)
        alternativeStack.arrangedSubviews.forEach { alternativeStack.removeArrangedSubview($0); $0.removeFromSuperview() }

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

    @objc private func alternativeSelected(_ sender: NSButton) {
        guard alternativeCards.indices.contains(sender.tag) else { return }
        card = alternativeCards[sender.tag]
        textView.string = card.primaryText
        themePopup.selectItem(at: ShareCardTheme.allCases.firstIndex(of: card.theme) ?? 0)
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
            let result = try service.export(card, to: directoryURL)
            lastExportResult = result
            shareButton.isEnabled = !result.files.isEmpty
            let canAirDrop = NSSharingService(named: .sendViaAirDrop)?
                .canPerform(withItems: result.files) ?? false
            airDropButton.isHidden = !canAirDrop
            airDropButton.isEnabled = canAirDrop
            let count = result.files.count
            statusLabel.stringValue = count == 1 ? "已保存" : "已保存 \(count) 张卡片"
        } catch {
            statusLabel.stringValue = "保存失败：\(error.localizedDescription)"
        }
    }

    @objc private func copyRequested() {
        do {
            let pages = service.pages(for: card)
            let images = try pages.indices.map { try service.previewImage(for: card, pageIndex: $0) }
            guard !images.isEmpty, copyHandler(images) else {
                statusLabel.stringValue = "复制失败"
                return
            }
            statusLabel.stringValue = images.count == 1 ? "已复制图片" : "已复制 \(images.count) 张图片"
        } catch {
            statusLabel.stringValue = "复制失败：\(error.localizedDescription)"
        }
    }

    @objc private func shareRequested() {
        guard let result = lastExportResult, !result.files.isEmpty else { return }
        let picker = NSSharingServicePicker(items: result.files)
        picker.delegate = self
        sharePicker = picker
        picker.show(relativeTo: shareButton.bounds, of: shareButton, preferredEdge: .minY)
    }

    func sharingServicePicker(
        _ sharingServicePicker: NSSharingServicePicker,
        sharingServicesForItems items: [Any],
        proposedSharingServices proposedServices: [NSSharingService]
    ) -> [NSSharingService] {
        proposedServices.filter { service in
            let labels = "\(service.title) \(service.menuItemTitle)".lowercased()
            return !labels.contains("微信") && !labels.contains("wechat")
        }
    }

    @objc private func airDropRequested() {
        guard let result = lastExportResult, !result.files.isEmpty,
              let airDrop = NSSharingService(named: .sendViaAirDrop),
              airDrop.canPerform(withItems: result.files) else {
            statusLabel.stringValue = "AirDrop 当前不可用"
            return
        }
        airDrop.perform(withItems: result.files)
    }

    @objc private func closeRequested() {
        dismiss(self)
    }

    private var lastExportResult: ShareCardExportResult?

    func textDidChange(_ notification: Notification) {
        updateCardFromEditor()
    }

    private func updateCardFromEditor() {
        let template = card.template
        let refreshed = service.makeCard(
            for: book,
            annotation: annotation,
            includeNote: noteCheckbox.state == .on,
            textOverride: textView.string,
            theme: template.theme
        )
        card = ShareCard(
            bookTitle: refreshed.bookTitle,
            author: refreshed.author,
            primaryText: refreshed.primaryText,
            supplementaryNote: refreshed.supplementaryNote,
            template: template
        )
        invalidateExport()
        updatePreview()
    }

    private func invalidateExport() {
        lastExportResult = nil
        shareButton.isEnabled = false
        airDropButton.isHidden = true
        airDropButton.isEnabled = false
        statusLabel.stringValue = ""
    }

    private func updatePreview() {
        do {
            previewImageView.image = try service.previewImage(for: card)
            copyButton.isEnabled = true
            if statusLabel.stringValue.hasPrefix("无法") || statusLabel.stringValue.hasPrefix("保存失败") {
                statusLabel.stringValue = ""
            }
        } catch {
            previewImageView.image = nil
            copyButton.isEnabled = false
            statusLabel.stringValue = "请输入卡片文字"
        }
    }
}
