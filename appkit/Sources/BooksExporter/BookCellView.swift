import AppKit

/// 书单的一行。
///
/// 早先直接把裸 `NSTextField` 当行视图返回:表格会把它的 frame 撑成整行高,
/// 而 `NSTextFieldCell` 在超高的 frame 里默认把单行文字画在顶部,看起来就是「靠上」。
/// 包一层 `NSTableCellView` 并用 centerY 约束定位,文字才会垂直居中。
final class BookCellView: NSTableCellView {
    private static let horizontalInset: CGFloat = 4

    private let label = NSTextField(labelWithString: "")

    init(identifier: NSUserInterfaceItemIdentifier, tabularDigits: Bool) {
        super.init(frame: .zero)
        self.identifier = identifier

        label.lineBreakMode = .byTruncatingTail
        if tabularDigits {
            label.font = .monospacedDigitSystemFont(ofSize: NSFont.systemFontSize, weight: .regular)
        }
        label.translatesAutoresizingMaskIntoConstraints = false
        addSubview(label)

        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: leadingAnchor, constant: BookCellView.horizontalInset),
            label.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -BookCellView.horizontalInset),
            label.centerYAnchor.constraint(equalTo: centerYAnchor)
        ])

        textField = label
    }

    required init?(coder: NSCoder) {
        fatalError("不支持从 Interface Builder 加载")
    }

    func configure(text: String) {
        label.stringValue = text
    }
}
