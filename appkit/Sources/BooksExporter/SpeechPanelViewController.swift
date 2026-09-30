import AppKit

/// The speech panel for one annotation.
///
/// The panel *is* the confirmation. ADR 0007 says a human entry point shows the
/// content kind, the voice and the estimated character count before a single
/// generation, and that executing the generate command is itself the
/// authorisation -- no second dialog. So the cost notice and the resolved
/// parameters live on screen at the moment the button is pressed, rather than
/// behind an "are you sure?".
final class SpeechPanelViewController: NSViewController {
    private let book: Book
    private let annotation: Annotation
    private let speech: SpeechService
    private let player: SpeechAudioPlaying
    private let hasCredential: () async -> Bool

    // Controls
    private let contentKindControl = NSSegmentedControl(
        labels: ["高亮", "笔记"],
        trackingMode: .selectOne,
        target: nil,
        action: nil
    )
    private let contentPreview = NSTextField(labelWithString: "")
    private let characterLabel = NSTextField(labelWithString: "")
    private let voiceGroupButton = NSPopUpButton(frame: .zero, pullsDown: false)
    private let variantButton = NSPopUpButton(frame: .zero, pullsDown: false)
    private let speedSlider = NSSlider(
        value: 1, minValue: 0.5, maxValue: 2, target: nil, action: nil
    )
    private let volumeSlider = NSSlider(
        value: 1, minValue: 0.01, maxValue: 10, target: nil, action: nil
    )
    private let pitchSlider = NSSlider(
        value: 0, minValue: -12, maxValue: 12, target: nil, action: nil
    )
    private let speedValue = NSTextField(labelWithString: "")
    private let volumeValue = NSTextField(labelWithString: "")
    private let pitchValue = NSTextField(labelWithString: "")
    private let costLabel = NSTextField(labelWithString: "")
    private let statusLabel = NSTextField(labelWithString: "")
    private let generateButton = NSButton(frame: .zero)
    private let playButton = NSButton(frame: .zero)
    private let regenerateButton = NSButton(frame: .zero)
    /// Stored rather than local so a later addition of another action has one
    /// obvious place to go, and so the probe can see the whole action row.
    private let buttonsStack = NSStackView()
    /// Re-runs the load, for a panel that opened before the key was stored or
    /// that hit a transient failure. Without it the panel's state is frozen at
    /// whatever the first `load()` found, and the only way to see the change is
    /// to close the panel -- which, before `closePanel`, it could not do.
    private let recheckButton = NSButton(frame: .zero)
    /// The way out. A sheet presented with `presentAsSheet` has no title bar of
    /// its own, so nothing else dismisses it: without this the panel is a trap.
    private let closeButton = NSButton(frame: .zero)

    // State
    private var catalog: SpeechVoiceCatalog?
    private var profile: SpeechProfile?
    private var generatedClipID: String?
    private var generatedPath: String?
    private var busy = false

    /// The contract's hard cap, mirrored so the panel can refuse before
    /// spending a round trip on a request the CLI would reject locally.
    private static let maximumCharacters = 10_000

    /// A sheet takes its size from the view controller, and this panel was the
    /// only sheet in the app that never declared one.
    ///
    /// Without it the width came from the content: every wrapping label in the
    /// stack reports its full single-line width as its intrinsic width, so a
    /// single long highlight opened a sheet wider than the display, with the
    /// voice pickers and the generate button pushed off screen. The probe never
    /// caught it because its fixture text was nine characters long.
    ///
    /// Sized to the widest row -- 音色 plus a 200pt and a 260pt popup plus
    /// padding -- rather than to the parent window, and clamped to the visible
    /// frame so the sheet cannot run off a small display either.
    /// ShareCardEditorViewController sets its size the same way.
    private static let preferredSize = NSSize(width: 600, height: 500)

    /// Never larger than what the display can actually show. Falls back to the
    /// declared size when there is no screen to measure, which is the case in
    /// some headless contexts.
    private static func panelSize() -> NSSize {
        guard let visible = NSScreen.main?.visibleFrame else { return preferredSize }
        return NSSize(
            width: min(preferredSize.width, visible.width - 80),
            height: min(preferredSize.height, visible.height - 80)
        )
    }

