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
}
