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
    private let exportRoots: BookExportRootStore

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
    /// One place per outcome, and **one writer each**.
    ///
    /// These were a single `statusLabel`, which meant every later action
    /// overwrote every earlier one: pressing 播放 replaced "已生成（clip …）"
    /// with "正在播放。", and an export replaced both. The user could not tell
    /// whether the panel had generated anything, was playing, or had written a
    /// file -- only what happened last.
    ///
    /// Splitting is only worth anything if the split is enforced, so the rule
    /// is that a label is written by exactly one code path: `setupLabel` by the
    /// load, `generationLabel` by generation, `playbackLabel` by playback,
    /// `exportLabel` by export. Nothing reaches across. A new generation clears
    /// the two that described the previous clip, because they *are* stale then
    /// -- but it never writes into the others.
    ///
    /// `setupLabel` is separate rather than folded into `generationLabel` on
    /// purpose: a missing key or an empty catalog is a precondition, and
    /// `load()` clears its area on success. Sharing a label with the
    /// generation result would mean pressing 重新检查 wiped a generation the
    /// user had just paid for.
    private let setupLabel = NSTextField(labelWithString: "")
    private let generationLabel = NSTextField(labelWithString: "")
    private let playbackLabel = NSTextField(labelWithString: "")
    private let exportLabel = NSTextField(labelWithString: "")
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
    /// Copies the generated clip into the book's export directory, the same
    /// "pick a directory, write into it" shape Share Card uses. Not a free-form
    /// "save anywhere": ADR 0007 keeps the audio and the exported Markdown as
    /// one self-consistent bundle, so the target is the book's export root and
    /// the Speech Export Manifest is refreshed with it.
    private let exportButton = NSButton(frame: .zero)
    /// Where the audio would go, and what to do about it when there is nowhere
    /// yet.
    ///
    /// `speech export` accepts any writable directory, so a panel that only said
    /// "choose a folder" invited the one choice that silently produces an
    /// orphan: a directory that is not this book's export root, where the audio
    /// lands but no exported note will ever link to it. Naming the directory up
    /// front -- and saying plainly when there is not one yet -- is what turns
    /// that from a silent mistake into a visible state.
    private let exportTargetLabel = NSTextField(labelWithString: "")

    /// The clips this annotation already has, behind a disclosure.
    ///
    /// A Speech Clip reads *one part* of an annotation, and the clip ID is a
    /// fingerprint over that text plus the voice and tone -- so changing any
    /// slider produces a different clip and a separate charge. One annotation
    /// can therefore own several clips, and until now the panel could only ever
    /// talk about the one it had just made. Reopening the panel showed nothing
    /// at all, which read as "this was never generated" when it was.
    ///
    /// Collapsed by default: for the common case of one clip it is a single
    /// line, and the cost notice already carries the count. Expanded it lists
    /// every ready clip with its own play and export, because those are the
    /// two things you can do with audio you did not just pay for.
    private let clipsToggle = NSButton(frame: .zero)
    private let clipsList = NSStackView()
    private let clipsErrorLabel = NSTextField(labelWithString: "")

    // State
    private var catalog: SpeechVoiceCatalog?
    private var profile: SpeechProfile?
    /// The clip the main 播放 / 导出 buttons act on, **with the content kind it
    /// belongs to**.
    ///
    /// Not "the last clip generated" and not "the first row": it is whatever the
    /// user last acted on. The content kind is stored rather than looked up in
    /// `existingClips` for two reasons.
    ///
    /// It is what makes the content-kind switch safe. Generating a highlight,
    /// switching to the note and pressing 导出 used to write the *highlight's*
    /// audio, because nothing tied the selection to the part of the annotation
    /// on screen -- the kind switch only redrew the preview. Pairing the two
    /// turns "is this selection still about what the user is looking at?" into
    /// a value comparison.
    ///
    /// And it cannot be answered from the list, because a clip generated during
    /// this panel session is not in `existingClips` yet: that list is a snapshot
    /// of what the cache held when the panel loaded. Looking it up would make
    /// a fresh selection read as "unknown kind" and clear it immediately.
    private var selectedClip: SelectedClip?
    private struct SelectedClip {
        let id: String
        /// `highlight` or `note`.
        let contentKind: String
    }
    /// clip ID -> verified path, for every clip the panel has resolved.
    ///
    /// A map rather than one `generatedPath` because the list has several live
    /// clips at once and each play/export needs its own.
    private var clipPaths: [String: String] = [:]
    private var existingClips: [SpeechClipSummary] = []
    private var busy = false

    /// Internal so the probe can name a row the same way the panel does. Built
    /// from the clip ID rather than an index because the list is rebuilt from
    /// the cache report and indices move; a probe that guessed an index would
    /// silently start addressing a different clip when the list order changed.
    static func clipRowIdentifier(_ clipID: String) -> String {
        "speech.clip-row.\(clipID.prefix(12))"
    }

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
        hasCredential: @escaping () async -> Bool,
        exportRoots: BookExportRootStore = .shared
    ) {
        self.book = book
        self.annotation = annotation
        self.speech = speech
        self.player = player
        self.hasCredential = hasCredential
        self.exportRoots = exportRoots
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
        // One computed size, shared by the window request and the width
        // constraint below, so the two cannot disagree.
        let size = Self.panelSize()

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

        // Same treatment as the other wrapping labels, for the same reason: a
        // single-line intrinsic width here would stretch the sheet, and these
        // hold filesystem paths and provider messages.
        for (label, name) in [(setupLabel, "speech.setup-result"),
                              (generationLabel, "speech.generation-result"),
                              (playbackLabel, "speech.playback-result"),
                              (exportLabel, "speech.export-result")] {
            label.textColor = .secondaryLabelColor
            label.maximumNumberOfLines = 0
            label.lineBreakMode = .byWordWrapping
            label.identifier = NSUserInterfaceItemIdentifier(name)
        }

        exportTargetLabel.textColor = .secondaryLabelColor
        exportTargetLabel.maximumNumberOfLines = 0
        exportTargetLabel.lineBreakMode = .byWordWrapping
        exportTargetLabel.identifier = NSUserInterfaceItemIdentifier("speech.export-target")
        refreshExportTarget()

        clipsToggle.title = "已有的语音（0 条）"
        clipsToggle.bezelStyle = .rounded
        clipsToggle.setButtonType(.momentaryPushIn)
        clipsToggle.target = self
        clipsToggle.action = #selector(toggleClipsSection)
        clipsToggle.identifier = NSUserInterfaceItemIdentifier("speech.clips-section")
        clipsToggle.alignment = .left

        clipsErrorLabel.textColor = .secondaryLabelColor
        clipsErrorLabel.maximumNumberOfLines = 0
        clipsErrorLabel.lineBreakMode = .byWordWrapping
        clipsErrorLabel.identifier = NSUserInterfaceItemIdentifier("speech.clips-error")
        clipsErrorLabel.isHidden = true

        clipsList.orientation = NSUserInterfaceLayoutOrientation.vertical
        clipsList.alignment = NSLayoutConstraint.Attribute.leading
        clipsList.spacing = 8
        clipsList.identifier = NSUserInterfaceItemIdentifier("speech.clips-list")
        clipsList.isHidden = true
        // Hidden until the cache says otherwise. The first load is async, and
        // without this the panel spends that window showing 「已有的语音（0 条）」
        // for an annotation that already has audio.
        clipsToggle.isHidden = true

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
        configureButton(exportButton, title: "导出音频",
                        action: #selector(exportAudio), identifier: "speech.export")
        playButton.isEnabled = false
        regenerateButton.isHidden = true

        // Both stay enabled whatever the load found. Gating them on the same
        // state as generate would make the panel unrecoverable exactly when it
        // is broken: the buttons that fix a bad state would themselves be
        // disabled by that bad state.
        recheckButton.isEnabled = true
        closeButton.isEnabled = true
        // Unlike the two above, export has nothing to repair and everything to
        // act on, so it follows the ordinary rule: no clip, nothing to export.
        exportButton.isEnabled = false

        // macOS puts the dismissing action on the trailing edge, so a flexible
        // gap pushes 关闭 away from the generation actions.
        let gap = NSView()
        gap.translatesAutoresizingMaskIntoConstraints = false
        gap.setContentHuggingPriority(.defaultLow, for: .horizontal)
        gap.widthAnchor.constraint(greaterThanOrEqualToConstant: 24).isActive = true

        for button in [generateButton, playButton, regenerateButton, recheckButton,
                       exportButton, gap, closeButton] {
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
            setupLabel,
            generationLabel,
            playbackLabel,
            exportTargetLabel,
            exportLabel,
            clipsToggle,
            clipsErrorLabel,
            clipsList,
            buttonsStack,
        ])
        stack.orientation = NSUserInterfaceLayoutOrientation.vertical
        stack.alignment = NSLayoutConstraint.Attribute.leading
        stack.spacing = 14
        stack.translatesAutoresizingMaskIntoConstraints = false

        // The content lives in a scroll view, and that is the whole fix for a
        // second overflow that `preferredContentSize` could not reach.
        //
        // A sheet is sized from its content, and an unlimited-line wrapping
        // label reports its full single-line width as its intrinsic width. A
        // provider error carrying a file path -- "the Exported Speech Clip in
        // /Users/…/100 Go Mistakes … was not used: the manifest belongs to
        // asset_id …" -- therefore opened a panel 1970pt wide with both edges
        // off a 1512pt display, while its height stayed at the declared 500.
        // Declaring a size only fixes the *initial* size; nothing stopped the
        // window growing past it.
        //
        // Inside a scroll view the document's width comes from the scroll
        // view's own layout rather than from the labels' intrinsic widths, so
        // the message wraps at the panel width instead of stretching the
        // window, and anything taller scrolls. Pinning a width constant alone
        // would only trade the horizontal overflow for a vertical one, because
        // wrapped text is taller.
        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = false
        scroll.drawsBackground = false
        scroll.translatesAutoresizingMaskIntoConstraints = false

        let document = NSView()
        document.translatesAutoresizingMaskIntoConstraints = false
        scroll.documentView = document
        document.addSubview(stack)

        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: document.leadingAnchor, constant: 24),
            stack.trailingAnchor.constraint(equalTo: document.trailingAnchor, constant: -24),
            stack.topAnchor.constraint(equalTo: document.topAnchor, constant: 20),
            stack.bottomAnchor.constraint(equalTo: document.bottomAnchor, constant: -20),
            contentPreview.widthAnchor.constraint(equalTo: stack.widthAnchor),
            characterLabel.widthAnchor.constraint(equalTo: stack.widthAnchor),
            costLabel.widthAnchor.constraint(equalTo: stack.widthAnchor),
            setupLabel.widthAnchor.constraint(equalTo: stack.widthAnchor),
            generationLabel.widthAnchor.constraint(equalTo: stack.widthAnchor),
            playbackLabel.widthAnchor.constraint(equalTo: stack.widthAnchor),
            exportTargetLabel.widthAnchor.constraint(equalTo: stack.widthAnchor),
            exportLabel.widthAnchor.constraint(equalTo: stack.widthAnchor),
            clipsToggle.widthAnchor.constraint(equalTo: stack.widthAnchor),
            clipsErrorLabel.widthAnchor.constraint(equalTo: stack.widthAnchor),
            clipsList.widthAnchor.constraint(equalTo: stack.widthAnchor),
            voiceRow.widthAnchor.constraint(equalTo: stack.widthAnchor),
            // The document's width is an absolute constant, not a chain of
            // relative constraints. A relative chain bounds the *layout* but
            // not the *fitting size*: nothing pinned a number, so the solver
            // still satisfied every constraint at the largest intrinsic width
            // available, and that is what the sheet sizes itself from --
            // 1980pt for a 537-character message. Measured, not reasoned.
            //
            // It also has to agree with `preferredContentSize`, so both read
            // the same computed value.
            document.widthAnchor.constraint(equalToConstant: size.width),
            // At least the visible height, so a short panel does not get a
            // document shorter than its own viewport.
            document.heightAnchor.constraint(greaterThanOrEqualTo: scroll.contentView.heightAnchor),
        ])

        root.addSubview(scroll)
        NSLayoutConstraint.activate([
            scroll.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            scroll.topAnchor.constraint(equalTo: root.topAnchor),
            scroll.bottomAnchor.constraint(equalTo: root.bottomAnchor),
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

        // Before the credential check, and on its own terms: `speech cache
        // status` is a local read with no provider call, and listing, playing
        // and exporting audio that already exists needs no key at all. Gating
        // it behind `hasCredential()` would hide the user's own files until
        // they configured something they may not need in order to hear them.
        await loadExistingClips()

        // The catalog is a network call, so an absent key is reported here
        // rather than being discovered as an opaque auth failure later.
        guard await hasCredential() else {
            setupLabel.stringValue = "准备：尚未配置语音 API Key。已有音频仍可播放和导出；生成前请到「设置 › 语音」填写。"
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
            //
            // Only this label. A generation the user already paid for is not
            // invalidated by the catalog being re-read.
            setupLabel.stringValue = ""

            // A stale catalog is usable but says so, because ADR 0007 warns it
            // is not a guarantee of what the account may use.
            if receipt.stale {
                setupLabel.stringValue =
                    "准备：音色目录已过期（取自 \(receipt.fetchedAt ?? "未知时间")），生成前会自动刷新。"
            }
            if receipt.voices.isEmpty {
                setupLabel.stringValue = "准备：音色目录为空，供应商没有返回可用音色。"
            }

            // The receipt is decoded and checked and then, until this line, it
            // was thrown away: `catalog` was only ever assigned nil, so
            // `rebuildVoiceMenus` hit `guard let catalog else { return }` and
            // left both pickers empty -- silently, because the guard returns
            // without a message. The builder was exercised by nine unit tests
            // and called from nowhere in the app.
            //
            // No `unavailableReason`: that argument marks the whole catalog
            // unusable for a reason the account reported, and the panel has no
            // such information. The stored profile being unverified is not one
            // -- the contract validates at generation time, and the CLI does
            // it. Passing it here would disable every row and leave
            // `selectedVoiceID` nil, which is the same broken panel by another
            // route.
            catalog = SpeechVoiceCatalogBuilder.build(from: receipt)

            applyProfileDefaults()
            rebuildVoiceMenus()
            reloadPreview()
        } catch let SpeechServiceError.commandFailed(error) {
            setupLabel.stringValue = "准备：\(error.userFacingDescription)"
        } catch {
            setupLabel.stringValue = "准备：\(error.localizedDescription)"
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
        setupLabel.stringValue = "准备：正在重新检查…"
        Task { await load() }
    }

    // MARK: - Existing clips

    /// Reads the cache and rebuilds the disclosure.
    ///
    /// Failures land in their own label rather than in the setup area: the
    /// voice catalog and the API key are the generation preconditions, and this
    /// list is not part of them. A cache read that fails must not read as "you
    /// have no audio", so the disclosure keeps whatever it showed and this
    /// area says the list could not be refreshed.
    private func loadExistingClips() async {
        do {
            let entries = try await speech.cacheStatus()
            existingClips = entries.playable(
                forAssetID: book.id,
                annotationID: annotation.id
            )
            clipsErrorLabel.stringValue = ""
            clipsErrorLabel.isHidden = true
            // Paths for clips the panel no longer lists would accumulate: the
            // list is a snapshot, and a clip generated here is added below
            // rather than re-read.
            clipPaths = clipPaths.filter { id, _ in
                existingClips.contains { $0.clipID == id } || id == selectedClip?.id
            }
            await resolvePathsForListedClips()
            rebuildClipsSection()
            // The count is part of the cost sentence, so it has to be recomputed
            // now rather than at the next tone nudge.
            reloadPreview()
        } catch let SpeechServiceError.commandFailed(error) {
            showClipsError(error.userFacingDescription)
        } catch {
            showClipsError(error.localizedDescription)
        }
    }

    private func showClipsError(_ message: String) {
        clipsErrorLabel.stringValue = "已有音频：读取失败（\(message)）。"
        clipsErrorLabel.isHidden = false
    }

    /// One local resolve per listed clip, so a row's play button is either
    /// already live or honestly disabled.
    ///
    /// Bounded by the number of clips one annotation owns, which is one per
    /// distinct text + voice + tone combination the user has tried. Each call is
    /// local and cheap, but the loop is still sequential and awaited: this runs
    /// inside the panel's load, and a burst of parallel subprocesses for a list
    /// that is usually empty or one row would be a poor trade.
    private func resolvePathsForListedClips() async {
        for clip in existingClips where clipPaths[clip.clipID] == nil {
            await resolvePlaybackPath(for: clip.clipID)
        }
    }

    private func rebuildClipsSection() {
        clipsToggle.title = existingClips.isEmpty
            ? "已有的语音（0 条）"
            : "已有的语音（\(existingClips.count) 条）"
        // Nothing to expand. Hidden rather than showing 「0 条」, so an
        // annotation that has never been generated does not carry a disclosure
        // that opens onto an empty area.
        clipsToggle.isHidden = existingClips.isEmpty
        clipsList.isHidden = true
        clipsErrorLabel.isHidden = clipsErrorLabel.stringValue.isEmpty

        for view in clipsList.arrangedSubviews {
            clipsList.removeArrangedSubview(view)
            view.removeFromSuperview()
        }
        for clip in existingClips {
            clipsList.addArrangedSubview(makeClipRow(clip))
        }
        refreshActionStates()
    }

    @objc private func toggleClipsSection() {
        guard !existingClips.isEmpty else { return }
        clipsList.isHidden = !clipsList.isHidden
        clipsToggle.title = clipsList.isHidden
            ? "已有的语音（\(existingClips.count) 条）▸"
            : "已有的语音（\(existingClips.count) 条）▾"
    }

    /// One clip: what it is, how long it is, and the two things you can do with
    /// audio you did not just pay for.
    ///
    /// The play and export controls are this row's own, not the panel's main
    /// ones. A shared pair would mean the row list could not be read without
    /// also changing what 导出 acts on, and the label the user just clicked and
    /// the file that gets written would be able to disagree.
    private func makeClipRow(_ clip: SpeechClipSummary) -> NSView {
        let kindLabel = NSTextField(labelWithString: Self.label(for: clip.contentKind))
        kindLabel.setContentHuggingPriority(.defaultHigh, for: .horizontal)

        var detail = String(clip.clipID.prefix(12)) + "…"
        if let ms = clip.durationMs, ms > 0 {
            detail = Self.durationText(ms) + " · " + detail
        }
        let detailLabel = NSTextField(labelWithString: detail)
        detailLabel.textColor = .secondaryLabelColor
        detailLabel.setContentHuggingPriority(.defaultLow, for: .horizontal)

        let play = SpeechClipButton(clipID: clip.clipID, role: .play)
        configureButton(play, title: "播放", action: #selector(playExisting),
                        identifier: Self.clipRowIdentifier(clip.clipID) + ".play")
        let export = SpeechClipButton(clipID: clip.clipID, role: .export)
        configureButton(export, title: "导出", action: #selector(exportExisting),
                        identifier: Self.clipRowIdentifier(clip.clipID) + ".export")

        let row = NSStackView(views: [kindLabel, detailLabel, play, export])
        row.orientation = NSUserInterfaceLayoutOrientation.horizontal
        row.spacing = 10
        row.alignment = .firstBaseline
        row.identifier = NSUserInterfaceItemIdentifier(Self.clipRowIdentifier(clip.clipID))
        applyRowControl(play)
        applyRowControl(export)
        return row
    }

    /// `11412` -> `11.4 秒`. Only for a value the contract actually gave; a
    /// `null` duration prints nothing rather than a fabricated `0.0 秒`.
    static func durationText(_ milliseconds: Int) -> String {
        String(format: "%.1f 秒", Double(milliseconds) / 1000.0)
    }

    @objc private func playExisting(_ sender: SpeechClipButton) {
        select(sender.clipID, contentKind: contentKind(of: sender.clipID))
        playSelected()
    }

    @objc private func exportExisting(_ sender: SpeechClipButton) {
        select(sender.clipID, contentKind: contentKind(of: sender.clipID))
        exportSelectedClip()
    }

    /// The kind a listed clip belongs to, falling back to the currently selected
    /// kind for a clip generated during this session (which is not in the list).
    private func contentKind(of clipID: String) -> String {
        existingClips.first { $0.clipID == clipID }?.contentKind ?? selectedContentKind
    }

    private func select(_ clipID: String, contentKind: String) {
        selectedClip = SelectedClip(id: clipID, contentKind: contentKind)
        refreshActionStates()
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
        let kind = selectedContentKind

        // A selection made under the other kind is not about what is on screen
        // any more, and its play/export results described audio the panel is no
        // longer offering. This is the fix for exporting a highlight while the
        // note is selected; it belongs here, in the one place that runs on a
        // kind switch, rather than in the segmented control's action.
        if let selection = selectedClip, selection.contentKind != kind {
            clearSelection()
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

    /// Forget the current selection, and the two results that described it.
    ///
    /// Same rule as a new generation: those two areas were true *of that clip*
    /// and stop being true once the panel is looking at a different part of the
    /// annotation. Only those two -- the generation result stays, because the
    /// audio the user paid for did not go anywhere.
    private func clearSelection() {
        selectedClip = nil
        playbackLabel.stringValue = ""
        exportLabel.stringValue = ""
    }

    /// Wording is constrained by what the panel can actually know.
    ///
    /// It used to compare the pickers against the stored Voice Profile and
    /// announce 「参数与已缓存音频一致，命中缓存时不会产生费用」. That claim was
    /// wrong in both directions, and the profile was the wrong thing to compare
    /// against:
    ///
    /// - The profile describes the voice and tone, but `clip_id` is a
    ///   fingerprint over the *text* as well. Identical tone on a different
    ///   annotation -- or on the other half of this one -- is a different clip,
    ///   so 「一致」 did not mean 「命中缓存」.
    /// - When no profile had loaded, the panel said 「这是一次新的生成」, which is
    ///   exactly backwards: no profile is a reason to know *nothing* about
    ///   whether the audio exists, not a reason to claim it does not.
    ///
    /// The cache report settles it instead, because it says which clips belong
    /// to *this annotation*. What it still cannot say is which voice and tone
    /// each of them used, so the reuse sentence is conditional rather than a
    /// prediction -- the panel has no way to know the fingerprint it would
    /// produce.
    private func updateCostNotice(characters: Int) {
        var notice = "生成这条音频约 \(characters) 个计费字符，最终金额以供应商账单为准。"
        notice += "\n" + reuseAdvice()
        costLabel.stringValue = notice
    }

    private func reuseAdvice() -> String {
        let kind = selectedContentKind
        let label = kind == "note" ? "笔记" : "高亮"

        guard !existingClips.isEmpty else {
            return "这条标注还没有音频，这次会是它的第一条。"
        }

        let sameKind = existingClips.filter { $0.contentKind == kind }
        guard sameKind.isEmpty else {
            return "这条标注已有 \(existingClips.count) 条音频"
                + "（\(clipBreakdown())）。当前的音色和音调若与其中一条完全相同，"
                + "会直接复用、不产生费用；改了就是新的一条，单独计费。"
        }
        return "这条标注已有 \(existingClips.count) 条音频，但没有\(label)这一条。"
            + "\(label)会是新的一条，单独计费。"
    }

    /// 「高亮 1 · 笔记 1」, counting only the kinds actually present so the
    /// sentence never names a kind the annotation does not have.
    private func clipBreakdown() -> String {
        var counts: [String: Int] = [:]
        for clip in existingClips { counts[clip.contentKind, default: 0] += 1 }
        return counts.keys.sorted().map { "\(Self.label(for: $0)) \(counts[$0]!)" }
            .joined(separator: " · ")
    }

    static func label(for contentKind: String) -> String {
        contentKind == "note" ? "笔记" : "高亮"
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
        generationLabel.stringValue = "生成：正在生成…"
        // The other two described the previous clip, which this run replaces.
        // Clearing them is not the overwrite the split is about -- it is the
        // one case where the old text is genuinely untrue rather than merely
        // in the way. A generation the user has already paid for is untouched
        // by 重新检查, which only ever writes the setup label.
        clearSelection()

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

                // Selected together with its kind, so a later switch to the
                // other part of the annotation drops it instead of leaving
                // 导出 pointing at audio from the part no longer on screen.
                select(receipt.clipID, contentKind: kind)
                // This is a new clip for the panel, so it joins the list. Not
                // by re-reading the cache -- the receipt already says what came
                // back, and a re-read would race the write the CLI just did.
                adoptGeneratedClip(receipt.clipID, kind: kind)
                regenerateButton.isHidden = true
                refreshActionStates()

                if receipt.wasBilled {
                    generationLabel.stringValue = "生成：已生成（clip \(String(receipt.clipID.prefix(12)))…）。"
                } else {
                    generationLabel.stringValue = "生成：已复用缓存音频，没有产生费用。"
                }
                if let warnings = receipt.warnings, !warnings.isEmpty {
                    generationLabel.stringValue += "\n" + warnings
                        .map { "\($0.code)：\($0.message ?? $0.reason ?? "")" }
                        .joined(separator: "\n")
                }
                // Resolve the path now so playback is one click, and so the
                // panel reports an unreadable clip before the user presses it.
                await resolvePlaybackPath(for: receipt.clipID)
            } catch let SpeechServiceError.commandFailed(error) {
                generationLabel.stringValue = "生成：\(error.userFacingDescription)"
                if error.mayHaveBeenBilled {
                    // The provider may already have charged for this attempt, so
                    // the retry is offered as a distinct, separately-labelled
                    // action rather than as the default button.
                    regenerateButton.isHidden = !(error.code == "SPEECH_RESULT_UNKNOWN")
                }
            } catch {
                generationLabel.stringValue = "生成：\(error.localizedDescription)"
            }
        }
    }

    private func resolvePlaybackPath(for clipID: String) async {
        do {
            let receipt = try await speech.play(clipID: clipID)
            clipPaths[clipID] = receipt.path
            refreshActionStates()
        } catch let SpeechServiceError.commandFailed(error) {
            // A clip whose audio cannot be resolved is not a playback result:
            // the user did not press play, and the main play button stays on
            // whatever it was. Only the row for that clip loses its button.
            // The generation result is untouched either way -- see
            // `checkGenerationSurvivesAPathFailure`.
            clipPaths.removeValue(forKey: clipID)
            refreshRowControlsInSection(clipID: clipID)
            if selectedClip?.id == clipID {
                playbackLabel.stringValue = "播放：音频暂时无法播放（\(error.message)）。"
            }
        } catch {
            clipPaths.removeValue(forKey: clipID)
            refreshRowControlsInSection(clipID: clipID)
            if selectedClip?.id == clipID {
                playbackLabel.stringValue = "播放：音频暂时无法播放（\(error.localizedDescription)）。"
            }
        }
    }

    /// Rebuilds one row's controls after its path changed.
    private func refreshRowControlsInSection(clipID: String) {
        for subview in clipsList.arrangedSubviews {
            for case let button as SpeechClipButton in subview.subviews
            where button.clipID == clipID {
                applyRowControl(button)
            }
        }
    }

    /// The per-role half of ``refreshRowControls(play:export:clipID:)``, so a
    /// single button can be re-evaluated without finding its sibling.
    private func applyRowControl(_ button: SpeechClipButton) {
        switch button.role {
        case .export:
            // Export needs only a clip ID.
            button.isEnabled = !busy
        case .play:
            // Play needs a path the CLI has verified.
            button.isEnabled = !busy && clipPaths[button.clipID] != nil
        }
    }

    /// Adds a clip this session just produced to the list, in place.
    ///
    /// The cache on disk already has it, but the list was read before the
    /// write, so it is merged from the receipt rather than re-read. A re-read
    /// would race the CLI's own write, which is how an earlier half of this
    /// flickered the count: the entry was not yet visible when the report came
    /// back, so the panel said 1 then 2 for the same audio.
    private func adoptGeneratedClip(_ clipID: String, kind: String) {
        if let index = existingClips.firstIndex(where: { $0.clipID == clipID }) {
            let durationMs = existingClips[index].durationMs
            existingClips[index] = SpeechClipSummary(
                clipID: clipID, contentKind: kind, durationMs: durationMs
            )
        } else {
            // Duration is `nil` rather than invented: the generate receipt
            // carries its own audio spec, but the cache entry's duration is what
            // the list means by it, and nothing has re-read that. A missing
            // duration prints as a shorter row, not as a wrong number.
            existingClips.append(
                SpeechClipSummary(clipID: clipID, contentKind: kind, durationMs: nil)
            )
        }
        rebuildClipsSection()
    }

    @objc private func play() {
        playSelected()
    }

    /// The one play path, shared by the main button and every row.
    ///
    /// Two copies is how the row and the main button would come to disagree
    /// about which clip is current; there is one selection and one play.
    private func playSelected() {
        guard let selection = selectedClip, let path = clipPaths[selection.id] else { return }
        do {
            try player.play(clipID: selection.id, url: URL(fileURLWithPath: path))
            playbackLabel.stringValue = "播放：正在播放。"
        } catch {
            playbackLabel.stringValue = "播放：\(error.localizedDescription)"
        }
    }

    /// Where this book's audio belongs, right now.
    ///
    /// Internal rather than private so the probe can assert the decision itself
    /// for every state of the store, rather than only reading the label that
    /// describes it. A label says what the panel claims; this says what it will
    /// do.
    enum ExportTarget {
        /// A directory that exists and is the one a previous export used.
        case known(URL)
        /// Nothing recorded, or the recorded directory has since gone away.
        case missing
    }

    var exportTarget: ExportTarget {
        exportRoots.existingExportRoot(forAssetID: book.id).map(ExportTarget.known) ?? .missing
    }

    /// Ask for the book's export directory, then hand the whole thing to the
    /// CLI. The panel does not copy the file itself: ADR 0007 makes the Rust
    /// core the only writer of the export bundle, because the audio and the
    /// Markdown that links to it have to stay one self-consistent artifact.
    ///
    /// With a recorded root this is one click and no dialog. Without one it is
    /// two: there is nothing to offer, and saying "choose a folder" is what
    /// produced the orphan in the first place.
    /// The one export path, shared by the main button and every row.
    ///
    /// Both arrive here through `selectedClip`, and a row selects before it
    /// calls, so the file that gets written is always the clip whose label the
    /// user pressed.
    @objc private func exportAudio() {
        exportSelectedClip()
    }

    private func exportSelectedClip() {
        // No selection means no clip, and the button that would get here is
        // disabled. The guard is for the row buttons, which the section can
        // outlive: a list rebuilt from a failed re-read can leave a row whose
        // clip is no longer the current one.
        guard let selection = selectedClip else { return }
        let clipID = selection.id

        switch exportTarget {
        case .known(let root):
            // Deliberately not gated on a window. There is nothing to ask on
            // this path, and requiring a window would make the only correct
            // action unavailable anywhere the panel is not presented as a sheet.
            runExport(clipID: clipID, to: root)
        case .missing:
            guard let window = view.window else { return }
            let alert = NSAlert()
            alert.alertStyle = .informational
            alert.messageText = "还不知道这本书的导出目录"
            alert.informativeText = """
            语音音频要写进这本书的导出目录，也就是 Markdown 已经导出到的那一层；\
            写进别处，Markdown 不会生成链接，这份音频就没人引用。

            先回到书籍详情「导出」本书的 Markdown，之后回到这里再导出音频。
            """
            alert.addButton(withTitle: "选择导出目录…")
            alert.addButton(withTitle: "取消")
            alert.beginSheetModal(for: window) { [weak self] response in
                guard response == .alertFirstButtonReturn else { return }
                self?.chooseExportDirectory(clipID: clipID)
            }
        }
    }

    /// The directory picker, for the case where there is no record -- or where
    /// the user wants to send this book somewhere else.
    ///
    /// The title says which layer to pick because the wrong one is not obvious
    /// from the result: writing into `assets/audio/` itself, or into a parent
    /// that holds no Markdown, both "work" and both are wrong.
    private func chooseExportDirectory(clipID: String) {
        guard let window = view.window else { return }
        let panel = NSOpenPanel()
        panel.title = "选择这本书的导出目录（Markdown 所在的那一层，不要选 assets/audio）"
        panel.prompt = "导出"
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.beginSheetModal(for: window) { [weak self] response in
            guard response == .OK, let directoryURL = panel.url else { return }
            self?.performExport(clipID: clipID, to: directoryURL)
        }
    }

    /// Last gate before the CLI writes: does this directory look like a book
    /// export root?
    ///
    /// The check is deliberately a warning rather than a refusal. Exporting the
    /// audio *before* the Markdown is legitimate -- the manifest is read by the
    /// next Markdown export, which then links it -- so a directory with no
    /// `.md` yet is a legitimate thing to write into. What is never legitimate
    /// is doing it without knowing, because the failure is invisible afterwards:
    /// the export succeeds, the receipt is a success, and the audio is an orphan
    /// that no note links to. So the panel states the consequence and lets the
    /// user decide.
    ///
    /// Internal so the probe can drive it directly: `NSOpenPanel` cannot be
    /// driven headlessly, so without this the "remember the directory the user
    /// chose" behaviour -- the thing that makes a manual choice stick for next
    /// time -- would be unreachable from any check.
    func performExport(clipID: String, to directoryURL: URL) {
        guard !containsExportedMarkdown(directoryURL) else {
            runExport(clipID: clipID, to: directoryURL)
            return
        }
        guard let window = view.window else { return }
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = "这个目录里没有 Markdown"
        alert.informativeText = """
        音频会写进 \(directoryURL.path)/assets/audio/。

        只有把本书的 Markdown 导出到同一层目录，之后的导出才会生成音频链接；\
        否则这份音频不会被任何笔记引用。
        """
        alert.addButton(withTitle: "返回")
        alert.addButton(withTitle: "仍要导出")
        alert.beginSheetModal(for: window) { [weak self] response in
            guard response == .alertSecondButtonReturn else { return }
            self?.runExport(clipID: clipID, to: directoryURL)
        }
    }

    /// Whether the directory already holds a book's Markdown. An unreadable or
    /// missing directory answers false, which is the safe direction: it sends
    /// the user through the confirmation rather than writing silently.
    ///
    /// Internal so the probe can exercise it against real directories. The
    /// behaviour worth testing here is not "a label says the right thing" but
    /// "a directory with no notes in it is recognised as having no notes in it",
    /// including the two shapes that are easy to get wrong: a path that does
    /// not exist, and a path that is a file.
    func containsExportedMarkdown(_ directoryURL: URL) -> Bool {
        guard let entries = try? FileManager.default.contentsOfDirectory(
            at: directoryURL,
            includingPropertiesForKeys: nil
        ) else {
            return false
        }
        return entries.contains { $0.pathExtension.lowercased() == "md" }
    }

    private func runExport(clipID: String, to directoryURL: URL) {
        setBusy(true)
        defer { setBusy(false) }
        Task {
            do {
                let receipt = try await speech.export(
                    clipID: clipID,
                    to: directoryURL.path
                )
                // A successful export proves this directory accepts this book's
                // audio, so it is the root to offer from now on.
                exportRoots.record(assetID: book.id, exportRoot: directoryURL)
                let where_ = receipt.reused ? "已存在相同文件，未重复写入" : "已导出"
                exportLabel.stringValue = "导出：\(where_)，写入 \(receipt.relativePath)"
            } catch let SpeechServiceError.commandFailed(error) {
                exportLabel.stringValue = "导出：\(error.userFacingDescription)"
            } catch {
                exportLabel.stringValue = "导出：\(error.localizedDescription)"
            }
            refreshExportTarget()
            refreshActionStates()
        }
    }

    /// The export target, or the reason there is not one.
    ///
    /// Reads the same `exportTarget` the button acts on, so the sentence on
    /// screen and the action cannot describe different things.
    ///
    /// The wording is 「还不知道」 rather than 「还没有」 on purpose: the panel
    /// knows what *this app* has exported, and nothing else. A book exported
    /// from the CLI, or before this version existed, does have an export root
    /// the panel cannot see -- and telling the user it does not would be a
    /// claim it has no way to make.
    private func refreshExportTarget() {
        switch exportTarget {
        case .known(let root):
            exportTargetLabel.stringValue = "导出目录：\(root.path)"
        case .missing:
            exportTargetLabel.stringValue = "还不知道这本书的导出目录。在书籍详情「导出」本书的 Markdown 之后，"
                + "这里会直接写进那一层；也可以点「导出音频」自己选一个。"
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
        // Both main buttons act on the selection, so both are off when there
        // is none -- which is exactly the state a content-kind switch leaves
        // behind, and the one that used to keep pointing at the other part's
        // audio.
        playButton.isEnabled = !busy && selectedClip.map { clipPaths[$0.id] != nil } == true
        exportButton.isEnabled = !busy && selectedClip != nil
        regenerateButton.isEnabled = !busy && !regenerateButton.isHidden
        refreshAllRowControls()
    }

    private func refreshAllRowControls() {
        for subview in clipsList.arrangedSubviews {
            for case let button as SpeechClipButton in subview.subviews {
                applyRowControl(button)
            }
        }
    }
}

/// A row's play or export control, carrying the clip it belongs to.
///
/// The clip ID travels on the control instead of through the action, because
/// `performClick` and a real click both hand the target a `sender` the panel
/// cannot rely on being a particular row, and a row index would be a second
/// thing that can go stale when the list is rebuilt. The role is carried the
/// same way so a single button can be re-evaluated on its own.
final class SpeechClipButton: NSButton {
    enum Role {
        case play
        case export
    }

    let clipID: String
    let role: Role

    init(clipID: String, role: Role) {
        self.clipID = clipID
        self.role = role
        super.init(frame: .zero)
    }

    required init?(coder: NSCoder) {
        fatalError("不支持从 Interface Builder 加载")
    }
}
