import AppKit

// UI 回归探针。直接编译进真实源码(见 verify-ui.sh),
// 不重建约束,因此不会与实现漂移。

private var failures: [String] = []

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
        checkSplitView()
        checkDetailLayout()
        checkAnnotationRowHeight()
        checkSortAccessibility()
        checkAnnotationFilter()
        checkExportScope()
        checkCardEntry()
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

    private static func checkCardEntry() {
        print("\n选中标注后显示生成卡片入口")
        let book = Book(id: "b1", title: "测试书", author: "某人",
                        totalAnnotations: 2, highlightsCount: 1, notesCount: 1)
        let annotations = [
            sample("h0", .highlight),
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
        check("选中第一行显示入口", !firstButton.isHidden && secondButton.isHidden,
              "first=\(firstButton.isHidden) second=\(secondButton.isHidden)")
        invoke(firstButton)
        check("入口传出第一条标注", requested?.id == "h0", "id=\(requested?.id ?? "nil")")

        table.selectRowIndexes(IndexSet(integer: 1), byExtendingSelection: false)
        detail.tableViewSelectionDidChange(Notification(name: NSTableView.selectionDidChangeNotification))
        check("切换后只显示第二行入口", firstButton.isHidden && !secondButton.isHidden,
              "first=\(firstButton.isHidden) second=\(secondButton.isHidden)")
    }

    private static func invoke(_ item: NSMenuItem) {
        guard let action = item.action else { return }
        NSApp.sendAction(action, to: item.target, from: item)
    }

    private static func invoke(_ control: NSControl) {
        guard let action = control.action else { return }
        NSApp.sendAction(action, to: control.target, from: control)
    }

    private static func view(named identifier: String, in view: NSView) -> NSView? {
        if view.identifier?.rawValue == identifier { return view }
        for subview in view.subviews {
            if let found = self.view(named: identifier, in: subview) { return found }
        }
        return nil
    }

    private static func firstTableView(in view: NSView) -> NSTableView? {
        if let table = view as? NSTableView { return table }
        for subview in view.subviews {
            if let found = firstTableView(in: subview) { return found }
        }
        return nil
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
