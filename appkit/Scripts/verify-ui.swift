import AppKit

// UI 回归探针。直接编译进真实源码(见 verify-ui.sh),
// 不重建约束,因此不会与实现漂移。

private var failures: [String] = []

/// Collects the result of an async step from inside a `Task`.
///
/// A captured `var` would be an error under the Swift 5.10 toolchain on CI and
/// a non-issue under 6.3 locally, which is precisely the kind of difference
/// that only shows up in one place.
private final class ProbeOutcome: @unchecked Sendable {
    private let lock = NSLock()
    private var error: String?

    func record(_ work: @escaping @Sendable () async throws -> Void) async {
        do {
            try await work()
        } catch {
            lock.lock()
            self.error = "\(error)"
            lock.unlock()
        }
    }

    var errorDescription: String? {
        lock.lock()
        defer { lock.unlock() }
        return error
    }
}

private func check(_ name: String, _ condition: Bool, _ detail: String) {
    print("\(condition ? "  ok  " : " FAIL ") \(name) — \(detail)")
    if !condition { failures.append(name) }
}

private func hosted(_ view: NSView, width: CGFloat, height: CGFloat) {
    let outer = NSView(frame: NSRect(x: 0, y: 0, width: width, height: height))
    view.translatesAutoresizingMaskIntoConstraints = false
    outer.addSubview(view)
    NSLayoutConstraint.activate([
        view.leadingAnchor.constraint(equalTo: outer.leadingAnchor),
        view.topAnchor.constraint(equalTo: outer.topAnchor),
        view.widthAnchor.constraint(equalToConstant: width),
        view.heightAnchor.constraint(equalToConstant: height)
    ])
    outer.layoutSubtreeIfNeeded()
}

@main
enum VerifyLayout {
    static func main() {
        NSApplication.shared.setActivationPolicy(.accessory)

        checkMainMenu()
        checkSettings()
        checkSpeechEntry()
        checkSplitView()
        checkDetailLayout()
        checkAnnotationRowHeight()
        checkSortAccessibility()
        checkAnnotationFilter()
        checkExportScope()
        checkCardEntry()
        checkShareCardRealEntry()
        checkShareCardAlternativeLayout()
        checkShareCardActions()
        checkShareCardFontSelection()
        checkShareCardThemeGrid()
        checkShareCardTypographyAndPages()
        checkClassifier()
        checkBookRowCentering()

        print("\n\(failures.isEmpty ? "全部通过" : "失败 \(failures.count) 项: \(failures.joined(separator: ", "))")")
        exit(failures.isEmpty ? 0 : 1)
    }

