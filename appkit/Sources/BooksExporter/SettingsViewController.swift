import AppKit

final class SettingsViewController: NSViewController {
    private let settingsStore: AppSettingsStore
    private let refreshIntervalPopup = NSPopUpButton(frame: .zero, pullsDown: false)
    private let speechClient: RustCLIClient?
    private let credentialResolver: SpeechCredentialResolver?

    private let apiKeyField = NSSecureTextField(frame: .zero)
    private let speechStatusLabel = NSTextField(labelWithString: "")
    private let verifyButton = NSButton(frame: .zero)

    init(
        settingsStore: AppSettingsStore = .shared,
        speechClient: RustCLIClient? = nil,
        credentialResolver: SpeechCredentialResolver? = nil
    ) {
        self.settingsStore = settingsStore
        self.speechClient = speechClient
        self.credentialResolver = credentialResolver
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
        addSpeechSection(to: contentStack)

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

    /// The speech credential and the one channel this app has been tested
    /// against.
    ///
    /// The channel is stated rather than offered as a choice. `speech voices`
    /// reports whatever provider the CLI is configured for, and a picker that
    /// implied more than one supported channel would be claiming coverage the
    /// app has not verified.
    private func addSpeechSection(to stack: NSStackView) {
        let separator = NSBox()
        separator.boxType = .separator
        stack.addArrangedSubview(separator)

        let title = NSTextField(labelWithString: "语音")
        title.font = .systemFont(ofSize: 15, weight: .semibold)

        let channelTitle = NSTextField(labelWithString: "支持渠道")
        channelTitle.setContentHuggingPriority(.defaultLow, for: .horizontal)
        let channelValue = NSTextField(labelWithString: Self.verifiedChannelDescription)
        channelValue.textColor = .secondaryLabelColor
        channelValue.identifier = NSUserInterfaceItemIdentifier("settings.speech-channel")

        let channelRow = NSStackView(views: [channelTitle, channelValue])
        channelRow.orientation = .horizontal
        channelRow.alignment = .centerY
        channelRow.spacing = 12

        let keyTitle = NSTextField(labelWithString: "API Key")
        keyTitle.setContentHuggingPriority(.defaultLow, for: .horizontal)
        apiKeyField.translatesAutoresizingMaskIntoConstraints = false
        apiKeyField.identifier = NSUserInterfaceItemIdentifier("settings.speech-api-key")
        apiKeyField.placeholderString = "存储在钥匙串，不写入配置文件"
        apiKeyField.widthAnchor.constraint(equalToConstant: 300).isActive = true

        verifyButton.title = "验证"
        verifyButton.bezelStyle = .rounded
        verifyButton.target = self
        verifyButton.action = #selector(verifyCredential)
        verifyButton.identifier = NSUserInterfaceItemIdentifier("settings.speech-verify")

        let keyRow = NSStackView(views: [keyTitle, apiKeyField, verifyButton])
        keyRow.orientation = .horizontal
        keyRow.alignment = .centerY
        keyRow.spacing = 12

        speechStatusLabel.textColor = .secondaryLabelColor
        speechStatusLabel.maximumNumberOfLines = 0
        speechStatusLabel.lineBreakMode = .byWordWrapping
        speechStatusLabel.identifier = NSUserInterfaceItemIdentifier("settings.speech-status")

        let note = NSTextField(labelWithString: """
            密钥只保存在本机钥匙串，不会写入配置文件，也不会出现在命令行参数里。\
            「生成语音」是唯一联网且产生费用的操作。
            """)
        note.textColor = .secondaryLabelColor
        note.maximumNumberOfLines = 0
        note.lineBreakMode = .byWordWrapping
        note.identifier = NSUserInterfaceItemIdentifier("settings.speech-note")

        stack.addArrangedSubview(title)
        stack.addArrangedSubview(channelRow)
        stack.addArrangedSubview(keyRow)
        stack.addArrangedSubview(speechStatusLabel)
        stack.addArrangedSubview(note)
    }

    /// What the app has actually been verified against. Not a list: ADR 0007
    /// makes SenseAudio the first provider rather than the only one forever, and
    /// a read-only label says which one is covered today without implying the
    /// others are one click away.
    private static let verifiedChannelDescription = "SenseAudio（sensenova-tts-2.0）— 已测试"

    override func viewDidLoad() {
        super.viewDidLoad()
        Task { await refreshCredentialState() }
    }

    private func refreshCredentialState() async {
        guard let credentialResolver else {
            speechStatusLabel.stringValue = ""
            return
        }
        speechStatusLabel.stringValue = await credentialResolver.hasStoredSecret()
            ? "已保存密钥。"
            : "尚未保存密钥。"
    }

    @objc private func verifyCredential() {
        guard let client = speechClient, let credentialResolver else {
            speechStatusLabel.stringValue = "未找到 Rust 导出器，无法验证。"
            return
        }

        verifyButton.isEnabled = false
        speechStatusLabel.stringValue = "正在验证…"

        Task {
            defer { verifyButton.isEnabled = true }
            do {
                let key = apiKeyField.stringValue
                if !key.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    try await credentialResolver.store(key)
                    apiKeyField.stringValue = ""
                }
                // `speech voices` is the only free command that still
                // authenticates, so it is the only reliable probe: the CLI
                // reports the variable's name but never whether it is set.
                let catalog = try await SpeechService(
                    client: client,
                    credentialResolver: credentialResolver
                ).voices()
                speechStatusLabel.stringValue = catalog.receipt.stale
                    ? "凭据有效。音色目录已过期（取自 \(catalog.receipt.fetchedAt ?? "未知时间")），生成前会自动刷新。"
                    : "凭据有效，可用音色 \(catalog.receipt.voices.count) 个。"
            } catch let SpeechServiceError.commandFailed(error) {
                speechStatusLabel.stringValue = error.userFacingDescription
            } catch {
                speechStatusLabel.stringValue = error.localizedDescription
            }
        }
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
