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
        [locationLabel, contentLabel, noteLabel].forEach { stack.addArrangedSubview($0) }
        addSubview(stack)

        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: AnnotationCellView.horizontalInset),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -AnnotationCellView.horizontalInset),
            stack.topAnchor.constraint(equalTo: topAnchor, constant: AnnotationCellView.verticalInset),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -AnnotationCellView.verticalInset)
        ])

        textField = contentLabel
    }

    func configure(with annotation: Annotation) {
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
        let available = width - AnnotationCellView.horizontalInset * 2
        guard available > 0 else { return }
        contentLabel.preferredMaxLayoutWidth = available
        noteLabel.preferredMaxLayoutWidth = available
    }
}
