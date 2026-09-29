import AppKit

final class SettingsWindowController: NSWindowController {
    init(settingsStore: AppSettingsStore = .shared) {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 500, height: 220),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )
        window.title = "设置"
        window.contentMinSize = NSSize(width: 460, height: 180)
        window.isReleasedWhenClosed = false
        window.center()

        super.init(window: window)
        window.contentViewController = SettingsViewController(settingsStore: settingsStore)
    }

    required init?(coder: NSCoder) {
        fatalError("不支持从 Interface Builder 加载")
    }
}
