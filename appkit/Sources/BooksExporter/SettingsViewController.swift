import AppKit

final class SettingsViewController: NSViewController {
    private let settingsStore: AppSettingsStore
    private let refreshIntervalPopup = NSPopUpButton(frame: .zero, pullsDown: false)

    init(settingsStore: AppSettingsStore = .shared) {
        self.settingsStore = settingsStore
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        fatalError("不支持从 Interface Builder 加载")
    }

    override func loadView() {
        let rootView = NSView()

        let sectionTitle = NSTextField(labelWithString: "Apple Books")
        sectionTitle.font = .systemFont(ofSize: 15, weight: .semibold)

        let refreshLabel = NSTextField(labelWithString: "自动刷新间隔")
        refreshLabel.setContentHuggingPriority(.defaultLow, for: .horizontal)

        configureRefreshIntervalPopup()
        refreshIntervalPopup.translatesAutoresizingMaskIntoConstraints = false
        refreshIntervalPopup.identifier = NSUserInterfaceItemIdentifier("settings.refresh-interval")
        refreshIntervalPopup.widthAnchor.constraint(equalToConstant: 160).isActive = true

        let refreshRow = NSStackView(views: [refreshLabel, refreshIntervalPopup])
        refreshRow.orientation = .horizontal
        refreshRow.alignment = .centerY
        refreshRow.spacing = 12

        let detailLabel = NSTextField(labelWithString: "从 Apple Books 本地数据库定期读取最新高亮和笔记。")
        detailLabel.textColor = .secondaryLabelColor
        detailLabel.maximumNumberOfLines = 0
        detailLabel.lineBreakMode = .byWordWrapping
        detailLabel.identifier = NSUserInterfaceItemIdentifier("settings.refresh-description")

        let contentStack = NSStackView(views: [sectionTitle, refreshRow, detailLabel])
        contentStack.translatesAutoresizingMaskIntoConstraints = false
        contentStack.orientation = .vertical
        contentStack.alignment = .leading
        contentStack.spacing = 16

        rootView.addSubview(contentStack)
        NSLayoutConstraint.activate([
            contentStack.leadingAnchor.constraint(equalTo: rootView.leadingAnchor, constant: 24),
            contentStack.trailingAnchor.constraint(equalTo: rootView.trailingAnchor, constant: -24),
            contentStack.topAnchor.constraint(equalTo: rootView.topAnchor, constant: 24),
            contentStack.bottomAnchor.constraint(equalTo: rootView.bottomAnchor, constant: -24),
            refreshRow.widthAnchor.constraint(equalTo: contentStack.widthAnchor),
            detailLabel.widthAnchor.constraint(equalTo: contentStack.widthAnchor)
        ])

        view = rootView
    }

    private func configureRefreshIntervalPopup() {
        refreshIntervalPopup.removeAllItems()
        for interval in RefreshInterval.allCases {
            refreshIntervalPopup.addItem(withTitle: interval.displayName)
            refreshIntervalPopup.lastItem?.tag = interval.rawValue
        }
        refreshIntervalPopup.selectItem(withTag: settingsStore.refreshInterval.rawValue)
        refreshIntervalPopup.target = self
        refreshIntervalPopup.action = #selector(refreshIntervalChanged)
    }

    @objc private func refreshIntervalChanged() {
        guard let item = refreshIntervalPopup.selectedItem,
              let interval = RefreshInterval(rawValue: item.tag) else {
            return
        }
        settingsStore.refreshInterval = interval
    }
}
