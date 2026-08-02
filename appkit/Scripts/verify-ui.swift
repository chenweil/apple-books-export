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
}
