import AppKit

/// 纯代码构建主菜单。没有主菜单时,系统标准快捷键(⌘C/⌘V/⌘X/⌘A/⌘Q/⌘W)
/// 全部失效 —— 它们靠菜单项的 keyEquivalent 派发到 responder chain。
enum MainMenu {
    static func build(appName: String) -> NSMenu {
        let mainMenu = NSMenu()
        mainMenu.addItem(submenu(titled: appName, items: appMenuItems(appName: appName)))
        mainMenu.addItem(submenu(titled: "编辑", items: editMenuItems()))

        let windowMenu = submenu(titled: "窗口", items: windowMenuItems())
        mainMenu.addItem(windowMenu)
        NSApp.windowsMenu = windowMenu.submenu

        return mainMenu
    }

    private static func submenu(titled title: String, items: [NSMenuItem]) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        let menu = NSMenu(title: title)
        items.forEach { menu.addItem($0) }
        item.submenu = menu
        return item
    }

    private static func appMenuItems(appName: String) -> [NSMenuItem] {
        let hideOthers = NSMenuItem(
            title: "隐藏其他",
            action: #selector(NSApplication.hideOtherApplications(_:)),
            keyEquivalent: "h"
        )
        hideOthers.keyEquivalentModifierMask = [.command, .option]

        let settings = NSMenuItem(
            title: "设置…",
            action: #selector(AppDelegate.showSettings(_:)),
            keyEquivalent: ","
        )
        settings.keyEquivalentModifierMask = [.command]
        settings.target = NSApp.delegate

        return [
            NSMenuItem(
                title: "关于 \(appName)",
                action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)),
                keyEquivalent: ""
            ),
            settings,
            .separator(),
            NSMenuItem(title: "隐藏 \(appName)", action: #selector(NSApplication.hide(_:)), keyEquivalent: "h"),
            hideOthers,
            NSMenuItem(title: "显示全部", action: #selector(NSApplication.unhideAllApplications(_:)), keyEquivalent: ""),
            .separator(),
            NSMenuItem(title: "退出 \(appName)", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        ]
    }

    private static func editMenuItems() -> [NSMenuItem] {
        let redo = NSMenuItem(title: "重做", action: Selector(("redo:")), keyEquivalent: "Z")
        redo.keyEquivalentModifierMask = [.command, .shift]

        return [
            NSMenuItem(title: "撤销", action: Selector(("undo:")), keyEquivalent: "z"),
            redo,
            .separator(),
            NSMenuItem(title: "剪切", action: #selector(NSText.cut(_:)), keyEquivalent: "x"),
            NSMenuItem(title: "拷贝", action: #selector(NSText.copy(_:)), keyEquivalent: "c"),
            NSMenuItem(title: "粘贴", action: #selector(NSText.paste(_:)), keyEquivalent: "v"),
            NSMenuItem(title: "全选", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        ]
    }

    private static func windowMenuItems() -> [NSMenuItem] {
        [
            NSMenuItem(title: "最小化", action: #selector(NSWindow.performMiniaturize(_:)), keyEquivalent: "m"),
            NSMenuItem(title: "缩放", action: #selector(NSWindow.performZoom(_:)), keyEquivalent: ""),
            .separator(),
            NSMenuItem(title: "关闭", action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
        ]
    }
}
