import AppKit

/// 笔记行。使用可复用 cell + 自适应行高,避免固定 64pt 行高把正文截断。
final class AnnotationCellView: NSTableCellView {
    static let reuseIdentifier = NSUserInterfaceItemIdentifier("AnnotationCellView")

    private static let horizontalInset: CGFloat = 8
    private static let verticalInset: CGFloat = 8
    private static let stackSpacing: CGFloat = 4

    private let locationLabel = NSTextField(labelWithString: "")
    private let contentLabel = NSTextField(wrappingLabelWithString: "")
    private let noteLabel = NSTextField(wrappingLabelWithString: "")
    private let stack = NSStackView()
    private let rowStack = NSStackView()
    private let cardButton = NSButton(title: "生成卡片", target: nil, action: nil)
    private var layoutWidth: CGFloat = 0
    private var onCardRequested: (() -> Void)?

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        identifier = AnnotationCellView.reuseIdentifier
        configureView()
    }

    required init?(coder: NSCoder) {
        fatalError("不支持从 Interface Builder 加载")
    }

    private func configureView() {
        locationLabel.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        locationLabel.textColor = .secondaryLabelColor
        locationLabel.lineBreakMode = .byTruncatingTail

        contentLabel.font = .systemFont(ofSize: NSFont.systemFontSize)
        contentLabel.isSelectable = true

        noteLabel.font = .systemFont(ofSize: NSFont.systemFontSize)
        noteLabel.textColor = .secondaryLabelColor
        noteLabel.isSelectable = true

        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = AnnotationCellView.stackSpacing
        stack.translatesAutoresizingMaskIntoConstraints = false
        stack.setContentHuggingPriority(.defaultLow, for: .horizontal)
        stack.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        [locationLabel, contentLabel, noteLabel].forEach { stack.addArrangedSubview($0) }

        cardButton.image = NSImage(
            systemSymbolName: "square.and.arrow.up",
            accessibilityDescription: "生成卡片"
        )
        cardButton.imagePosition = .imageLeading
        cardButton.target = self
        cardButton.action = #selector(cardRequested)
        cardButton.isHidden = true
        cardButton.identifier = NSUserInterfaceItemIdentifier("share-card-entry")
        cardButton.setContentHuggingPriority(.required, for: .horizontal)
        cardButton.setContentCompressionResistancePriority(.required, for: .horizontal)

        rowStack.orientation = .horizontal
        rowStack.alignment = .top
        rowStack.distribution = .fill
        rowStack.spacing = 12
        rowStack.translatesAutoresizingMaskIntoConstraints = false
        rowStack.addArrangedSubview(stack)
        rowStack.addArrangedSubview(cardButton)
        addSubview(rowStack)

        NSLayoutConstraint.activate([
            rowStack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: AnnotationCellView.horizontalInset),
            rowStack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -AnnotationCellView.horizontalInset),
            rowStack.topAnchor.constraint(equalTo: topAnchor, constant: AnnotationCellView.verticalInset),
            rowStack.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -AnnotationCellView.verticalInset)
        ])

        textField = contentLabel
    }

    func configure(with annotation: Annotation) {
        cardButton.isHidden = true
        onCardRequested = nil
        locationLabel.stringValue = annotation.displayLocation

        let content = annotation.contentText ?? ""
        contentLabel.stringValue = content
        contentLabel.isHidden = content.isEmpty

        let note = annotation.noteText ?? ""
        noteLabel.stringValue = note.isEmpty ? "" : "笔记：\(note)"
        noteLabel.isHidden = note.isEmpty
    }

    /// 自适应行高要求包裹型 label 知道自己的换行宽度。
    func updateLayoutWidth(_ width: CGFloat) {
        layoutWidth = width
        updateTextWidth()
    }

    func setCardEntryVisible(_ visible: Bool, onRequested: (() -> Void)? = nil) {
        cardButton.isHidden = !visible
        onCardRequested = visible ? onRequested : nil
        updateTextWidth()
    }

    private func updateTextWidth() {
        let buttonWidth = cardButton.isHidden ? 0 : cardButton.fittingSize.width + rowStack.spacing
        let available = layoutWidth - AnnotationCellView.horizontalInset * 2 - buttonWidth
        guard available > 0 else { return }
        contentLabel.preferredMaxLayoutWidth = available
        noteLabel.preferredMaxLayoutWidth = available
    }

    @objc private func cardRequested() {
        onCardRequested?()
    }
}