    private static func checkMainMenu() {
        print("\n#2 主菜单")
        let menu = MainMenu.build(appName: "Books Exporter")
        check("菜单存在", menu.items.count >= 3, "顶层菜单 \(menu.items.map(\.title))")

        let edit = menu.items.first { $0.title == "编辑" }?.submenu
        let equivalents = Set(edit?.items.map(\.keyEquivalent) ?? [])
        check("Edit 含 ⌘C/⌘V/⌘X/⌘A", equivalents.isSuperset(of: ["c", "v", "x", "a"]),
              "keyEquivalents \(equivalents.filter { !$0.isEmpty }.sorted())")

        let quit = menu.items.first?.submenu?.items.first { $0.keyEquivalent == "q" }
        check("⌘Q 退出", quit?.action == #selector(NSApplication.terminate(_:)), "\(quit?.title ?? "缺失")")

        let settings = menu.items.first?.submenu?.items.first { $0.title == "设置…" }
        check("设置入口 ⌘,", settings?.action == #selector(AppDelegate.showSettings(_:))
                  && settings?.keyEquivalent == ","
                  && settings?.keyEquivalentModifierMask == [.command],
              "\(settings?.title ?? "缺失")")

        let checkForUpdates = menu.items.first?.submenu?.items.first { $0.title == "检查更新…" }
        check("检查更新入口", checkForUpdates?.action == #selector(AppDelegate.checkForUpdates(_:)),
              "\(checkForUpdates?.title ?? "缺失")")
    }

    private static func checkSettings() {
        print("\n设置页")
        let defaults = UserDefaults(suiteName: "books-exporter-verify-settings")!
        defaults.removePersistentDomain(forName: "books-exporter-verify-settings")
        let settings = AppSettingsStore(
            defaults: defaults,
            notificationCenter: NotificationCenter()
        )
        let controller = SettingsViewController(settingsStore: settings)
        controller.loadView()
        guard let popup = view(named: "settings.refresh-interval", in: controller.view) as? NSPopUpButton else {
            check("设置页有自动刷新控件", false, "找不到刷新间隔下拉菜单")
            return
        }

        check("默认每 5 分钟刷新", settings.refreshInterval == .fiveMinutes,
              settings.refreshInterval.displayName)
        check("刷新间隔包含关闭/1/5/15/30/60 分钟",
              popup.itemTitles == RefreshInterval.allCases.map(\.displayName),
              "\(popup.itemTitles)")

        popup.selectItem(withTag: RefreshInterval.fifteenMinutes.rawValue)
        invoke(popup)
        check("修改刷新间隔立即持久化", settings.refreshInterval == .fifteenMinutes,
              settings.refreshInterval.displayName)

        checkSpeechSettings(controller: controller, defaults: defaults)
    }

    /// The speech section, asserted on behaviour rather than on the presence
    /// of controls: the property worth protecting is that the key reaches the
    /// keychain and nothing else. A probe that only checks a text field exists
    /// would stay green if someone switched it to a plain field and started
    /// echoing the value, which is the failure that matters.
    private static func checkSpeechSettings(
        controller: SettingsViewController,
        defaults: UserDefaults
    ) {
        print("\n设置页 · 语音")

        guard let keyField = view(named: "settings.speech-api-key", in: controller.view) else {
            check("设置页有语音密钥输入框", false, "找不到 settings.speech-api-key")
            return
        }
        check("语音密钥输入框存在", true, "settings.speech-api-key")

        check("密钥输入框是安全输入框，不是明文",
              keyField is NSSecureTextField,
              "\(type(of: keyField))")

        guard let channel = view(named: "settings.speech-channel", in: controller.view) as? NSTextField else {
            check("设置页显示支持渠道", false, "找不到 settings.speech-channel")
            return
        }
        // ADR 0007 keeps the provider a read-only statement: SenseAudio is the
        // first, not a choice among verified channels. A pop-up here would
        // imply options the app has never exercised.
        check("支持渠道是只读声明而非可选列表",
              !controller.view.subviews.contains(where: { subview in
                  guard let popup = subview as? NSPopUpButton else { return false }
                  return popup.identifier?.rawValue == "settings.speech-channel"
              }),
              channel.stringValue)

        check("支持渠道只声明已测试的那个",
              channel.stringValue.contains("SenseAudio")
                  && !channel.stringValue.contains("、"),
              channel.stringValue)

        guard view(named: "settings.speech-verify", in: controller.view) != nil else {
            check("设置页有凭据验证按钮", false, "找不到 settings.speech-verify")
            return
        }
        check("凭据验证按钮存在", true, "settings.speech-verify")

        guard view(named: "settings.speech-status", in: controller.view) != nil,
              view(named: "settings.speech-note", in: controller.view) != nil else {
            check("设置页有语音状态与说明", false, "缺少 settings.speech-status 或 settings.speech-note")
            return
        }
        check("语音状态与说明存在", true, "status + note")

        // Behavioural, and it has to actually drive the store. Comparing the
        // key set before and after *doing nothing* would pass forever, which
        // is the same failure mode as a guard that cannot fail.
        let defaultsBefore = Set(defaults.dictionaryRepresentation().keys)
        let keychain = InMemoryKeychainStore()
        let profileJSON = """
        {"schema_version":1,"receipt":{"operation":"profile_show","profile":\
        {"provider":"senseaudio","model":"sensenova-tts-2.0","voice_id":"male_0004_a",\
        "emotion_label":null,"style_label":null,"speed":1.0,"volume":1.0,"pitch":0,\
        "verification_status":"unverified","verified_at":null,\
        "audio":{"format":"mp3","sample_rate":32000,"bitrate":128000,"channel":2}},\
        "api_key_env":"SENSEAUDIO_API_KEY","config_path":null,"warnings":[]}}
        """
        let client = RustCLIClient(
            executableURL: URL(fileURLWithPath: "/tmp/apple-books-exporter"),
            runner: { _, _, _ in .success(profileJSON) }
        )
        let resolver = SpeechCredentialResolver(client: client, keychain: keychain)
        // A reference box rather than a captured `var`: the Swift 5.10 toolchain
        // on the CI runner diagnoses a captured mutable variable inside `Task`
        // as an error, while 6.3 locally accepts it. Writing the result through
        // a final Sendable box satisfies both, so this check is not the thing
        // that decides whether the probe compiles.
        let outcome = ProbeOutcome()
        let stored = DispatchSemaphore(value: 0)
        Task {
            await outcome.record { try await resolver.store("probe-secret-value") }
            stored.signal()
        }
        _ = stored.wait(timeout: .now() + 10)

        check("写入密钥成功", outcome.errorDescription == nil, outcome.errorDescription ?? "无错误")
        check("密钥存进钥匙串并可取回",
              (try? keychain.secret(forKey: "SENSEAUDIO_API_KEY")) == "probe-secret-value",
              "InMemoryKeychainStore round-trip")

        let defaultsAfter = Set(defaults.dictionaryRepresentation().keys)
        let leaked = defaultsAfter.subtracting(defaultsBefore)
        check("保存密钥不会新增任何 UserDefaults 键",
              leaked.isEmpty,
              "\(leaked.sorted())")

        let field = keyField as? NSSecureTextField
        check("密钥输入框没有预填内容",
              (field?.stringValue ?? "").isEmpty,
              field?.stringValue ?? "")
    }

    private static func checkSplitView() {
        print("\n#1 分栏与窗口最小尺寸")
        let controller = MainViewController()
        controller.loadView()
        guard let split = controller.view as? NSSplitView else {
            check("分栏可解析", false, "根视图不是 NSSplitView")
            return
        }
        split.frame = NSRect(x: 0, y: 0, width: 1200, height: 720)
        split.layoutSubtreeIfNeeded()

        split.setPosition(0, ofDividerAt: 0)
        split.layoutSubtreeIfNeeded()
        let leftAtMin = split.subviews[0].frame.width

        split.setPosition(1200, ofDividerAt: 0)
        split.layoutSubtreeIfNeeded()
        let rightAtMax = split.subviews[1].frame.width

        check("左栏不可塌陷", leftAtMin >= MainViewController.minimumListWidth - 0.5,
              "拖到底 left=\(leftAtMin) 下限=\(MainViewController.minimumListWidth)")
        check("右栏不可塌陷", rightAtMax >= MainViewController.minimumDetailWidth - 0.5,
              "拖到底 right=\(rightAtMax) 下限=\(MainViewController.minimumDetailWidth)")
        check("窗口有最小尺寸",
              MainViewController.minimumContentSize.width > 0 && MainViewController.minimumContentSize.height > 0,
              "contentMinSize=\(MainViewController.minimumContentSize)")
    }

    private static func checkDetailLayout() {
        print("\n#5 内容列 measure cap + #14a 按钮行")
        for width in [CGFloat(200), 400, 779, 1600] {
            let detail = BookDetailView()
            hosted(detail, width: width, height: 700)

            let content = detail.subviews.first { $0.subviews.count == 2 }
            let stack = detail.subviews.compactMap { $0 as? NSStackView }.first
            guard let content, let stack else {
                check("布局可解析 @\(Int(width))pt", false, "找不到内容列或按钮行")
                continue
            }

            let frame = content.frame
            let leadGap = frame.minX
            let trailGap = width - frame.maxX
            check("内容列 @\(Int(width))pt",
                  frame.width <= 720.5 && leadGap >= 15.5 && trailGap >= 15.5,
                  "x=\(frame.minX) w=\(frame.width) 左\(leadGap) 右\(trailGap)")

            let buttons = stack.arrangedSubviews
            if buttons.count == 2 {
                check("按钮并排 @\(Int(width))pt",
                      buttons[0].frame.minX != buttons[1].frame.minX,
                      "b0.x=\(buttons[0].frame.minX) b1.x=\(buttons[1].frame.minX) stack=\(stack.frame.size)")
            }
        }
    }

    private static func checkAnnotationRowHeight() {
        print("\n#4 笔记行自适应高度")
        let cell = AnnotationCellView(frame: .zero)
        let long = String(
            repeating: "这是一段很长的书摘正文,用来验证行高会随内容增长而不是被固定在 64pt。",
            count: 6
        )
        cell.updateLayoutWidth(600)
        cell.configure(with: Annotation(
            id: "1", type: .highlight, chapterTitle: "第三章", locationInfo: "",
            contentText: long, noteText: "我的笔记",
            createdAt: Date(timeIntervalSinceReferenceDate: 0)
        ))
        hosted(cell, width: 600, height: cell.fittingSize.height)
        check("长文不被 64pt 截断", cell.fittingSize.height > 64,
              "fittingSize.height=\(cell.fittingSize.height)")
    }

    private static func checkSortAccessibility() {
        print("\n#6 排序可访问性")
        let defaults = UserDefaults.standard

        // 无损保存/恢复:bool(forKey:) 对缺失键返回 false,直接回写会
        // 凭空造出一个键,污染真实用户偏好。
        let savedColumn = defaults.object(forKey: BookListView.sortColumnKey)
        let savedAscending = defaults.object(forKey: BookListView.sortAscendingKey)
        defer {
            restore(savedColumn, forKey: BookListView.sortColumnKey)
            restore(savedAscending, forKey: BookListView.sortAscendingKey)
        }

        defaults.removeObject(forKey: BookListView.sortColumnKey)
        defaults.removeObject(forKey: BookListView.sortAscendingKey)

        guard let (list, table, bookColumn, countColumn) = makeList() else { return }

        check("初始无排序两列都是 unknown",
              direction(table, bookColumn) == "AXUnknownSortDirection"
                  && direction(table, countColumn) == "AXUnknownSortDirection",
              "book=\(direction(table, bookColumn)) count=\(direction(table, countColumn))")

        list.tableView(table, didClick: bookColumn)
        check("第一次点击 = 升序", direction(table, bookColumn) == "AXAscendingSortDirection",
              direction(table, bookColumn))
        check("未参与排序的列保持 unknown", direction(table, countColumn) == "AXUnknownSortDirection",
              direction(table, countColumn))

        list.tableView(table, didClick: bookColumn)
        check("第二次点击 = 降序", direction(table, bookColumn) == "AXDescendingSortDirection",
              direction(table, bookColumn))

        list.tableView(table, didClick: bookColumn)
        check("第三次点击 = 回到无排序(三态保留)",
              direction(table, bookColumn) == "AXUnknownSortDirection", direction(table, bookColumn))
        check("无排序时不留指示图标", table.indicatorImage(in: bookColumn) == nil,
              table.indicatorImage(in: bookColumn)?.accessibilityDescription ?? "nil")

        // 排序偏好持久化后,启动时必须把状态带出来,否则它只存在于数据里。
        defaults.set(BookColumn.count.rawValue, forKey: BookListView.sortColumnKey)
        defaults.set(false, forKey: BookListView.sortAscendingKey)
        guard let (_, restoredTable, _, restoredCount) = makeList() else { return }
        check("恢复保存的排序会显示指示图标", restoredTable.indicatorImage(in: restoredCount) != nil,
              restoredTable.indicatorImage(in: restoredCount)?.accessibilityDescription ?? "nil")
        check("恢复保存的排序方向正确",
              direction(restoredTable, restoredCount) == "AXDescendingSortDirection",
              direction(restoredTable, restoredCount))
    }

    private static func restore(_ value: Any?, forKey key: String) {
        if let value {
            UserDefaults.standard.set(value, forKey: key)
        } else {
            UserDefaults.standard.removeObject(forKey: key)
        }
    }

    private static func makeList() -> (BookListView, NSTableView, NSTableColumn, NSTableColumn)? {
        let list = BookListView()
        list.frame = NSRect(x: 0, y: 0, width: 420, height: 600)
        list.layoutSubtreeIfNeeded()
        guard let table = list.subviews.compactMap({ $0 as? NSScrollView }).first?.documentView as? NSTableView,
              let book = table.tableColumns.first(where: { $0.identifier.rawValue == BookColumn.book.rawValue }),
              let count = table.tableColumns.first(where: { $0.identifier.rawValue == BookColumn.count.rawValue }) else {
            check("书单表格可解析", false, "找不到表格或列")
            return nil
        }
        table.headerView?.tableView = table
        return (list, table, book, count)
    }

    /// 读 VoiceOver 真正消费的通道 —— 表头 proxy 上的 AXSortDirection。
    /// 直接读 headerCell.accessibilitySortDirection() 只是把刚写进去的值
    /// 再读一遍,测不到 AppKit 是否真的把它转发给了 AX 客户端。
    private static func direction(_ table: NSTableView, _ column: NSTableColumn) -> String {
        guard let index = table.tableColumns.firstIndex(of: column),
              let children = table.headerView?.accessibilityChildren(),
              index < children.count else { return "无 proxy" }

        let element = children[index] as AnyObject
        let selector = Selector(("accessibilityAttributeValue:"))
        guard element.responds(to: selector),
              let value = element.perform(selector, with: "AXSortDirection")?.takeUnretainedValue() else {
            return "无 AXSortDirection"
        }
        return "\(value)"
    }

    private static func checkAnnotationFilter() {
        print("\n笔记类型筛选")
        let book = Book(id: "b1", title: "测试书", author: "某人",
                        totalAnnotations: 5, highlightsCount: 3, notesCount: 2)
        let annotations =
            (0..<3).map { sample("h\($0)", .highlight) }
            + (0..<2).map { sample("n\($0)", .note) }

        // 纯函数层
        check("filter .all 不改变集合",
              AnnotationFilter.all.apply(to: annotations).count == 5,
              "\(AnnotationFilter.all.apply(to: annotations).count)")
        check("filter .highlight 只留高亮",
              AnnotationFilter.type(.highlight).apply(to: annotations).allSatisfy { $0.type == .highlight }
                  && AnnotationFilter.type(.highlight).apply(to: annotations).count == 3,
              "\(AnnotationFilter.type(.highlight).apply(to: annotations).count) 条")
        check("段顺序为 全部/高亮/笔记",
              AnnotationFilter.ordered == [.all, .type(.highlight), .type(.note)],
              "\(AnnotationFilter.ordered.map { $0.title(for: book) })")
        check("段标题带计数",
              AnnotationFilter.ordered.map { $0.title(for: book) } == ["全部 5", "高亮 3", "笔记 2"],
              "\(AnnotationFilter.ordered.map { $0.title(for: book) })")

        // UI 层
        let detail = BookDetailView()
        hosted(detail, width: 779, height: 700)
        guard let segmented = firstSegmentedControl(in: detail) else {
            check("详情页有分段筛选控件", false, "找不到 NSSegmentedControl")
            return
        }
        check("详情页有分段筛选控件", true, "\(segmented.segmentCount) 段")

        detail.show(book: book)
        detail.setAnnotations(annotations)
        check("段数 = 3", segmented.segmentCount == 3, "\(segmented.segmentCount)")
        check("默认选中「全部」", segmented.selectedSegment == 0, "selectedSegment=\(segmented.selectedSegment)")
        check("默认显示全部 5 条", detail.annotations.count == 5, "\(detail.annotations.count)")

        select(segment: 1, in: segmented)
        check("选「高亮」后只剩 3 条",
              detail.annotations.count == 3 && detail.annotations.allSatisfy { $0.type == .highlight },
              "\(detail.annotations.count) 条")

        select(segment: 2, in: segmented)
        check("选「笔记」后只剩 2 条",
              detail.annotations.count == 2 && detail.annotations.allSatisfy { $0.type == .note },
              "\(detail.annotations.count) 条")

        select(segment: 0, in: segmented)
        check("切回「全部」恢复 5 条", detail.annotations.count == 5, "\(detail.annotations.count)")

        // 换书必须重置筛选,否则新书会沿用上一本的筛选却没有任何提示
        select(segment: 1, in: segmented)
        detail.show(book: book)
        check("换书重置为「全部」", segmented.selectedSegment == 0, "selectedSegment=\(segmented.selectedSegment)")
    }

    private static func sample(_ id: String, _ type: AnnotationType) -> Annotation {
        Annotation(id: id, type: type, chapterTitle: "第一章", locationInfo: "",
                   contentText: "正文 \(id)", noteText: nil,
                   createdAt: Date(timeIntervalSinceReferenceDate: 0))
    }

    private static func firstSegmentedControl(in view: NSView) -> NSSegmentedControl? {
        if let control = view as? NSSegmentedControl { return control }
        for subview in view.subviews {
            if let found = firstSegmentedControl(in: subview) { return found }
        }
        return nil
    }

    private static func select(segment: Int, in control: NSSegmentedControl) {
        control.selectedSegment = segment
        if let action = control.action {
            NSApp.sendAction(action, to: control.target, from: control)
        }
    }

    private static func checkExportScope() {
        print("\n导出范围:全书 vs 当前筛选")
        let book = Book(id: "b1", title: "测试书", author: "某人",
                        totalAnnotations: 5, highlightsCount: 3, notesCount: 2)
        let annotations =
            (0..<3).map { sample("h\($0)", .highlight) }
            + (0..<2).map { sample("n\($0)", .note) }

        let detail = BookDetailView()
        hosted(detail, width: 779, height: 700)
        detail.show(book: book)
        detail.setAnnotations(annotations)

        guard let exportButton = view(named: "export", in: detail) as? NSButton,
              let exportMenu = view(named: "export-menu", in: detail) as? NSPopUpButton,
              let segmented = firstSegmentedControl(in: detail) else {
            check("导出控件可定位", false, "找不到 export / export-menu")
            return
        }

        // 未筛选:普通按钮,没有歧义,不需要下拉
        check("未筛选时显示普通按钮", !exportButton.isHidden && exportMenu.isHidden,
              "button.hidden=\(exportButton.isHidden) menu.hidden=\(exportMenu.isHidden)")
        check("未筛选时标题带条数", exportButton.title.contains("5"), "\"\(exportButton.title)\"")

        // 筛选后:换成下拉,两项分别写明范围与条数
        select(segment: 1, in: segmented)
        check("筛选后切换为下拉按钮", exportButton.isHidden && !exportMenu.isHidden,
              "button.hidden=\(exportButton.isHidden) menu.hidden=\(exportMenu.isHidden)")

        let items = Array(exportMenu.menu?.items.dropFirst() ?? [])
        check("下拉有两项", items.count == 2, "\(items.map(\.title))")
        check("第一项 = 当前筛选 3 条高亮",
              items.first.map { $0.title.contains("筛选") && $0.title.contains("3") } ?? false,
              "\"\(items.first?.title ?? "nil")\"")
        check("第二项 = 全书 5 条",
              items.last.map { $0.title.contains("全书") && $0.title.contains("5") } ?? false,
              "\"\(items.last?.title ?? "nil")\"")

        // 两项必须真的送出不同的集合
        var delivered: [Annotation]?
        detail.onExportRequested = { delivered = $0 }

        delivered = nil
        invoke(items[0])
        check("选「当前筛选」送出 3 条高亮",
              delivered?.count == 3 && (delivered?.allSatisfy { $0.type == .highlight } ?? false),
              "\(delivered?.count ?? -1) 条")

        delivered = nil
        invoke(items[1])
        check("选「全书」送出全部 5 条", delivered?.count == 5, "\(delivered?.count ?? -1) 条")

        // 未筛选时点普通按钮同样要送出全部
        select(segment: 0, in: segmented)
        delivered = nil
        invoke(exportButton)
        check("未筛选点按钮送出全部 5 条", delivered?.count == 5, "\(delivered?.count ?? -1) 条")
    }

    /// The speech entry and its panel.
    ///
    /// Two properties are asserted because they are the ones a user is harmed
    /// by: the note segment is disabled when there is no note, and the cost
    /// notice never states an amount. The contract has no currency, no unit
    /// price and no total, so any figure shown would be invented.
    private static func checkSpeechEntry() {
        print("\n生成语音入口与参数面板")
        let book = Book(id: "b1", title: "测试书", author: "某人",
                        totalAnnotations: 2, highlightsCount: 1, notesCount: 1)
        let highlightOnly = Annotation(
            id: "h0", type: .highlight, chapterTitle: "第一章", locationInfo: "",
            contentText: "只有高亮，没有笔记。",
            noteText: nil, createdAt: Date(timeIntervalSinceReferenceDate: 0)
        )
        let annotations = [highlightOnly, sample("n0", .note)]

        let detail = BookDetailView()
        hosted(detail, width: 779, height: 700)
        detail.show(book: book)
        detail.setAnnotations(annotations)

        guard let table = firstTableView(in: detail) else {
            check("标注表格可定位", false, "找不到 NSTableView")
            return
        }
        table.layoutSubtreeIfNeeded()

        guard let firstCell = table.view(atColumn: 0, row: 0, makeIfNecessary: true),
              let speechButton = view(named: "speech-entry", in: firstCell) as? NSButton else {
            check("标注行含生成语音入口", false, "找不到入口按钮")
            return
        }
        check("标注行含生成语音入口", true, "speech-entry")
        check("未选中行的语音入口隐藏", speechButton.isHidden, "\(speechButton.isHidden)")

        // Panel behaviour that does not need a provider: an annotation with no
        // note must not offer the note segment, and the notice must not invent
        // a price.
        let panel = makePanel(book: book, annotation: highlightOnly)
        panel.loadView()
        guard let kind = view(named: "speech.content-kind", in: panel.view) as? NSSegmentedControl,
              let cost = view(named: "speech.cost-notice", in: panel.view) as? NSTextField,
              let preview = view(named: "speech.content-preview", in: panel.view) as? NSTextField,
              let generate = view(named: "speech.generate", in: panel.view) as? NSButton else {
            check("语音面板控件齐备", false, "缺少 content-kind / cost-notice / preview / generate")
            return
        }
        check("语音面板控件齐备", true, "content-kind + cost + preview + generate")

        check("没有笔记时禁用笔记分段", !kind.isEnabled(forSegment: 1),
              "note segment enabled=\(kind.isEnabled(forSegment: 1))")
        check("高亮分段仍可用", kind.isEnabled(forSegment: 0), "highlight segment")

        // Without a selected voice the billable action stays disabled: the
        // catalog has not loaded in the probe, so nothing is resolved.
        check("未选定音色时不能生成", !generate.isEnabled, "\(generate.isEnabled)")
        check("预览显示将要发送的正文", preview.stringValue == highlightOnly.contentText,
              preview.stringValue)

        let notice = cost.stringValue
        check("费用提示说明会调用供应商", notice.contains("计费"), notice)
        check("费用提示不显示金额",
              !notice.contains("¥") && !notice.contains("元") && !notice.contains("$"),
              notice)
        check("费用提示指向供应商账单", notice.contains("供应商账单"), notice)

        // A second confirmation would contradict ADR 0007: the panel is the
        // confirmation, and the generate command is the authorisation.
        //
        // Matched by identifier *prefix* rather than by one exact name, so
        // adding `speech.confirm-dialog` is caught as readily as adding
        // `speech.confirm`. Naming a single identifier would leave the guard
        // green for every other name a second dialog could take.
        let actionIdentifiers = allViews(in: panel.view)
            .compactMap { $0.identifier?.rawValue }
            .filter { $0.hasPrefix("speech.") }
        let expected = Set(["speech.content-kind", "speech.content-preview",
                            "speech.characters", "speech.voice-group",
                            "speech.voice-variant", "speech.speed", "speech.volume",
                            "speech.pitch", "speech.cost-notice",
                            "speech.setup-result", "speech.generation-result",
                            "speech.playback-result", "speech.export-result",
                            "speech.export-target",
                            "speech.generate", "speech.play", "speech.regenerate",
                            "speech.recheck", "speech.close", "speech.export"])
        let unexpected = Set(actionIdentifiers).subtracting(expected)
        check("面板没有多余的动作控件（无第二次确认）", unexpected.isEmpty,
              "unexpected=\(unexpected.sorted())")
        check("面板动作控件齐全", expected.subtracting(Set(actionIdentifiers)).isEmpty,
              "missing=\(expected.subtracting(Set(actionIdentifiers)).sorted())")

        // The panel must be able to get out of its own way. A sheet presented
        // with `presentAsSheet` gets no title bar, so no close box and no menu
        // item reaches it; without an explicit control the window is a trap.
        let close = view(named: "speech.close", in: panel.view) as? NSButton
        let recheck = view(named: "speech.recheck", in: panel.view) as? NSButton
        check("面板有关闭入口", close != nil, "speech.close")
        check("面板有重新检查入口", recheck != nil, "speech.recheck")
        // Gating these on the same state as generate would disable the very
        // controls that repair a broken state, so they must survive a panel
        // that failed to load anything.
        check("关闭入口不被状态门控", close?.isEnabled == true,
              "enabled=\(close?.isEnabled.description ?? "nil")")
        check("重新检查不被状态门控", recheck?.isEnabled == true,
              "enabled=\(recheck?.isEnabled.description ?? "nil")")

        checkPanelFitsTheScreen(book: book)
        checkVoiceMenusAreFilled(book: book)
        checkLongMessageDoesNotWidenThePanel(book: book)
        checkExportFollowsTheClipRule(book: book)
        checkExportTargetGuidance(book: book)
        checkResultsDoNotOverwriteEachOther(book: book)
        checkGenerationSurvivesAPathFailure(book: book)
    }

    /// The four result areas must not eat each other's text.
    ///
    /// They were one `statusLabel`, so each action overwrote the last: pressing
    /// 播放 replaced "已生成（clip …）" with "正在播放。", and an export replaced
    /// both. The user could not see whether the panel had generated anything,
    /// was playing, or had written a file -- only what happened most recently.
    ///
    /// The assertion is therefore about **simultaneity**, not about any one
    /// label's text: after all three actions, every result has to still be on
    /// screen. Asserting each label separately in isolation would pass against
    /// the old single label too, one at a time -- which is exactly why the
    /// defect survived the assertions that were already there.
    private static func checkResultsDoNotOverwriteEachOther(book: Book) {
        print("\n生成 / 播放 / 导出结果互不覆盖")

        let scratch = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("books-exporter-verify-areas-\(UUID().uuidString)")
        let bookRoot = scratch.appendingPathComponent("100 Go Mistakes and How to Avoid Them")
        try? FileManager.default.createDirectory(at: bookRoot, withIntermediateDirectories: true)
        try? Data("# 100 Go Mistakes\n".utf8).write(
            to: bookRoot.appendingPathComponent("100 Go Mistakes.md")
        )
        defer { try? FileManager.default.removeItem(at: scratch) }

        let suiteName = "books-exporter-verify-areas-roots"
        UserDefaults.standard.removePersistentDomain(forName: suiteName)
        defer { UserDefaults.standard.removePersistentDomain(forName: suiteName) }
        guard let defaults = UserDefaults(suiteName: suiteName) else {
            check("结果分区：可用的 UserDefaults suite", false, "无法创建 suite")
            return
        }
        let store = BookExportRootStore(defaults: defaults)
        store.record(assetID: book.id, exportRoot: bookRoot)

        let fixtures = SpeechPanelFixtures()
        final class Calls: @unchecked Sendable {
            private let lock = NSLock()
            private var recorded: [[String]] = []
            var all: [[String]] {
                lock.lock(); defer { lock.unlock() }
                return recorded
            }
            func record(_ arguments: [String]) {
                lock.lock(); recorded.append(arguments); lock.unlock()
            }
        }
        let calls = Calls()
        let panel = makePanel(
            book: book,
            annotation: Annotation(
                id: "h-areas", type: .highlight, chapterTitle: "第一章", locationInfo: "",
                contentText: "结果分区检查用的正文。", noteText: nil,
                createdAt: Date(timeIntervalSinceReferenceDate: 0)
            ),
            runner: { _, arguments, _ in
                calls.record(arguments)
                return fixtures.reply(to: arguments)
            },
            hasCredential: true,
            exportRoots: store
        )
        _ = panel.view

        func text(_ name: String) -> String {
            (view(named: name, in: panel.view) as? NSTextField)?.stringValue ?? ""
        }

        guard let group = view(named: "speech.voice-group", in: panel.view) as? NSPopUpButton,
              let generate = view(named: "speech.generate", in: panel.view) as? NSButton,
              let play = view(named: "speech.play", in: panel.view) as? NSButton,
              let export = view(named: "speech.export", in: panel.view) as? NSButton else {
            check("结果分区：控件齐全", false, "缺少必要控件")
            return
        }
        settle(group, until: { group.numberOfItems > 0 })
        guard generate.isEnabled else {
            check("结果分区：已选定音色后可生成（前提）", false, "generate 仍禁用")
            return
        }

        // 1. Generate.
        generate.performClick(nil)
        settle(panel.view, until: {
            text("speech.generation-result").contains("已生成")
        })
        check("生成结果落在生成区", text("speech.generation-result").contains("已生成"),
              text("speech.generation-result"))
        settle(panel.view, until: { play.isEnabled })

        // 2. Play. `SpeechAudioPlayer` cannot play the fixture's path, so the
        //    playback area reports a failure -- which is the stronger case: the
        //    generation result must survive a *failed* playback message, which
        //    is the one that used to hide it.
        guard play.isEnabled else {
            check("结果分区：生成后播放可用（前提）", false, "播放按钮仍禁用")
            return
        }
        play.performClick(nil)
        settle(panel.view, until: { text("speech.playback-result").contains("播放") })
        check("播放结果落在播放区", text("speech.playback-result").contains("播放"),
              text("speech.playback-result"))
        check("播放不覆盖生成结果",
              text("speech.generation-result").contains("已生成"),
              "生成区=\(text("speech.generation-result"))")

        // 3. Export.
        guard export.isEnabled else {
            check("结果分区：生成后导出可用（前提）", false, "导出按钮仍禁用")
            return
        }
        export.performClick(nil)
        settle(panel.view, until: {
            calls.all.contains { $0.contains("export") && $0.contains(bookRoot.path) }
        })
        check("导出结果落在导出区", text("speech.export-result").contains("导出"),
              text("speech.export-result"))
        check("导出不覆盖生成结果",
              text("speech.generation-result").contains("已生成"),
              "生成区=\(text("speech.generation-result"))")
        check("导出不覆盖播放结果",
              text("speech.playback-result").contains("播放"),
              "播放区=\(text("speech.playback-result"))")

        // 4. A new generation does clear the other two -- they described the
        //    previous clip, so that is staleness rather than overwriting. If
        //    this ever goes green, the clearing was dropped and the areas
        //    accumulate claims about clips that no longer exist.
        generate.performClick(nil)
        settle(panel.view, until: {
            text("speech.generation-result").contains("正在生成")
                || text("speech.generation-result").contains("已生成")
        })
        check("重新生成清掉上一条音频的播放与导出结果",
              text("speech.playback-result").isEmpty && text("speech.export-result").isEmpty,
              "播放区=\(text("speech.playback-result")) 导出区=\(text("speech.export-result"))")
    }

    /// When the path cannot be resolved, "已生成" has to stay on screen.
    ///
    /// This is the specific form the overwrite took: the panel generated
    /// successfully, then resolved the playback path and reported the failure
    /// into the same label -- so the one thing the user had just paid for
    /// disappeared exactly when something went wrong. The first run of this
    /// check drove playback through the 播放 button, where `SpeechAudioPlayer`
    /// always throws for a fixture path; that covers `play()`'s catch branch
    /// but not `resolvePlaybackPath`'s, and the mutation of the latter came
    /// back green. So this case fails the `speech play` *command* instead.
    private static func checkGenerationSurvivesAPathFailure(book: Book) {
        print("\n路径解析失败不抹掉生成结果")

        let fixtures = SpeechPanelFixtures()
        let playFails = """
        {"schema_version":1,"error":{"code":"SPEECH_CLIP_NOT_FOUND",\
        "message":"the Cached Speech Clip is gone","remediation":"generate it again"}}
        """
        let panel = makePanel(
            book: book,
            annotation: Annotation(
                id: "h-path-fail", type: .highlight, chapterTitle: "第一章", locationInfo: "",
                contentText: "路径失败检查用的正文。", noteText: nil,
                createdAt: Date(timeIntervalSinceReferenceDate: 0)
            ),
            runner: { _, arguments, _ in
                if arguments.contains("play") {
                    return RustCLICommandResult(
                        stderr: Data(playFails.utf8), terminationStatus: 1
                    )
                }
                return fixtures.reply(to: arguments)
            },
            hasCredential: true
        )
        _ = panel.view

        func text(_ name: String) -> String {
            (view(named: name, in: panel.view) as? NSTextField)?.stringValue ?? ""
        }
        guard let group = view(named: "speech.voice-group", in: panel.view) as? NSPopUpButton,
              let generate = view(named: "speech.generate", in: panel.view) as? NSButton else {
            check("路径失败：控件齐全", false, "缺少控件")
            return
        }
        settle(group, until: { group.numberOfItems > 0 })
        guard generate.isEnabled else {
            check("路径失败：已选定音色后可生成（前提）", false, "generate 仍禁用")
            return
        }
        generate.performClick(nil)
        settle(panel.view, until: {
            text("speech.playback-result").contains("无法播放")
        })

        check("路径解析失败落在播放区",
              text("speech.playback-result").contains("无法播放"),
              text("speech.playback-result"))
        check("路径解析失败仍保留生成结果",
              text("speech.generation-result").contains("已生成"),
              "生成区=\(text("speech.generation-result"))")
    }

    /// The export target has to be the *book's* directory, and the panel has to
    /// say so when there is not one.
    ///
    /// This is the fix for a defect no assertion covered: `speech export` takes
    /// any writable directory, creates `assets/audio/` there, and reports
    /// success. Choosing `~/Downloads` produced a manifest and an mp3 that no
    /// exported note will ever link to, because `resolve_export_links` only
    /// looks in `book_dir` -- and the panel's only guidance was the string
    /// "choose a folder", which cannot express which folder.
    ///
    /// So the assertions are about the *decision*, not the wording. The panel
    /// must (a) name a recorded directory, (b) treat a recorded-but-missing
    /// directory as no directory, and (c) hand the recorded path to the CLI
    /// itself rather than opening a picker first. (c) is the load-bearing one:
    /// it is the difference between "exports to the right place" and "offers to
    /// choose", and it is observable in the arguments the runner receives.
    private static func checkExportTargetGuidance(book: Book) {
        print("\n导出目标引导")

        let scratch = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("books-exporter-verify-export-\(UUID().uuidString)")
        let bookRoot = scratch.appendingPathComponent("100 Go Mistakes and How to Avoid Them")
        let emptyDir = scratch.appendingPathComponent("empty")
        let goneDir = scratch.appendingPathComponent("gone")
        for directory in [bookRoot, emptyDir] {
            try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        }
        try? Data("# 100 Go Mistakes\n".utf8).write(
            to: bookRoot.appendingPathComponent("100 Go Mistakes.md")
        )
        defer { try? FileManager.default.removeItem(at: scratch) }

        let suiteName = "books-exporter-verify-export-roots"
        UserDefaults.standard.removePersistentDomain(forName: suiteName)
        defer { UserDefaults.standard.removePersistentDomain(forName: suiteName) }
        guard let defaults = UserDefaults(suiteName: suiteName) else {
            check("导出目标引导：可用的 UserDefaults suite", false, "无法创建 suite")
            return
        }
        let store = BookExportRootStore(defaults: defaults)

        let fixtures = SpeechPanelFixtures()
        let brokenFixtures = SpeechPanelFixtures.validate()
        check("语音面板的 canned 回执都能解成真实响应类型", brokenFixtures.isEmpty,
              brokenFixtures.joined(separator: " | "))
        let annotation = Annotation(
            id: "h-export-target", type: .highlight, chapterTitle: "第一章", locationInfo: "",
            contentText: "导出目标检查用的正文。", noteText: nil,
            createdAt: Date(timeIntervalSinceReferenceDate: 0)
        )

        // A recorder rather than a captured `var`: the runner is `@Sendable` and
        // runs off the main thread, so writing into a captured variable is an
        // error under the Swift 6 language mode even though the probe is pinned
        // to 5. This is the same shape `ProbeOutcome` uses.
        final class CallRecorder: @unchecked Sendable {
            private let lock = NSLock()
            private var recorded: [[String]] = []

            var calls: [[String]] {
                lock.lock()
                defer { lock.unlock() }
                return recorded
            }

            func reset() {
                lock.lock()
                recorded = []
                lock.unlock()
            }

            func record(_ arguments: [String]) {
                lock.lock()
                recorded.append(arguments)
                lock.unlock()
            }
        }

        func panelCapturing(_ recorder: CallRecorder) -> SpeechPanelViewController {
            makePanel(
                book: book,
                annotation: annotation,
                runner: { _, arguments, _ in
                    recorder.record(arguments)
                    return fixtures.reply(to: arguments)
                },
                hasCredential: true,
                exportRoots: store
            )
        }

        // 1. No record yet: the panel must say there is no export root rather
        //    than offer a location, and must not export anywhere.
        do {
            let recorder = CallRecorder()
            let panel = panelCapturing(recorder)
            _ = panel.view
            let label = (view(named: "speech.export-target", in: panel.view) as? NSTextField)?
                .stringValue ?? ""

            check("无记录时说明还不知道导出目录", label.contains("还不知道这本书的导出目录"), label)
            check("无记录时不给出任何路径", !label.contains("/"), label)
            if case .known = panel.exportTarget {
                check("无记录时导出目标为缺失", false, "exportTarget 报为 known")
            } else {
                check("无记录时导出目标为缺失", true, "")
            }
            check("无记录时不自行发起导出",
                  !recorder.calls.contains { $0.contains("export") },
                  "calls=\(recorder.calls)")
        }

        // 2. Recorded and still a directory: the panel names it, and pressing
        //    export writes there through the CLI with no picker in between.
        do {
            store.record(assetID: book.id, exportRoot: bookRoot)
            let recorder = CallRecorder()
            let panel = panelCapturing(recorder)
            _ = panel.view

            let label = (view(named: "speech.export-target", in: panel.view) as? NSTextField)?
                .stringValue ?? ""
            check("有记录时显示该书导出目录", label.contains(bookRoot.path), label)

            guard case .known(let resolved) = panel.exportTarget else {
                check("有记录时导出目标为已知目录", false, "exportTarget 报为 missing")
                return
            }
            check("有记录时导出目标为已知目录", resolved.path == bookRoot.path,
                  "resolved=\(resolved.path)")

            // Generate so there is a clip, then press the real export control.
            guard let group = view(named: "speech.voice-group", in: panel.view) as? NSPopUpButton,
                  let generate = view(named: "speech.generate", in: panel.view) as? NSButton,
                  let export = view(named: "speech.export", in: panel.view) as? NSButton else {
                check("生成并导出（导出目标检查的前提）", false, "缺少控件")
                return
            }
            settle(group, until: { group.numberOfItems > 0 })
            guard generate.isEnabled else {
                check("已选定音色后可生成（导出目标检查的前提）", false, "generate 仍禁用")
                return
            }
            generate.performClick(nil)
            settle(export, until: { export.isEnabled })

            // `settle` gives up silently when its condition never holds, and a
            // disabled export button turns `performClick` into a no-op -- which
            // would make the assertion below fail for the wrong reason, or pass
            // for one. So the precondition is asserted, not assumed.
            check("生成后导出可用（导出目标检查的前提）", export.isEnabled,
                  "enabled=\(export.isEnabled)")
            let generatedStatus = (view(named: "speech.generation-result", in: panel.view) as? NSTextField)?
                .stringValue ?? ""
            check("生成已成功（导出目标检查的前提）", generatedStatus.contains("已生成"),
                  "status=\(generatedStatus) calls=\(recorder.calls)")

            recorder.reset()
            export.performClick(nil)
            settle(panel.view, until: {
                recorder.calls.contains { $0.contains("export") }
            })

            let exportCall = recorder.calls.first { $0.contains("export") }
            check("导出直接交给 CLI，不弹目录选择", exportCall != nil,
                  "calls=\(recorder.calls)")
            if let exportCall {
                // The recorded directory has to be the one that reaches
                // `--output`. This is the assertion that would fail if the
                // panel passed the parent, or an arbitrary picker result.
                check("导出使用该书的导出目录", exportCall.contains(bookRoot.path),
                      "arguments=\(exportCall)")
            }
        }

        // 3. Recorded but the directory is gone: offering a dead path would
        //    send the export somewhere the book's notes are not.
        do {
            store.record(assetID: book.id, exportRoot: goneDir)
            let panel = panelCapturing(CallRecorder())
            _ = panel.view
            let label = (view(named: "speech.export-target", in: panel.view) as? NSTextField)?
                .stringValue ?? ""
            if case .known = panel.exportTarget {
                check("目录已不存在时不作为导出目标", false, "exportTarget 报为 known")
            } else {
                check("目录已不存在时不作为导出目标", true, "")
            }
            check("目录已不存在时回到「还不知道导出目录」",
                  label.contains("还不知道这本书的导出目录"), label)
        }

        // 4. The pre-write check itself, against real directories.
        do {
            store.record(assetID: book.id, exportRoot: bookRoot)
            let panel = panelCapturing(CallRecorder())
            _ = panel.view
            check("含 Markdown 的目录通过导出前检查",
                  panel.containsExportedMarkdown(bookRoot), bookRoot.path)
            check("空目录不通过导出前检查",
                  !panel.containsExportedMarkdown(emptyDir), emptyDir.path)
            check("不存在的目录不通过导出前检查",
                  !panel.containsExportedMarkdown(goneDir), goneDir.path)
            check("文件不是导出目录",
                  !panel.containsExportedMarkdown(
                    bookRoot.appendingPathComponent("100 Go Mistakes.md")),
                  "a .md file, not a directory")
        }

        // 5. A directory the user picked becomes the one offered next time.
        //
        //    Driven through `performExport` because `NSOpenPanel` cannot be
        //    driven headlessly -- without this the "remember the choice" line is
        //    the one behaviour in this change that no check can reach, and a
        //    mutation that deletes it is invisible. The directory holds a `.md`
        //    so the pre-write confirmation is not what is under test here.
        do {
            let chosen = scratch.appendingPathComponent("chosen")
            try? FileManager.default.createDirectory(
                at: chosen, withIntermediateDirectories: true)
            try? Data("# chosen\n".utf8).write(to: chosen.appendingPathComponent("book.md"))
            store.record(assetID: book.id, exportRoot: bookRoot)

            let recorder = CallRecorder()
            let panel = panelCapturing(recorder)
            _ = panel.view
            guard let generate = view(named: "speech.generate", in: panel.view) as? NSButton,
                  let group = view(named: "speech.voice-group", in: panel.view) as? NSPopUpButton
            else {
                check("手动选目录后可导出（记忆检查的前提）", false, "缺少控件")
                return
            }
            settle(group, until: { group.numberOfItems > 0 })
            guard generate.isEnabled else {
                check("已选定音色后可生成（记忆检查的前提）", false, "generate 仍禁用")
                return
            }
            generate.performClick(nil)
            settle(panel.view, until: {
                (view(named: "speech.export", in: panel.view) as? NSButton)?.isEnabled == true
            })

            recorder.reset()
            panel.performExport(clipID: SpeechPanelFixtures.clipID, to: chosen)
            settle(panel.view, until: {
                recorder.calls.contains {
                    $0.contains("export") && $0.contains(chosen.path)
                }
            })
            check("手动选的目录确实被导出",
                  recorder.calls.contains { $0.contains(chosen.path) },
                  "calls=\(recorder.calls)")
            settle(panel.view, until: {
                store.exportRoot(forAssetID: book.id)?.path == chosen.path
            })
            check("手动选的目录成为之后的导出目标",
                  store.exportRoot(forAssetID: book.id)?.path == chosen.path,
                  "stored=\(store.exportRoot(forAssetID: book.id)?.path ?? "nil") "
                      + "expected=\(chosen.path)")
        }
    }

    /// The export control appears only once there is something to export, and
    /// it is wired to the CLI rather than to a local file copy.
    ///
    /// The rule being tested is the ordinary one -- no clip, nothing to export
    /// -- so unlike recheck and close this button *is* gated. The assertions
    /// therefore go both ways: disabled before a generation, enabled after one,
    /// driven through the real action rather than by poking the flag.
    private static func checkExportFollowsTheClipRule(book: Book) {
        print("\n导出音频按钮的可用性")
        let profileJSON = """
        {"schema_version":1,"receipt":{"operation":"profile_show","profile":\
        {"provider":"senseaudio","model":"sensenova-tts-2.0","voice_id":"male_0004_a",\
        "emotion_label":null,"style_label":null,"speed":1.0,"volume":1.0,"pitch":0,\
        "verification_status":"unverified","verified_at":null,\
        "audio":{"format":"mp3","sample_rate":32000,"bitrate":128000,"channel":2}},\
        "api_key_env":"SENSEAUDIO_API_KEY","config_path":null,"warnings":[]}}
        """
        let catalogJSON = """
        {"schema_version":1,"receipt":{"operation":"voices","provider":"senseaudio",\
        "fetched_at":"2026-09-30T09:38:03Z","stale":false,"warnings":[],"voices":[\
        {"provider":"senseaudio","source_type":"system","voice_id":"female_0006_a",\
        "voice_name":"温柔御姐","emotion_label":null,"style_label":null,\
        "description":[],"created_time":"2025-09-26"}]}}
        """
        let clipID = "14004204099a0116e9e43ca3d02ed7c5e035373e646ae5dc9feb7a23aaee9742"
        let generateJSON = """
        {"schema_version":1,"receipt":{"operation":"generate","clip_id":"\(clipID)",\
        "attempt_id":"attempt-probe","source":"provider","provider_called":true,\
        "asset_id":"b1","annotation_id":"h-export","content_kind":"highlight",\
        "text_sha256":"01805727314e4def395b368a7f71768e6129a55de56a68a5d7de97736145e9c9",\
        "unicode_characters":10,"estimated_billing_characters":20,\
        "billing_estimator_version":"senseaudio-docs-2026-09-10",\
        "audio":{"format":"mp3","sample_rate":32000,"bitrate":128000,"channel":2,\
        "duration_ms":11412,"size_bytes":182272},\
        "provider":{"trace_id":"probe","usage_characters":10},"warnings":[]}}
        """
        let panel = makePanel(
            book: book,
            annotation: Annotation(
                id: "h-export", type: .highlight, chapterTitle: "第一章", locationInfo: "",
                contentText: "导出检查用的正文。", noteText: nil,
                createdAt: Date(timeIntervalSinceReferenceDate: 0)
            ),
            runner: { _, arguments, _ in
                if arguments.contains("voices") { return .success(catalogJSON) }
                if arguments.contains("generate") { return .success(generateJSON) }
                return .success(profileJSON)
            },
            hasCredential: true
        )

        _ = panel.view
        guard let group = view(named: "speech.voice-group", in: panel.view) as? NSPopUpButton,
              let export = view(named: "speech.export", in: panel.view) as? NSButton,
              let generate = view(named: "speech.generate", in: panel.view) as? NSButton else {
            check("导出按钮存在", false, "缺少 speech.export / speech.generate / 音色下拉")
            return
        }
        settle(group, until: { group.numberOfItems > 0 })

        // Nothing generated yet, so there is nothing to export.
        check("未生成时导出按钮禁用", !export.isEnabled, "enabled=\(export.isEnabled)")

        guard generate.isEnabled else {
            check("已选定音色后可生成（导出检查的前提）", false,
                  "generate 仍禁用，无法验证生成后状态")
            return
        }
        generate.performClick(nil)
        settle(export, until: { export.isEnabled })

        check("生成后可导出", export.isEnabled, "enabled=\(export.isEnabled)")
        let status = (view(named: "speech.generation-result", in: panel.view) as? NSTextField)?
            .stringValue ?? ""
        check("生成后生成结果显示已生成", status.contains("已生成"), status)
    }

    /// Turn the run loop until `condition` holds or the budget runs out. The
    /// panel's load and generate are `Task`s, so the loop is what lets them
    /// progress; without it every assertion here would read the pre-load state.
    private static func settle(_ view: NSView, until condition: () -> Bool) {
        let deadline = Date().addingTimeInterval(5)
        while !condition() && Date() < deadline {
            RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.02))
        }
        view.layoutSubtreeIfNeeded()
    }

    /// A long status message must not widen the panel.
    ///
    /// This is the second overflow, and `preferredContentSize` cannot reach
    /// it: a sheet is sized from its content, and an unlimited-line wrapping
    /// label reports its full single-line width as its intrinsic width. A
    /// provider error carrying a file path opened a panel 1970pt wide with both
    /// edges off a 1512pt display while the declared height was still intact --
    /// the size declaration set the initial size and nothing stopped the
    /// window growing past it.
    ///
    /// So the assertion is on the *fitting* width, which is what the sheet
    /// actually consults, and the message is the shape that really occurred.
    private static func checkLongMessageDoesNotWidenThePanel(book: Book) {
        print("\n超长状态文案不撑宽面板")
        let profileJSON = """
        {"schema_version":1,"receipt":{"operation":"profile_show","profile":\
        {"provider":"senseaudio","model":"sensenova-tts-2.0","voice_id":"male_0004_a",\
        "emotion_label":null,"style_label":null,"speed":1.0,"volume":1.0,"pitch":0,\
        "verification_status":"unverified","verified_at":null,\
        "audio":{"format":"mp3","sample_rate":32000,"bitrate":128000,"channel":2}},\
        "api_key_env":"SENSEAUDIO_API_KEY","config_path":null,"warnings":[]}}
        """
        // The shape that really happened: an exported clip located by path,
        // then refused because its manifest belongs to another asset. Long,
        // one-token-ish runs, no spaces -- the worst case for intrinsic width.
        let longMessage = "the Exported Speech Clip in /Users/someone/books-exported/"
            + "100 Go Mistakes and How to Avoid Them (found via export_locator) was not used: "
            + "the manifest belongs to asset_id 706DB5A46682C0CA482434189BBACE24 rather than "
            + "0C61EE0000000000000000000000000000000000"
        let errorJSON = """
        {"schema_version":1,"error":{"code":"SPEECH_EXPORT_MISMATCH",\
        "message":"\(longMessage)","remediation":"\(longMessage)"}}
        """
        let panel = makePanel(
            book: book,
            annotation: Annotation(
                id: "h-long-status", type: .highlight, chapterTitle: "第一章", locationInfo: "",
                contentText: "超长文案检查用的正文。", noteText: nil,
                createdAt: Date(timeIntervalSinceReferenceDate: 0)
            ),
            runner: { _, arguments, _ in
                if arguments.contains("voices") {
                    return RustCLICommandResult(
                        stderr: Data(errorJSON.utf8), terminationStatus: 1
                    )
                }
                return .success(profileJSON)
            },
            hasCredential: true
        )

        _ = panel.view
        let status = view(named: "speech.setup-result", in: panel.view) as? NSTextField
        guard let status else {
            check("超长文案检查：找得到准备标签", false, "缺少 speech.setup-result")
            return
        }
        let deadline = Date().addingTimeInterval(5)
        while status.stringValue.isEmpty && Date() < deadline {
            RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.02))
        }
        check("超长文案已进入准备标签", status.stringValue.contains("manifest belongs to"),
              "len=\(status.stringValue.count)")

        // What the sheet consults. Unbounded here is the shipped defect.
        let fitting = panel.view.fittingSize
        let declared = panel.preferredContentSize
        check("超长文案不撑宽面板",
              fitting.width <= declared.width + 1,
              "fitting=\(fitting) declared=\(declared)")

        // And the positive evidence that it wrapped rather than stretched:
        // laid out at the declared width, the label is narrower than the
        // message and taller than a single line.
        panel.view.frame = NSRect(origin: .zero, size: declared)
        panel.view.layoutSubtreeIfNeeded()
        check("超长文案在面板宽度内换行",
              status.frame.height > 20,
              "label=\(status.frame) message chars=\(status.stringValue.count)")
    }

    /// The panel is a sheet, so its size comes from the view controller. Sized
    /// from content instead, it grows with the text: every wrapping label
    /// reports its full single-line width as its intrinsic width, so one long
    /// highlight opened a sheet wider than the display, with the voice pickers
    /// and the generate button off screen.
    ///
    /// The fixture above is nine characters and can never reproduce that, which
    /// is why this check builds its own annotation from text long enough to
    /// matter rather than reusing the one the rest of the probe runs on.
    private static func checkPanelFitsTheScreen(book: Book) {
        print("\n语音面板尺寸有界")
        let longText = String(
            repeating: "幻觉是指模型输出的数据看似准确，但实际上不正确或不以训练模型的输入数据为基础的情况。",
            count: 6
        )
        let panel = makePanel(book: book, annotation: Annotation(
            id: "h-long", type: .highlight, chapterTitle: "第一章", locationInfo: "",
            contentText: longText, noteText: nil,
            createdAt: Date(timeIntervalSinceReferenceDate: 0)
        ))
        panel.loadView()

        let size = panel.preferredContentSize
        check("面板声明了尺寸而非由内容撑开", size.width > 0 && size.height > 0,
              "preferredContentSize=\(size)")

        // The bound is the display, not a number someone likes.
        guard let screen = NSScreen.main else {
            check("面板不超出屏幕", false, "没有可测量的屏幕，探针无法验证该主张")
            return
        }
        let visible = screen.visibleFrame
        check("面板不超出屏幕",
              size.width <= visible.width && size.height <= visible.height,
              "panel=\(size) screen=\(visible.size)")

        // ... and wide enough for the widest row, so "fix it" cannot mean
        // "shrink it until the pickers fall off the other side".
        panel.view.frame = NSRect(origin: .zero, size: size)
        panel.view.layoutSubtreeIfNeeded()
        let bounds = panel.view.bounds
        func inside(_ view: NSView) -> Bool {
            let frame = view.convert(view.bounds, to: panel.view)
            return frame.minX >= -0.5 && frame.minY >= -0.5
                && frame.maxX <= bounds.maxX + 0.5 && frame.maxY <= bounds.maxY + 0.5
                && frame.width > 0 && frame.height > 0
        }
        for name in ["speech.voice-group", "speech.voice-variant",
                     "speech.generate", "speech.close", "speech.recheck"] {
            guard let control = view(named: name, in: panel.view) else {
                check("控件在面板内：\(name)", false, "找不到控件")
                continue
            }
            let frame = control.convert(control.bounds, to: panel.view)
            check("控件在面板内：\(name)", inside(control),
                  "frame=\(frame) panel=\(bounds.size)")
        }
    }

    private static func makePanel(
        book: Book,
        annotation: Annotation,
        runner: RustCLIClient.Runner? = nil,
        hasCredential: Bool = false,
        exportRoots: BookExportRootStore = BookExportRootStore(
            defaults: UserDefaults(suiteName: "books-exporter-verify-default-export-roots")!
        )
    ) -> SpeechPanelViewController {
        let client = RustCLIClient(
            executableURL: URL(fileURLWithPath: "/tmp/apple-books-exporter"),
            runner: runner ?? { _, _, _ in RustCLICommandResult(terminationStatus: 1) }
        )
        return SpeechPanelViewController(
            book: book,
            annotation: annotation,
            speech: SpeechService(client: client),
            player: SpeechAudioPlayer(),
            hasCredential: { hasCredential },
            exportRoots: exportRoots
        )
    }

    /// Canned machine JSON for the four speech calls a panel makes, so one
    /// runner can drive a whole panel lifecycle.
    ///
    /// Shared because the export-target check needs a *generated clip* before it
    /// can press the export control, and hand-copying the generate receipt into a
    /// second check is how the two copies drift.
    private struct SpeechPanelFixtures {
        static let clipID =
            "14004204099a0116e9e43ca3d02ed7c5e035373e646ae5dc9feb7a23aaee9742"

        private let profileJSON = """
        {"schema_version":1,"receipt":{"operation":"profile_show","profile":\
        {"provider":"senseaudio","model":"sensenova-tts-2.0","voice_id":"male_0004_a",\
        "emotion_label":null,"style_label":null,"speed":1.0,"volume":1.0,"pitch":0,\
        "verification_status":"unverified","verified_at":null,\
        "audio":{"format":"mp3","sample_rate":32000,"bitrate":128000,"channel":2}},\
        "api_key_env":"SENSEAUDIO_API_KEY","config_path":null,"warnings":[]}}
        """
        private let catalogJSON = """
        {"schema_version":1,"receipt":{"operation":"voices","provider":"senseaudio",\
        "fetched_at":"2026-09-30T09:38:03Z","stale":false,"warnings":[],"voices":[\
        {"provider":"senseaudio","source_type":"system","voice_id":"female_0006_a",\
        "voice_name":"温柔御姐","emotion_label":null,"style_label":null,\
        "description":[],"created_time":"2025-09-26"}]}}
        """
        private func generateJSON(annotationID: String) -> String {
            """
            {"schema_version":1,"receipt":{"operation":"generate","clip_id":"\(Self.clipID)",\
            "attempt_id":"attempt-probe","source":"provider","provider_called":true,\
            "asset_id":"b1","annotation_id":"\(annotationID)","content_kind":"highlight",\
            "text_sha256":"01805727314e4def395b368a7f71768e6129a55de56a68a5d7de97736145e9c9",\
            "unicode_characters":10,"estimated_billing_characters":20,\
            "billing_estimator_version":"senseaudio-docs-2026-09-10",\
            "audio":{"format":"mp3","sample_rate":32000,"bitrate":128000,"channel":2,\
            "duration_ms":11412,"size_bytes":182272},\
            "provider":{"trace_id":"probe","usage_characters":10},"warnings":[]}}
            """
        }
        private func exportJSON(annotationID: String) -> String {
            """
            {"schema_version":1,"receipt":{"operation":"export","clip_id":"\(Self.clipID)",\
            "asset_id":"b1","annotation_id":"\(annotationID)","content_kind":"highlight",\
            "relative_path":"assets/audio/highlight-\(Self.clipID.prefix(12)).mp3",\
            "path":"/tmp/books/assets/audio/highlight-\(Self.clipID.prefix(12)).mp3",\
            "sha256":"01805727314e4def395b368a7f71768e6129a55de56a68a5d7de97736145e9c9",\
            "size_bytes":182272,"format":"mp3","exported_at":"2026-10-01T00:00:00Z",\
            "reused":false,"replaced":false,"provider_called":false,"warnings":[]}}
            """
        }

        func reply(to arguments: [String]) -> RustCLICommandResult {
            let annotationID = value(after: "--annotation-id", in: arguments) ?? "h-probe"
            if arguments.contains("voices") { return .success(catalogJSON) }
            if arguments.contains("generate") { return .success(generateJSON(annotationID: annotationID)) }
            if arguments.contains("export") { return .success(exportJSON(annotationID: annotationID)) }
            if arguments.contains("play") { return .success(playJSON(annotationID: annotationID)) }
            return .success(profileJSON)
        }

        private func playJSON(annotationID: String) -> String {
            """
            {"schema_version":1,"receipt":{"operation":"play","clip_id":"\(Self.clipID)",\
            "source":"cache","played":false,"provider_called":false,"asset_id":"b1",\
            "annotation_id":"\(annotationID)","content_kind":"highlight",\
            "path":"/tmp/clips/\(Self.clipID).mp3",\
            "audio":{"format":"mp3","sample_rate":32000,"bitrate":128000,"channel":2,\
            "duration_ms":11412,"size_bytes":182272},"export_origin":null,"warnings":[]}}
            """
        }

        /// Decodes every canned receipt into the type the client will decode it
        /// into, and returns the ones that do not survive.
        ///
        /// A malformed fixture is invisible from the outside: it makes the panel
        /// take its *error* path, so an assertion fails for a reason that has
        /// nothing to do with what it is checking, and a neighbouring check can
        /// stay green on a path nobody intended. A hand-typed extra `}` in the
        /// generate receipt did exactly that; it was only found because the
        /// export-target section also asserts its preconditions. Decoding each
        /// fixture into its real response type turns "the fixture is valid" from
        /// an assumption into a check.
        static func validate() -> [String] {
            let fixtures = SpeechPanelFixtures()
            var broken: [String] = []

            func verify<T: Decodable>(_ name: String, _ json: String, as type: T.Type) {
                do {
                    _ = try JSONDecoder().decode(T.self, from: Data(json.utf8))
                } catch {
                    broken.append("\(name): \(error)")
                }
            }

            verify("profile", fixtures.profileJSON, as: SpeechProfileResponse.self)
            verify("voices", fixtures.catalogJSON, as: VoiceCatalogResponse.self)
            verify("generate", fixtures.generateJSON(annotationID: "a-1"),
                   as: SpeechGenerateResponse.self)
            verify("export", fixtures.exportJSON(annotationID: "a-1"),
                   as: SpeechExportResponse.self)
            verify("play", fixtures.playJSON(annotationID: "a-1"),
                   as: SpeechPlayResponse.self)
            return broken
        }

        private func value(after flag: String, in arguments: [String]) -> String? {
            guard let index = arguments.firstIndex(of: flag),
                  arguments.indices.contains(index + 1) else { return nil }
            return arguments[index + 1]
        }
    }

    /// The catalog must actually reach the pickers.
    ///
    /// `rebuildVoiceMenus` opens with `guard let catalog else { return }`, so a
    /// panel whose `catalog` was never assigned leaves both pickers empty and
    /// reports nothing: the guard returns silently. That is exactly how the
    /// shipped build behaved -- the receipt was decoded, checked for `stale`
    /// and emptiness, and then discarded, while nine unit tests exercised the
    /// builder that no production code called.
    ///
    /// So this drives the real path: a panel whose runner returns a real
    /// catalog, `hasCredential` true, and then `.view` is touched -- which is
    /// what fires `viewDidLoad` and the load task. The other checks call
    /// `loadView()` directly and therefore never reach it.
    private static func checkVoiceMenusAreFilled(book: Book) {
        print("\n音色目录接进面板")
        let profileJSON = """
        {"schema_version":1,"receipt":{"operation":"profile_show","profile":\
        {"provider":"senseaudio","model":"sensenova-tts-2.0","voice_id":"male_0004_a",\
        "emotion_label":null,"style_label":null,"speed":1.0,"volume":1.0,"pitch":0,\
        "verification_status":"unverified","verified_at":null,\
        "audio":{"format":"mp3","sample_rate":32000,"bitrate":128000,"channel":2}},\
        "api_key_env":"SENSEAUDIO_API_KEY","config_path":null,"warnings":[]}}
        """
        let catalogJSON = """
        {"schema_version":1,"receipt":{"operation":"voices","provider":"senseaudio",\
        "fetched_at":"2026-09-30T09:38:03Z","stale":false,"warnings":[],"voices":[\
        {"provider":"senseaudio","source_type":"system","voice_id":"female_0006_a",\
        "voice_name":"温柔御姐","emotion_label":null,"style_label":null,\
        "description":["成熟女声"],"created_time":"2025-09-26"},\
        {"provider":"senseaudio","source_type":"system","voice_id":"female_0033_a",\
        "voice_name":"女声 0033","emotion_label":"平静","style_label":null,\
        "description":[],"created_time":"2025-09-26"},\
        {"provider":"someone-else","source_type":"system","voice_id":"other_1",\
        "voice_name":"别家音色","emotion_label":null,"style_label":null,\
        "description":[],"created_time":"2025-09-26"}]}}
        """
        let panel = makePanel(
            book: book,
            annotation: Annotation(
                id: "h-wired", type: .highlight, chapterTitle: "第一章", locationInfo: "",
                contentText: "接线检查用的正文。", noteText: nil,
                createdAt: Date(timeIntervalSinceReferenceDate: 0)
            ),
            runner: { _, arguments, _ in
                arguments.contains("voices") ? .success(catalogJSON) : .success(profileJSON)
            },
            hasCredential: true
        )

        // Touching .view is what runs viewDidLoad; the load task is async, so
        // the run loop is turned until the menus settle or the budget runs out.
        _ = panel.view
        let group = view(named: "speech.voice-group", in: panel.view) as? NSPopUpButton
        let variant = view(named: "speech.voice-variant", in: panel.view) as? NSPopUpButton
        guard let group, let variant else {
            check("目录接进面板：找得到两个音色下拉", false, "缺少 speech.voice-* 控件")
            return
        }
        let deadline = Date().addingTimeInterval(5)
        while group.numberOfItems == 0 && Date() < deadline {
            RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.02))
        }
        panel.view.layoutSubtreeIfNeeded()

        check("目录接进面板：一级音色下拉已填充", group.numberOfItems > 0,
              "items=\(group.numberOfItems)")
        check("目录接进面板：二级音色下拉已填充", variant.numberOfItems > 0,
              "items=\(variant.numberOfItems)")
        // Two SenseAudio voices under two names; the third is another provider
        // and must not be offered at all (ADR 0007, and the builder's contract).
        check("目录接进面板：按音色名分组且丢弃未测试供应商",
              group.numberOfItems == 2
                  && group.itemTitles.contains("温柔御姐")
                  && group.itemTitles.contains("女声 0033")
                  && !group.itemTitles.contains("别家音色"),
              "titles=\(group.itemTitles)")
        // A picked voice is what makes generate available; without it the panel
        // looks populated but nothing can be produced.
        let generate = view(named: "speech.generate", in: panel.view) as? NSButton
        check("目录接进面板：选定音色后可生成", generate?.isEnabled == true,
              "enabled=\(generate?.isEnabled.description ?? "nil") status=\(view(named: "speech.generation-result", in: panel.view).map { ($0 as? NSTextField)?.stringValue ?? "" } ?? "")")
    }

    private static func checkCardEntry() {
        print("\n选中标注后显示生成卡片入口")
        let book = Book(id: "b1", title: "测试书", author: "某人",
                        totalAnnotations: 2, highlightsCount: 1, notesCount: 1)
        let annotations = [
            Annotation(
                id: "h0", type: .highlight, chapterTitle: "第一章", locationInfo: "",
                contentText: String(repeating: "这是一段较长的正文,用来验证生成卡片按钮不会随着内容宽度移动。", count: 8),
                noteText: nil, createdAt: Date(timeIntervalSinceReferenceDate: 0)
            ),
            sample("n0", .note)
        ]
        let detail = BookDetailView()
        hosted(detail, width: 779, height: 700)
        detail.show(book: book)
        detail.setAnnotations(annotations)

        guard let table = firstTableView(in: detail) else {
            check("标注表格可定位", false, "找不到 NSTableView")
            return
        }
        table.layoutSubtreeIfNeeded()

        guard let firstCell = table.view(atColumn: 0, row: 0, makeIfNecessary: true),
              let secondCell = table.view(atColumn: 0, row: 1, makeIfNecessary: true),
              let firstButton = view(named: "share-card-entry", in: firstCell) as? NSButton,
              let secondButton = view(named: "share-card-entry", in: secondCell) as? NSButton else {
            check("标注行含生成卡片入口", false, "找不到入口按钮")
            return
        }

        check("未选中时入口隐藏", firstButton.isHidden && secondButton.isHidden,
              "first=\(firstButton.isHidden) second=\(secondButton.isHidden)")

        var requested: Annotation?
        detail.onCardRequested = { requested = $0 }
        table.selectRowIndexes(IndexSet(integer: 0), byExtendingSelection: false)
        detail.tableViewSelectionDidChange(Notification(name: NSTableView.selectionDidChangeNotification))
        table.layoutSubtreeIfNeeded()
        firstCell.layoutSubtreeIfNeeded()
        check("选中第一行显示入口", !firstButton.isHidden && secondButton.isHidden,
              "first=\(firstButton.isHidden) second=\(secondButton.isHidden)")
        invoke(firstButton)
        check("入口传出第一条标注", requested?.id == "h0", "id=\(requested?.id ?? "nil")")
        let firstButtonFrame = frame(of: firstButton, in: firstCell)

        table.selectRowIndexes(IndexSet(integer: 1), byExtendingSelection: false)
        detail.tableViewSelectionDidChange(Notification(name: NSTableView.selectionDidChangeNotification))
        table.layoutSubtreeIfNeeded()
        secondCell.layoutSubtreeIfNeeded()
        check("切换后只显示第二行入口", firstButton.isHidden && !secondButton.isHidden,
              "first=\(firstButton.isHidden) second=\(secondButton.isHidden)")
        let secondButtonFrame = frame(of: secondButton, in: secondCell)
        check("生成卡片入口固定在行右侧",
              abs(firstButtonFrame.maxX - secondButtonFrame.maxX) < 1
                  && firstButtonFrame.maxX > firstCell.bounds.maxX - 20
                  && secondButtonFrame.maxX > secondCell.bounds.maxX - 20,
              "long.maxX=\(firstButtonFrame.maxX) short.maxX=\(secondButtonFrame.maxX) cells=\(firstCell.bounds.maxX)/\(secondCell.bounds.maxX)")
        check("生成卡片入口垂直居中",
              abs(firstButtonFrame.midY - firstCell.bounds.midY) <= 1
                  && abs(secondButtonFrame.midY - secondCell.bounds.midY) <= 1,
              "long.midY=\(firstButtonFrame.midY)/\(firstCell.bounds.midY) short.midY=\(secondButtonFrame.midY)/\(secondCell.bounds.midY)")

        // 筛选会重新加载 rows；选中态必须按 Annotation identity 处理，
        // 不能把原来选中的高亮按旧 row index 误投射到新的列表首行。
        guard let filterControl = firstSegmentedControl(in: detail) else {
            check("筛选移除已选标注后清除 Card Entry", false, "找不到类型筛选控件")
            return
        }
        table.selectRowIndexes(IndexSet(integer: 0), byExtendingSelection: false)
        detail.tableViewSelectionDidChange(Notification(name: NSTableView.selectionDidChangeNotification))
        select(segment: 2, in: filterControl)
        table.layoutSubtreeIfNeeded()
        let filteredCell = table.view(atColumn: 0, row: 0, makeIfNecessary: true)
        let filteredButton = filteredCell.flatMap { view(named: "share-card-entry", in: $0) as? NSButton }
        check("筛选移除已选标注后清除 Card Entry",
              table.selectedRow == -1 && (filteredButton?.isHidden ?? false),
              "selectedRow=\(table.selectedRow) buttonHidden=\(filteredButton?.isHidden ?? false)")
    }

    private static func checkShareCardAlternativeLayout() {
        print("\n分享卡片候选布局")
        let book = Book(id: "b1", title: "测试书", author: "某人",
                        totalAnnotations: 1, highlightsCount: 1, notesCount: 0)
        let editor = ShareCardEditorViewController(book: book, annotation: sample("h0", .highlight))
        editor.loadView()
        hosted(editor.view, width: 980, height: 700)
        editor.view.layoutSubtreeIfNeeded()

        guard let changeButton = view(titled: "换一换", in: editor.view) as? NSButton else {
            check("换一换控件可定位", false, "找不到按钮")
            return
        }
        invoke(changeButton)
        editor.view.layoutSubtreeIfNeeded()

        let candidateButtons = buttons(in: editor.view).filter {
            $0.toolTip?.hasPrefix("选择候选卡片") == true
        }
        check("换一换生成四个候选", candidateButtons.count == 4,
              "候选数=\(candidateButtons.count)")
        check("候选区不撑大编辑器", editor.view.bounds.width <= 980.5,
              "editor=\(editor.view.bounds)")
        check("候选缩略图不撑大编辑器",
              candidateButtons.allSatisfy { $0.frame.width <= 120 && $0.frame.height <= 120 },
              "候选尺寸=\(candidateButtons.map { "\(Int($0.frame.width))x\(Int($0.frame.height))" })")

        guard let closeButton = view(titled: "完成", in: editor.view) as? NSButton,
              let closeSuperview = closeButton.superview else {
            check("完成按钮可定位", false, "找不到关闭按钮")
            return
        }
        let closeFrame = closeSuperview.convert(closeButton.frame, to: editor.view)
        check("完成按钮绑定关闭路径",
              closeButton.target === editor && closeButton.action != nil,
              "target=\(String(describing: closeButton.target)), action=\(String(describing: closeButton.action))")
        check("完成按钮仍在编辑器内",
              closeFrame.minX >= -0.5 && closeFrame.maxX <= editor.view.bounds.maxX + 0.5
                  && closeFrame.minY >= -0.5 && closeFrame.maxY <= editor.view.bounds.maxY + 0.5,
              "frame=\(closeFrame) editor=\(editor.view.bounds)")
    }

    private static func checkShareCardRealEntry() {
        print("\n详情控制器入口打开分享卡片编辑器")
        let book = Book(id: "b1", title: "测试书", author: "某人",
                        totalAnnotations: 1, highlightsCount: 1, notesCount: 0)
        let annotation = sample("h0", .highlight)
        let controller = BookDetailViewController()
        controller.loadView()
        let detail = controller.view as! BookDetailView
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 779, height: 700),
            styleMask: [.titled],
            backing: .buffered,
            defer: false
        )
        window.contentViewController = controller
        window.makeKeyAndOrderFront(nil)
        defer { window.orderOut(nil); window.close() }

        controller.show(book: book, annotations: [annotation])
        detail.layoutSubtreeIfNeeded()

        guard let table = firstTableView(in: detail),
              let cell = table.view(atColumn: 0, row: 0, makeIfNecessary: true),
              let cardButton = view(named: "share-card-entry", in: cell) as? NSButton else {
            check("真实详情入口可定位", false, "找不到标注行入口")
            return
        }

        table.selectRowIndexes(IndexSet(integer: 0), byExtendingSelection: false)
        detail.tableViewSelectionDidChange(Notification(name: NSTableView.selectionDidChangeNotification))
        table.layoutSubtreeIfNeeded()
        cell.layoutSubtreeIfNeeded()
        invoke(cardButton)
        runMainLoop(for: 0.05)

        let editor = controller.presentedViewControllers?.compactMap {
            $0 as? ShareCardEditorViewController
        }.first
        check("详情控制器入口打开编辑器", editor != nil,
              "presented=\(controller.presentedViewControllers?.count ?? 0)")
        guard let editor else { return }
        editor.view.layoutSubtreeIfNeeded()
        let preview = view(named: "share-card-preview", in: editor.view) as? NSImageView
        let pageLabel = view(named: "share-card-page-label", in: editor.view) as? NSTextField
        check("入口默认生成预览", preview?.image?.size == ShareCardService.canvasSize,
              "图片尺寸=\(preview?.image?.size ?? .zero)")
        check("入口默认页码可见", pageLabel?.stringValue == "第 1 / 1 页",
              "页码=\(pageLabel?.stringValue ?? "缺失")")
        editor.dismiss(editor)
    }

    private static func checkShareCardActions() {
        print("\n分享卡片动作")
        let book = Book(id: "b1", title: "测试书", author: "某人",
                        totalAnnotations: 1, highlightsCount: 1, notesCount: 0)
        var copiedImages: [NSImage] = []
        let editor = ShareCardEditorViewController(
            book: book,
            annotation: sample("h0", .highlight),
            copyHandler: { images in
                copiedImages = images
                return true
            }
        )
        editor.loadView()
        hosted(editor.view, width: 980, height: 700)
        editor.view.layoutSubtreeIfNeeded()

        guard let copyButton = view(titled: "复制图片", in: editor.view) as? NSButton else {
            check("复制图片控件可定位", false, "找不到按钮")
            return
        }
        check("复制图片默认可用", copyButton.isEnabled, "enabled=\(copyButton.isEnabled)")
        invoke(copyButton)
        check("复制图片传出预览图", copiedImages.count == 1 && copiedImages[0].size == ShareCardService.canvasSize,
              "count=\(copiedImages.count) size=\(copiedImages.first?.size ?? .zero)")
        check("泛用分享面板入口已移除", view(titled: "分享", in: editor.view) == nil,
              "未发现通用分享按钮")

        guard let airDropButton = view(titled: "AirDrop", in: editor.view) as? NSButton else {
            check("AirDrop 控件可定位", false, "找不到 AirDrop 按钮")
            return
        }
        check("AirDrop 按钮保持可见", !airDropButton.isHidden,
              "hidden=\(airDropButton.isHidden) enabled=\(airDropButton.isEnabled)")
    }

    private static func checkShareCardFontSelection() {
        print("\n分享卡片字体")
        let book = Book(id: "b1", title: "测试书", author: "某人",
                        totalAnnotations: 1, highlightsCount: 1, notesCount: 0)
        let editor = ShareCardEditorViewController(book: book, annotation: sample("h0", .highlight))
        editor.loadView()
        hosted(editor.view, width: 980, height: 700)
        editor.view.layoutSubtreeIfNeeded()

        guard let fontPopup = view(named: "share-card-font", in: editor.view) as? NSPopUpButton else {
            check("字体控件可定位", false, "找不到字体下拉框")
            return
        }

        let titles = fontPopup.itemTitles
        check("字体包含全部已接入选项",
              titles == ShareCardFont.allCases.map(\.displayName),
              "字体=\(titles)")

        guard let changeButton = view(titled: "换一换", in: editor.view) as? NSButton else {
            check("字体切换前候选按钮可定位", false, "找不到换一换按钮")
            return
        }
        invoke(changeButton)
        let candidatesBeforeFontChange = buttons(in: editor.view).filter {
            $0.toolTip?.hasPrefix("选择候选卡片") == true
        }
        check("字体切换前生成候选卡片", candidatesBeforeFontChange.count == 4,
              "候选数=\(candidatesBeforeFontChange.count)")

        if let sourceIndex = ShareCardFont.allCases.firstIndex(of: .sourceHanSansSC) {
            fontPopup.selectItem(at: sourceIndex)
            invoke(fontPopup)
            check("选择思源黑体后控件保持选中",
                  fontPopup.indexOfSelectedItem == sourceIndex,
                  "selected=\(fontPopup.indexOfSelectedItem)")
            let candidatesAfterFontChange = buttons(in: editor.view).filter {
                $0.toolTip?.hasPrefix("选择候选卡片") == true
            }
            check("切换字体后清理旧候选卡片", candidatesAfterFontChange.isEmpty,
                  "候选数=\(candidatesAfterFontChange.count)")
        }
    }

    private static func checkShareCardThemeGrid() {
        print("\n分享卡片主题网格")
        let book = Book(id: "b1", title: "测试书", author: "某人",
                        totalAnnotations: 1, highlightsCount: 1, notesCount: 0)
        let editor = ShareCardEditorViewController(book: book, annotation: sample("h0", .highlight))
        editor.loadView()
        hosted(editor.view, width: 980, height: 700)
        editor.view.layoutSubtreeIfNeeded()

        let themeButtons = buttons(in: editor.view).filter {
            $0.identifier?.rawValue.hasPrefix("share-card-theme-") == true
        }
        check("主题网格包含 12 个模板", themeButtons.count == ShareCardTheme.allCases.count,
              "主题数=\(themeButtons.count)")
        let visibleThemeNames = Set(textFields(in: editor.view).map(\.stringValue))
        check("主题缩略图带有可见名称",
              Set(ShareCardTheme.allCases.map(\.displayName)).isSubset(of: visibleThemeNames),
              "名称=\(ShareCardTheme.allCases.map(\.displayName))")

        let themeFrames = themeButtons.map { frame(of: $0, in: editor.view) }
        let themeDimensions = Set(themeFrames.map { "\(Int($0.width.rounded()))x\(Int($0.height.rounded()))" })
        check("主题缩略图尺寸稳定",
              themeDimensions.count == 1
                  && themeFrames.allSatisfy { $0.width <= 80 && $0.height <= 100 },
              "尺寸=\(themeDimensions.sorted())")
        check("主题网格不溢出编辑器",
              themeFrames.allSatisfy {
                  $0.minX >= -0.5 && $0.maxX <= editor.view.bounds.maxX + 0.5
                      && $0.minY >= -0.5 && $0.maxY <= editor.view.bounds.maxY + 0.5
              },
              "范围=\(themeFrames)")
        let themeScrollViews = scrollViews(in: editor.view).filter { scrollView in
            guard let documentView = scrollView.documentView else { return false }
            return scrollView.hasVerticalScroller
                && !scrollView.hasHorizontalScroller
                && buttons(in: documentView).count == ShareCardTheme.allCases.count
        }
        if let themeScrollView = themeScrollViews.first {
            let scrollFrame = frame(of: themeScrollView, in: editor.view)
            let documentHeight = themeScrollView.documentView?.bounds.height ?? 0
            let viewportHeight = themeScrollView.contentView.bounds.height
            check("主题面板有内部滚动且边界固定",
                  scrollFrame.height <= 126.5 && documentHeight > viewportHeight,
                  "面板=\(scrollFrame), 内容高=\(documentHeight), 视口高=\(viewportHeight)")
        } else {
            check("主题面板有内部滚动且边界固定", false, "找不到主题滚动面板")
        }
        check("主题网格有唯一选中态", themeButtons.filter { $0.state == .on }.count == 1,
              "选中数=\(themeButtons.filter { $0.state == .on }.count)")

        let fontPopup = view(named: "share-card-font", in: editor.view) as? NSPopUpButton
        let sizeModePopup = view(named: "share-card-size-mode", in: editor.view) as? NSPopUpButton
        let fontSizePopup = view(named: "share-card-font-size", in: editor.view) as? NSPopUpButton
        let horizontalAlignment = view(named: "share-card-horizontal-alignment", in: editor.view)
            as? NSSegmentedControl
        let verticalAlignment = view(named: "share-card-vertical-alignment", in: editor.view)
            as? NSSegmentedControl
        if let fontPopup, let sizeModePopup, let fontSizePopup,
           let horizontalAlignment, let verticalAlignment {
            fontPopup.selectItem(at: min(1, fontPopup.numberOfItems - 1))
            invoke(fontPopup)
            sizeModePopup.selectItem(at: 1)
            invoke(sizeModePopup)
            fontSizePopup.selectItem(withTitle: "64")
            invoke(fontSizePopup)
            horizontalAlignment.selectedSegment = 2
            invoke(horizontalAlignment)
            verticalAlignment.selectedSegment = 0
            invoke(verticalAlignment)

            let typographyState = (
                font: fontPopup.indexOfSelectedItem,
                sizeMode: sizeModePopup.indexOfSelectedItem,
                fontSize: fontSizePopup.indexOfSelectedItem,
                horizontal: horizontalAlignment.selectedSegment,
                vertical: verticalAlignment.selectedSegment
            )
            if let lastTheme = themeButtons.last {
                invoke(lastTheme)
                check("切换主题后保留排版状态",
                      fontPopup.indexOfSelectedItem == typographyState.font
                          && sizeModePopup.indexOfSelectedItem == typographyState.sizeMode
                      && fontSizePopup.indexOfSelectedItem == typographyState.fontSize
                      && horizontalAlignment.selectedSegment == typographyState.horizontal
                      && verticalAlignment.selectedSegment == typographyState.vertical,
                      "切换前后排版状态保持一致")
            }
        } else {
            check("切换主题后保留排版状态", false, "找不到排版控件")
        }

        if let lastTheme = themeButtons.last {
            check("选择主题后保持唯一选中态",
                  themeButtons.filter { $0.state == .on }.count == 1 && lastTheme.state == .on,
                  "选中数=\(themeButtons.filter { $0.state == .on }.count)")
        }
    }

    private static func checkShareCardTypographyAndPages() {
        print("\n分享卡片排版与多页")
        let book = Book(id: "b1", title: "测试书", author: "某人",
                        totalAnnotations: 1, highlightsCount: 1, notesCount: 0)
        let longText = (0..<220)
            .map { "第 \($0) 段 mixed passage，必须完整保留。 " }
            .joined()
        let annotation = Annotation(
            id: "long-1",
            type: .highlight,
            chapterTitle: "第一章",
            locationInfo: "",
            contentText: longText,
            noteText: nil,
            createdAt: Date(timeIntervalSinceReferenceDate: 0)
        )
        var copiedImages: [NSImage] = []
        let editor = ShareCardEditorViewController(
            book: book,
            annotation: annotation,
            copyHandler: { images in
                copiedImages = images
                return true
            }
        )
        editor.loadView()
        hosted(editor.view, width: 980, height: 700)
        editor.view.layoutSubtreeIfNeeded()

        guard let sizeMode = view(named: "share-card-size-mode", in: editor.view) as? NSPopUpButton,
              let fontSize = view(named: "share-card-font-size", in: editor.view) as? NSPopUpButton,
              let horizontal = view(named: "share-card-horizontal-alignment", in: editor.view)
                  as? NSSegmentedControl,
              let vertical = view(named: "share-card-vertical-alignment", in: editor.view)
                  as? NSSegmentedControl,
              let copyMenu = view(named: "share-card-copy-menu", in: editor.view) as? NSPopUpButton,
              let pageLabel = view(named: "share-card-page-label", in: editor.view) as? NSTextField,
              let preview = view(named: "share-card-preview", in: editor.view) as? NSImageView else {
            check("排版与页码控件可定位", false, "缺少字号、对齐或页码控件")
            return
        }

        check("字号模式包含自动和固定", sizeMode.itemTitles == ["自动字号", "固定字号"],
              "模式=\(sizeMode.itemTitles)")
        check("固定字号选项可用", fontSize.itemTitles.contains("64"),
              "字号=\(fontSize.itemTitles)")
        check("复制菜单提供全部页面", copyMenu.itemTitles == ["复制全部页面"],
              "菜单=\(copyMenu.itemTitles)")

        guard let textView = firstTextView(in: editor.view),
              let textScrollView = scrollViews(in: editor.view).first(where: { $0.documentView === textView }) else {
            check("正文编辑区配置为受边界的垂直滚动", false, "找不到 NSTextView 或其 NSScrollView")
            return
        }
        check("正文编辑区配置为受边界的垂直滚动",
              textScrollView.hasVerticalScroller
                  && !textScrollView.hasHorizontalScroller
                  && textScrollView.autohidesScrollers
                  && textView.isVerticallyResizable
                  && !textView.isHorizontallyResizable
                  && textView.textContainer?.widthTracksTextView == true,
              "vertical=\(textScrollView.hasVerticalScroller) horizontal=\(textScrollView.hasHorizontalScroller) autohide=\(textScrollView.autohidesScrollers) verticalResize=\(textView.isVerticallyResizable) horizontalResize=\(textView.isHorizontallyResizable) widthTracks=\(textView.textContainer?.widthTracksTextView ?? false)")

        sizeMode.selectItem(at: 1)
        invoke(sizeMode)
        fontSize.selectItem(withTitle: "64")
        invoke(fontSize)
        horizontal.selectedSegment = 1
        invoke(horizontal)
        vertical.selectedSegment = 2
        invoke(vertical)
        editor.view.layoutSubtreeIfNeeded()

        let pageButtons = buttons(in: editor.view).filter {
            $0.identifier?.rawValue.hasPrefix("share-card-page-") == true
        }
        check("固定字号长文生成多页缩略图", pageButtons.count > 1,
              "页数=\(pageButtons.count)")
        check("页码状态可见", pageLabel.stringValue == "第 1 / \(pageButtons.count) 页",
              "页码=\(pageLabel.stringValue)")
        check("缩略图带保持在窗口内", pageButtons.allSatisfy { $0.frame.width <= 120 && $0.frame.height <= 120 },
              "缩略图高度=\(pageButtons.map { Int($0.frame.height) })")
        let previewFrame = frame(of: preview, in: editor.view)
        check("大预览保持 3:4 比例",
              previewFrame.height > 0 && abs(previewFrame.width / previewFrame.height - 0.75) < 0.01,
              "预览尺寸=\(previewFrame.size)")
        let previewSize = preview.image?.size ?? .zero
        check("预览使用固定导出画布",
              abs(previewSize.width - ShareCardService.canvasSize.width) < 0.5
                  && abs(previewSize.height - ShareCardService.canvasSize.height) < 0.5,
              "图片尺寸=\(previewSize)")
        check("缩略图显式降采样",
              pageButtons.allSatisfy {
                  guard let image = $0.image else { return false }
                  return abs(image.size.width - 72) < 0.5 && abs(image.size.height - 96) < 0.5
              },
              "图片尺寸=\(pageButtons.compactMap(\.image).map(\.size))")
        let thumbnailScrollViews = scrollViews(in: editor.view).filter { scrollView in
            guard let documentView = scrollView.documentView else { return false }
            return scrollView.hasHorizontalScroller
                && !scrollView.hasVerticalScroller
                && buttons(in: documentView).filter {
                    $0.identifier?.rawValue.hasPrefix("share-card-page-") == true
                }.count == pageButtons.count
        }
        if let thumbnailScrollView = thumbnailScrollViews.first {
            let scrollFrame = frame(of: thumbnailScrollView, in: editor.view)
            let documentWidth = thumbnailScrollView.documentView?.bounds.width ?? 0
            let viewportWidth = thumbnailScrollView.contentView.bounds.width
            check("多页缩略图使用内部横向滚动且边界固定",
                  scrollFrame.height <= 112.5 && documentWidth > viewportWidth,
                  "面板=\(scrollFrame), 内容宽=\(documentWidth), 视口宽=\(viewportWidth)")
        } else {
            check("多页缩略图使用内部横向滚动且边界固定", false, "找不到缩略图滚动面板")
        }

        if pageButtons.count > 1 {
            guard let inspectedPageButton = pageButtons.dropFirst().first else {
                check("点击缩略图切换当前页", false, "找不到非首页缩略图")
                return
            }
            invoke(inspectedPageButton)
            let selectedPageIndex = pageButtons.firstIndex { $0.state == .on }
            let selectedPageNumber = inspectedPageButton.tag + 1
            check("点击缩略图切换当前页",
                  pageLabel.stringValue == "第 \(selectedPageNumber) / \(pageButtons.count) 页",
                  "页码=\(pageLabel.stringValue)")
            check("当前页缩略图选中态唯一",
                  pageButtons.filter { $0.state == .on }.count == 1
                      && inspectedPageButton.state == .on,
                  "选中数=\(pageButtons.filter { $0.state == .on }.count)")
            guard let copyButton = view(titled: "复制图片", in: editor.view) as? NSButton else {
                check("多页复制入口可定位", false, "找不到复制按钮")
                return
            }
            invoke(copyButton)
            let copiedCurrentPageData = copiedImages.first?.tiffRepresentation
            check("多页复制默认只传出当前页", copiedImages.count == 1,
                  "图片数=\(copiedImages.count)")
            invoke(copyMenu)
            let copiedAllPageData = copiedImages.compactMap(\.tiffRepresentation)
            let selectedPageIsPreserved: Bool
            if let selectedPageIndex,
               copiedAllPageData.indices.contains(selectedPageIndex),
               let copiedCurrentPageData {
                selectedPageIsPreserved = copiedAllPageData[selectedPageIndex] == copiedCurrentPageData
            } else {
                selectedPageIsPreserved = false
            }
            let allPagesContainDistinctContent = copiedAllPageData.count == pageButtons.count
                && Set(copiedAllPageData).count == pageButtons.count
            check("复制菜单传出全部页面", copiedImages.count == pageButtons.count,
                  "图片数=\(copiedImages.count)")
            check("当前页与全部页面复制内容范围明确",
                  selectedPageIsPreserved && allPagesContainDistinctContent,
                  "选中页位于全部页对应位置=\(selectedPageIsPreserved), 全部页内容均可区分=\(allPagesContainDistinctContent)")

            if let textView = firstTextView(in: editor.view) {
                textView.string = "改成短文本"
                editor.textDidChange(Notification(name: NSText.didChangeNotification, object: textView))
                editor.view.layoutSubtreeIfNeeded()
                let saveButton = view(titled: "保存 PNG", in: editor.view) as? NSButton
                check("文字防抖期间禁用旧输出",
                      !copyButton.isEnabled && !copyMenu.isEnabled && !(saveButton?.isEnabled ?? true)
                          && pageLabel.stringValue.isEmpty,
                      "复制=\(copyButton.isEnabled), 全部=\(copyMenu.isEnabled), 保存=\(saveButton?.isEnabled ?? false), 页码=\(pageLabel.stringValue)")
                runMainLoop(for: 0.3)
                editor.view.layoutSubtreeIfNeeded()
                let updatedPageButtons = buttons(in: editor.view).filter {
                    $0.identifier?.rawValue.hasPrefix("share-card-page-") == true
                }
                check("页数减少时当前页钳制到末页",
                      pageLabel.stringValue == "第 1 / 1 页" && updatedPageButtons.count == 1,
                      "页码=\(pageLabel.stringValue), 缩略图数=\(updatedPageButtons.count)")
            }
        }
        if let closeButton = view(titled: "完成", in: editor.view) {
            let closeFrame = frame(of: closeButton, in: editor.view)
            check("长文状态下关闭入口保持可见",
                  closeFrame.minX >= -0.5 && closeFrame.minY >= -0.5
                      && closeFrame.maxX <= editor.view.bounds.maxX + 0.5
                      && closeFrame.maxY <= editor.view.bounds.maxY + 0.5,
                  "关闭按钮=\(closeFrame), 编辑器=\(editor.view.bounds)")
        } else {
            check("长文状态下关闭入口保持可见", false, "找不到完成按钮")
        }
    }

    private static func runMainLoop(for duration: TimeInterval) {
        RunLoop.main.run(until: Date(timeIntervalSinceNow: duration))
    }

    private static func invoke(_ item: NSMenuItem) {
        guard let action = item.action else { return }
        NSApp.sendAction(action, to: item.target, from: item)
    }

    private static func invoke(_ control: NSControl) {
        guard let action = control.action else { return }
        NSApp.sendAction(action, to: control.target, from: control)
    }

    private static func allViews(in view: NSView) -> [NSView] {
        [view] + view.subviews.flatMap { self.allViews(in: $0) }
    }

    private static func view(named identifier: String, in view: NSView) -> NSView? {
        if view.identifier?.rawValue == identifier { return view }
        for subview in view.subviews {
            if let found = self.view(named: identifier, in: subview) { return found }
        }
        return nil
    }

    private static func view(titled title: String, in view: NSView) -> NSView? {
        if let control = view as? NSButton, control.title == title { return control }
        for subview in view.subviews {
            if let found = self.view(titled: title, in: subview) { return found }
        }
        return nil
    }

    private static func buttons(in view: NSView) -> [NSButton] {
        let own = view as? NSButton
        return ([own].compactMap { $0 }) + view.subviews.flatMap { buttons(in: $0) }
    }

    private static func scrollViews(in view: NSView) -> [NSScrollView] {
        let own = view as? NSScrollView
        return ([own].compactMap { $0 }) + view.subviews.flatMap { scrollViews(in: $0) }
    }

    private static func frame(of view: NSView, in ancestor: NSView) -> NSRect {
        guard let superview = view.superview else { return .zero }
        return superview.convert(view.frame, to: ancestor)
    }

    private static func firstTableView(in view: NSView) -> NSTableView? {
        if let table = view as? NSTableView { return table }
        for subview in view.subviews {
            if let found = firstTableView(in: subview) { return found }
        }
        return nil
    }

    private static func firstTextView(in view: NSView) -> NSTextView? {
        if let textView = view as? NSTextView { return textView }
        for subview in view.subviews {
            if let found = firstTextView(in: subview) { return found }
        }
        return nil
    }

    private static func textFields(in view: NSView) -> [NSTextField] {
        let own = view as? NSTextField
        return ([own].compactMap { $0 }) + view.subviews.flatMap { textFields(in: $0) }
    }

    private static func checkClassifier() {
        print("\n标注分类(按内容,不按 ZANNOTATIONTYPE)")

        check("有批注 → 笔记",
              AnnotationClassifier.classify(hasNote: true, hasSelectedText: true) == .note,
              "\(String(describing: AnnotationClassifier.classify(hasNote: true, hasSelectedText: true)))")
        check("只有正文 → 高亮",
              AnnotationClassifier.classify(hasNote: false, hasSelectedText: true) == .highlight,
              "\(String(describing: AnnotationClassifier.classify(hasNote: false, hasSelectedText: true)))")
        check("批注但无正文 → 仍是笔记",
              AnnotationClassifier.classify(hasNote: true, hasSelectedText: false) == .note,
              "\(String(describing: AnnotationClassifier.classify(hasNote: true, hasSelectedText: false)))")
        // 这一条正是 bug 的根源:旧代码按 type 把这类空行当成「独立笔记」
        check("无正文也无批注 → 丢弃",
              AnnotationClassifier.classify(hasNote: false, hasSelectedText: false) == nil,
              "\(String(describing: AnnotationClassifier.classify(hasNote: false, hasSelectedText: false)))")

        check("笔记与高亮不重叠",
              AnnotationType.allCases.count == 2
                  && AnnotationType.allCases.contains(.note)
                  && AnnotationType.allCases.contains(.highlight),
              "\(AnnotationType.allCases.map(\.shortName))")

        // 计数为 0 的分段必须置灰,否则又是一个点进去必然空的死路
        let noNotes = Book(id: "b2", title: "只有高亮的书", author: "某人",
                           totalAnnotations: 3, highlightsCount: 3, notesCount: 0)
        let detail = BookDetailView()
        hosted(detail, width: 779, height: 700)
        detail.show(book: noNotes)
        guard let segmented = firstSegmentedControl(in: detail) else {
            check("分段控件可定位", false, "找不到")
            return
        }
        check("笔记为 0 时该段置灰", !segmented.isEnabled(forSegment: 2),
              "enabled=\(segmented.isEnabled(forSegment: 2))")
        check("高亮非 0 时该段可点", segmented.isEnabled(forSegment: 1),
              "enabled=\(segmented.isEnabled(forSegment: 1))")
    }

    private static func checkBookRowCentering() {
        print("\n书单行文字垂直居中")
        guard let (list, table, bookColumn, countColumn) = makeList() else { return }
        list.setBooks([Book(id: "b1", title: "测试书", author: "某人",
                            totalAnnotations: 3, highlightsCount: 3, notesCount: 0)])

        for (name, column) in [("书名", bookColumn), ("笔记数", countColumn)] {
            guard let raw = list.tableView(table, viewFor: column, row: 0) else {
                check("\(name)列能取到 cell", false, "viewFor 返回 nil")
                continue
            }
            // 裸 NSTextField 会被表格撑满整行,单行文字画在顶部 —— 必须是
            // NSTableCellView,内部 label 用 centerY 约束定位。
            guard let cell = raw as? NSTableCellView, let label = cell.textField else {
                check("\(name)列是 NSTableCellView", false, "实际是 \(type(of: raw))")
                continue
            }

            cell.frame = NSRect(x: 0, y: 0, width: 240, height: table.rowHeight)
            cell.layoutSubtreeIfNeeded()

            let offset = abs(label.frame.midY - cell.bounds.midY)
            check("\(name)列文字垂直居中", offset < 1.0,
                  "行高=\(table.rowHeight) cell 中线=\(cell.bounds.midY) 文字中线=\(label.frame.midY) 偏差=\(offset)")
            // 若 label 被拉满行高,midY 会「碰巧」相等但文字仍画在顶部,
            // 所以同时要求它保持自身高度。
            check("\(name)列文字保持自身高度", label.frame.height < cell.bounds.height,
                  "label 高=\(label.frame.height) 行高=\(cell.bounds.height)")
        }
    }
}