    init(
        book: Book,
        annotation: Annotation,
        speech: SpeechService,
        player: SpeechAudioPlaying,
        hasCredential: @escaping () async -> Bool
    ) {
        self.book = book
        self.annotation = annotation
        self.speech = speech
        self.player = player
        self.hasCredential = hasCredential
        super.init(nibName: nil, bundle: nil)
        preferredContentSize = Self.panelSize()
    }

    required init?(coder: NSCoder) {
        fatalError("不支持从 Interface Builder 加载")
    }

    // MARK: - Content

    /// The text that would be sent for the current content kind, or nil when
    /// that part does not exist.
    ///
    /// A Speech Clip reads one part of an annotation -- highlight or note -- and
    /// never a merged version, so the panel previews exactly one of them and
    /// disables the choice that has no text.
    private func text(for kind: String) -> String? {
        switch kind {
        case "note":
            guard let note = annotation.noteText,
                  !note.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
                return nil
            }
            return note
        default:
            guard let content = annotation.contentText,
                  !content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
                return nil
            }
            return content
        }
    }

    private var selectedContentKind: String {
        contentKindControl.selectedSegment == 1 ? "note" : "highlight"
    }

    // MARK: - Layout

    override func loadView() {
        let root = NSView()

        let title = NSTextField(labelWithString: "生成语音")
        title.font = .systemFont(ofSize: 15, weight: .semibold)

        contentKindControl.target = self
        contentKindControl.action = #selector(reloadPreview)
        contentKindControl.identifier = NSUserInterfaceItemIdentifier("speech.content-kind")

        contentPreview.textColor = .secondaryLabelColor
        contentPreview.maximumNumberOfLines = 4
        contentPreview.lineBreakMode = .byWordWrapping
        contentPreview.identifier = NSUserInterfaceItemIdentifier("speech.content-preview")

        characterLabel.textColor = .secondaryLabelColor
        characterLabel.identifier = NSUserInterfaceItemIdentifier("speech.characters")

        voiceGroupButton.target = self
        voiceGroupButton.action = #selector(voiceGroupChanged)
        voiceGroupButton.identifier = NSUserInterfaceItemIdentifier("speech.voice-group")

        variantButton.target = self
        variantButton.action = #selector(variantChanged)
        variantButton.identifier = NSUserInterfaceItemIdentifier("speech.voice-variant")

        let voiceTitle = NSTextField(labelWithString: "音色")
        voiceTitle.setContentHuggingPriority(.defaultLow, for: .horizontal)
        let voiceRow = NSStackView(views: [voiceTitle, voiceGroupButton, variantButton])
        voiceRow.orientation = NSUserInterfaceLayoutOrientation.horizontal
        voiceRow.spacing = 12
        voiceGroupButton.widthAnchor.constraint(equalToConstant: 200).isActive = true
        variantButton.widthAnchor.constraint(equalToConstant: 260).isActive = true

        let toneStack = NSStackView(views: [
            toneRow(title: "语速", slider: speedSlider, value: speedValue,
                    identifier: "speech.speed"),
            toneRow(title: "音量", slider: volumeSlider, value: volumeValue,
                    identifier: "speech.volume"),
            toneRow(title: "声调", slider: pitchSlider, value: pitchValue,
                    identifier: "speech.pitch"),
        ])
        toneStack.orientation = NSUserInterfaceLayoutOrientation.vertical
        toneStack.spacing = 8

        for slider in [speedSlider, volumeSlider, pitchSlider] {
            slider.target = self
            slider.action = #selector(toneChanged)
        }
        // A step so the value the panel shows is a value the clip ID can be
        // reproduced from; continuous dragging would produce parameters no
        // keyboard entry could match.
        speedSlider.numberOfTickMarks = 0
        speedSlider.isContinuous = false
        volumeSlider.isContinuous = false
        pitchSlider.isContinuous = false

        costLabel.maximumNumberOfLines = 0
        costLabel.lineBreakMode = .byWordWrapping
        costLabel.identifier = NSUserInterfaceItemIdentifier("speech.cost-notice")

        statusLabel.textColor = .secondaryLabelColor
        statusLabel.maximumNumberOfLines = 0
        statusLabel.lineBreakMode = .byWordWrapping
        statusLabel.identifier = NSUserInterfaceItemIdentifier("speech.status")

        configureButton(generateButton, title: "生成", action: #selector(generate),
                        identifier: "speech.generate")
        configureButton(playButton, title: "播放", action: #selector(play),
                        identifier: "speech.play")
        configureButton(regenerateButton, title: "重新生成（再次计费）",
                        action: #selector(regenerate), identifier: "speech.regenerate")
        configureButton(recheckButton, title: "重新检查",
                        action: #selector(recheck), identifier: "speech.recheck")
        configureButton(closeButton, title: "关闭",
                        action: #selector(closePanel), identifier: "speech.close")
        playButton.isEnabled = false
        regenerateButton.isHidden = true

        // Both stay enabled whatever the load found. Gating them on the same
        // state as generate would make the panel unrecoverable exactly when it
        // is broken: the buttons that fix a bad state would themselves be
        // disabled by that bad state.
        recheckButton.isEnabled = true
        closeButton.isEnabled = true

        // macOS puts the dismissing action on the trailing edge, so a flexible
        // gap pushes 关闭 away from the generation actions.
        let gap = NSView()
        gap.translatesAutoresizingMaskIntoConstraints = false
        gap.setContentHuggingPriority(.defaultLow, for: .horizontal)
        gap.widthAnchor.constraint(greaterThanOrEqualToConstant: 24).isActive = true

        for button in [generateButton, playButton, regenerateButton, recheckButton,
                       gap, closeButton] {
            buttonsStack.addArrangedSubview(button)
        }
        buttonsStack.orientation = NSUserInterfaceLayoutOrientation.horizontal
        buttonsStack.spacing = 12

        let stack = NSStackView(views: [
            title,
            contentKindControl,
            contentPreview,
            characterLabel,
            voiceRow,
            toneStack,
            costLabel,
            statusLabel,
            buttonsStack,
        ])
        stack.orientation = NSUserInterfaceLayoutOrientation.vertical
        stack.alignment = NSLayoutConstraint.Attribute.leading
        stack.spacing = 14
        stack.translatesAutoresizingMaskIntoConstraints = false

        root.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 24),
            stack.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -24),
            stack.topAnchor.constraint(equalTo: root.topAnchor, constant: 20),
            stack.bottomAnchor.constraint(equalTo: root.bottomAnchor, constant: -20),
            contentPreview.widthAnchor.constraint(equalTo: stack.widthAnchor),
            characterLabel.widthAnchor.constraint(equalTo: stack.widthAnchor),
            costLabel.widthAnchor.constraint(equalTo: stack.widthAnchor),
            statusLabel.widthAnchor.constraint(equalTo: stack.widthAnchor),
            voiceRow.widthAnchor.constraint(equalTo: stack.widthAnchor),
        ])

        view = root
        reloadPreview()
    }

    private func toneRow(
        title: String,
        slider: NSSlider,
        value: NSTextField,
        identifier: String
    ) -> NSView {
        let label = NSTextField(labelWithString: title)
        label.setContentHuggingPriority(.defaultLow, for: .horizontal)
        value.textColor = .secondaryLabelColor
        value.alignment = .right
        slider.translatesAutoresizingMaskIntoConstraints = false
        slider.identifier = NSUserInterfaceItemIdentifier(identifier)
        slider.widthAnchor.constraint(equalToConstant: 220).isActive = true

        let row = NSStackView(views: [label, slider, value])
        row.orientation = NSUserInterfaceLayoutOrientation.horizontal
        row.spacing = 12
        return row
    }

    private func configureButton(
        _ button: NSButton,
        title: String,
        action: Selector,
        identifier: String
    ) {
        button.title = title
        button.bezelStyle = .rounded
        button.target = self
        button.action = action
        button.identifier = NSUserInterfaceItemIdentifier(identifier)
    }

    // MARK: - Loading

    override func viewDidLoad() {
        super.viewDidLoad()
        Task { await load() }
    }

    private func load() async {
        setBusy(true)
        defer { setBusy(false) }

        // The catalog is a network call, so an absent key is reported here
        // rather than being discovered as an opaque auth failure later.
        guard await hasCredential() else {
            statusLabel.stringValue = "尚未配置语音 API Key，请先在「设置 › 语音」中填写。"
            return
        }

        do {
            async let fetchedProfile = speech.profile()
            async let fetchedCatalog = speech.voices()
            let (profileResponse, catalogResponse) = try await (fetchedProfile, fetchedCatalog)

            profile = profileResponse.receipt.profile
            let receipt = catalogResponse.receipt

            // Cleared before the two messages below rather than after, so a
            // successful load cannot leave the previous failure's text on
            // screen. That matters once the panel can be re-checked: without
            // this, configuring the key and pressing 重新检查 would repopulate
            // the pickers while still claiming no key is configured.
            statusLabel.stringValue = ""

            // A stale catalog is usable but says so, because ADR 0007 warns it
            // is not a guarantee of what the account may use.
            if receipt.stale {
                statusLabel.stringValue =
                    "音色目录已过期（取自 \(receipt.fetchedAt ?? "未知时间")），生成前会自动刷新。"
            }
            if receipt.voices.isEmpty {
                statusLabel.stringValue = "音色目录为空，供应商没有返回可用音色。"
            }

            applyProfileDefaults()
            rebuildVoiceMenus()
            reloadPreview()
        } catch let SpeechServiceError.commandFailed(error) {
            statusLabel.stringValue = error.userFacingDescription
        } catch {
            statusLabel.stringValue = error.localizedDescription
        }
    }

    /// Re-runs the whole load so a panel that opened before the key was stored,
    /// or that hit a transient failure, can recover where it stands.
    ///
    /// The previous result is dropped rather than merged: a catalog and an
    /// error from the failed attempt must not survive into the next one, or the
    /// pickers and the message would describe two different states.
    @objc private func recheck() {
        catalog = nil
        profile = nil
        statusLabel.stringValue = "正在重新检查…"
        Task { await load() }
    }

    /// The way out of the sheet.
    ///
    /// `presentAsSheet` attaches the panel to the window without giving it a
    /// title bar, so there is no close box and no menu item that reaches it.
    @objc private func closePanel() {
        if let presentingViewController {
            presentingViewController.dismiss(self)
        } else {
            view.window?.close()
        }
    }

    /// Escape closes the panel, like every other sheet. The inherited
    /// implementation is NSResponder's, which does nothing, so without this the
    /// key silently did nothing.
    override func cancelOperation(_ sender: Any?) {
        closePanel()
    }

    private func applyProfileDefaults() {
        guard let profile else { return }
        speedSlider.doubleValue = profile.speed
        volumeSlider.doubleValue = profile.volume
        pitchSlider.doubleValue = Double(profile.pitch)
        reloadPreview()
    }

    // MARK: - Voice menus

    private func rebuildVoiceMenus() {
        guard let catalog else { return }

        voiceGroupButton.removeAllItems()
        for (index, group) in catalog.groups.enumerated() {
            // An entirely unusable group is shown disabled rather than hidden:
            // hiding it would make the catalog look smaller than it is.
            voiceGroupButton.addItem(withTitle: group.name)
            voiceGroupButton.lastItem?.tag = index
            voiceGroupButton.lastItem?.isEnabled = group.isUsable
        }

        // Start from the stored profile so a saved choice survives reopening,
        // and fall back to the first *usable* group only when the profile's
        // voice is not in the catalog. ADR 0007 allows asking the user to
        // choose here but not silently substituting a different voice at
        // generation time.
        let profileVoiceID = profile?.voiceID
        let startIndex = catalog.group(containing: profileVoiceID).flatMap { group in
            catalog.groups.firstIndex(of: group)
        } ?? catalog.groups.firstIndex(where: \.isUsable) ?? 0
        if catalog.groups.indices.contains(startIndex) {
            voiceGroupButton.selectItem(withTag: startIndex)
        }

        rebuildVariantMenu()
    }

    private func rebuildVariantMenu() {
        guard let catalog, catalog.groups.indices.contains(voiceGroupButton.selectedTag()) else {
            variantButton.removeAllItems()
            return
        }
        let group = catalog.groups[voiceGroupButton.selectedTag()]
        for (index, variant) in group.variants.enumerated() {
            let title = variant.option.subtitle.isEmpty
                ? variant.option.voiceID
                : "\(variant.option.subtitle) · \(variant.option.voiceID)"
            variantButton.addItem(withTitle: title)
            variantButton.lastItem?.tag = index
            if case .unavailable(let reason) = variant.availability {
                // The reason travels with the row: an entry that cannot be
                // used says why instead of turning grey for no stated reason.
                variantButton.lastItem?.isEnabled = false
                variantButton.lastItem?.toolTip = reason
            }
        }
        if let firstAvailable = group.variants.firstIndex(where: \.isAvailable) {
            variantButton.selectItem(withTag: firstAvailable)
        } else if !group.variants.isEmpty {
            variantButton.selectItem(withTag: 0)
        }
        reloadPreview()
    }

    private var selectedVoiceID: String? {
        guard let catalog, catalog.groups.indices.contains(voiceGroupButton.selectedTag()),
              catalog.groups[voiceGroupButton.selectedTag()].variants.indices
                .contains(variantButton.selectedTag()) else {
            return nil
        }
        let variant = catalog.groups[voiceGroupButton.selectedTag()]
            .variants[variantButton.selectedTag()]
        return variant.isAvailable ? variant.voiceID : nil
    }

    // MARK: - Preview and the cost notice

    @objc private func reloadPreview() {
        let kind = selectedContentKind
        let hasNote = text(for: "note") != nil
        // The note segment is enabled only when the annotation actually has a
        // note. Enabling it otherwise would let the user select a content part
        // that does not exist, which the CLI then rejects with
        // SPEECH_CONTENT_UNAVAILABLE after the panel has already promised a
        // character count for it.
        contentKindControl.setEnabled(hasNote, forSegment: 1)
        if !hasNote, contentKindControl.selectedSegment == 1 {
            contentKindControl.selectedSegment = 0
        }

        guard let body = text(for: kind) else {
            contentPreview.stringValue = "（这条标注没有可用文本）"
            characterLabel.stringValue = ""
            refreshActionStates()
            return
        }

        contentPreview.stringValue = body
        characterLabel.stringValue =
            "字符数 \(body.count)（本地上限 \(Self.maximumCharacters)）"
        updateCostNotice(characters: body.count)
        refreshActionStates()
    }

    /// Wording is constrained by the contract: there is no currency, no unit
    /// price and no total, so the panel states the local estimate and that the
    /// provider bills. Showing an amount would be inventing one.
    private func updateCostNotice(characters: Int) {
        let current = profile
        let speed = speedSlider.doubleValue
        let volume = volumeSlider.doubleValue
        let pitch = Int(pitchSlider.doubleValue.rounded())
        let voiceID = selectedVoiceID

        // Wording is constrained by the contract: there is no currency, no unit
        // price and no total, so the panel states the local estimate and that the
        // provider bills. Showing an amount would be inventing one.
        var notice = "生成会调用语音供应商并计费，约 \(characters) 个计费字符。最终金额以供应商账单为准。"
        guard let current else {
            // No profile has loaded, so there is no cached clip to compare
            // against and claiming the parameters "differ" would be inventing a
            // baseline. Say it is the first generation instead.
            notice += "\n这是一次新的生成。"
            costLabel.stringValue = notice
            return
        }
        if !current.toneDiffers(voiceID: voiceID, speed: speed, volume: volume, pitch: pitch) {
            notice += "\n参数与已缓存音频一致，命中缓存时不会产生费用。"
        } else {
            notice += "\n当前参数与已缓存音频不同，会生成新的音频并单独计费。"
        }
        costLabel.stringValue = notice
    }

    @objc private func toneChanged() {
        speedValue.stringValue = String(format: "%.2f", speedSlider.doubleValue)
        volumeValue.stringValue = String(format: "%.2f", volumeSlider.doubleValue)
        pitchValue.stringValue = String(Int(pitchSlider.doubleValue.rounded()))
        updateCostNotice(characters: text(for: selectedContentKind)?.count ?? 0)
    }

    @objc private func voiceGroupChanged() {
        rebuildVariantMenu()
    }

    @objc private func variantChanged() {
        reloadPreview()
    }

    // MARK: - Actions

    @objc private func generate() {
        runGeneration(regenerate: false)
    }

    @objc private func regenerate() {
        runGeneration(regenerate: true)
    }

    private func runGeneration(regenerate: Bool) {
        guard let voiceID = selectedVoiceID, text(for: selectedContentKind) != nil else { return }
        let kind = selectedContentKind

        setBusy(true)
        statusLabel.stringValue = "正在生成…"

        Task {
            defer { setBusy(false) }
            do {
                let receipt = try await speech.generate(
                    assetID: book.id,
                    annotationID: annotation.id,
                    contentKind: kind,
                    voiceID: voiceID,
                    speed: speedSlider.doubleValue,
                    volume: volumeSlider.doubleValue,
                    pitch: Int(pitchSlider.doubleValue.rounded()),
                    regenerate: regenerate
                ).receipt

                generatedClipID = receipt.clipID
                generatedPath = nil
                playButton.isEnabled = false
                regenerateButton.isHidden = true

                if receipt.wasBilled {
                    statusLabel.stringValue = "已生成（clip \(String(receipt.clipID.prefix(12)))…）。"
                } else {
                    statusLabel.stringValue = "已复用缓存音频，没有产生费用。"
                }
                if let warnings = receipt.warnings, !warnings.isEmpty {
                    statusLabel.stringValue += "\n" + warnings
                        .map { "\($0.code)：\($0.message ?? $0.reason ?? "")" }
                        .joined(separator: "\n")
                }
                // Resolve the path now so playback is one click, and so the
                // panel reports an unreadable clip before the user presses it.
                await resolvePlaybackPath(for: receipt.clipID)
            } catch let SpeechServiceError.commandFailed(error) {
                statusLabel.stringValue = error.userFacingDescription
                if error.mayHaveBeenBilled {
                    // The provider may already have charged for this attempt, so
                    // the retry is offered as a distinct, separately-labelled
                    // action rather than as the default button.
                    regenerateButton.isHidden = !(error.code == "SPEECH_RESULT_UNKNOWN")
                }
            } catch {
                statusLabel.stringValue = error.localizedDescription
            }
        }
    }

    private func resolvePlaybackPath(for clipID: String) async {
        do {
            let receipt = try await speech.play(clipID: clipID)
            generatedPath = receipt.path
            playButton.isEnabled = true
        } catch let SpeechServiceError.commandFailed(error) {
            statusLabel.stringValue += "\n音频暂时无法播放：\(error.message)"
        } catch {
            statusLabel.stringValue += "\n音频暂时无法播放：\(error.localizedDescription)"
        }
    }

    @objc private func play() {
        guard let clipID = generatedClipID, let path = generatedPath else { return }
        do {
            try player.play(clipID: clipID, url: URL(fileURLWithPath: path))
            statusLabel.stringValue = "正在播放。"
        } catch {
            statusLabel.stringValue = error.localizedDescription
        }
    }

    private func setBusy(_ value: Bool) {
        busy = value
        refreshActionStates()
    }

    /// The single place that decides which actions are available.
    ///
    /// Generation needs a non-empty text within the local character cap, a
    /// resolved voice, and no work in flight. Play needs a clip whose path the
    /// CLI has already verified. All three conditions are state, so deriving
    /// them once means a second entry point cannot disagree with the first --
    /// and a mutation of the rule shows up in every path that consults it.
    private func refreshActionStates() {
        let characters = text(for: selectedContentKind)?.count ?? 0
        let withinLimit = characters > 0 && characters <= Self.maximumCharacters
        generateButton.isEnabled = !busy && withinLimit && selectedVoiceID != nil
        playButton.isEnabled = !busy && generatedPath != nil
        regenerateButton.isEnabled = !busy && !regenerateButton.isHidden
    }
}
