import AppKit

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var window: NSWindow!
    private var settingsWindowController: SettingsWindowController?
    private var updateChecker: UpdateChecker?
    private var updateCheckTask: Task<Void, Never>?

    private static let appName = "Books Exporter"

    func applicationDidFinishLaunching(_ notification: Notification) {
        updateChecker = UpdateChecker.makeForCurrentApp()
        NSApp.mainMenu = MainMenu.build(appName: AppDelegate.appName)

        let viewController = MainViewController()
        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1200, height: 720),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.contentViewController = viewController
        window.title = AppDelegate.appName
        window.setContentSize(NSSize(width: 1200, height: 720))
        window.contentMinSize = MainViewController.minimumContentSize
        window.center()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        scheduleAutomaticUpdateCheck()
    }

    deinit {
        updateCheckTask?.cancel()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    @objc func checkForUpdates(_ sender: Any?) {
        updateCheckTask?.cancel()
        updateCheckTask = Task { @MainActor [weak self] in
            guard let self else { return }
            guard let updateChecker else {
                showUpdateStatus(
                    message: "暂时无法检查更新",
                    details: "版本检查仅支持正式打包的 App。"
                )
                return
            }

            let result = await updateChecker.check(trigger: .manual)
            handleUpdateCheckResult(result, trigger: .manual)
        }
    }

    @objc func showSettings(_ sender: Any?) {
        if settingsWindowController == nil {
            settingsWindowController = SettingsWindowController()
        }
        settingsWindowController?.showWindow(sender)
        settingsWindowController?.window?.makeKeyAndOrderFront(sender)
        NSApp.activate(ignoringOtherApps: true)
    }

    private func scheduleAutomaticUpdateCheck() {
        guard updateChecker != nil else { return }

        updateCheckTask = Task { @MainActor [weak self] in
            try? await Task.sleep(nanoseconds: 1_000_000_000)
            guard !Task.isCancelled,
                  let self,
                  let updateChecker = self.updateChecker else {
                return
            }

            let result = await updateChecker.check(trigger: .automatic)
            guard !Task.isCancelled else { return }
            self.handleUpdateCheckResult(result, trigger: .automatic)
        }
    }

    private func handleUpdateCheckResult(_ result: UpdateCheckResult, trigger: UpdateCheckTrigger) {
        switch result {
        case let .available(manifest, shouldNotify):
            guard trigger == .manual || shouldNotify else { return }
            presentUpdate(manifest)
        case .upToDate:
            guard trigger == .manual else { return }
            showUpdateStatus(
                message: "已是最新版本",
                details: "当前版本 \(currentAppVersionDescription)。"
            )
        case .skipped:
            break
        case .unavailable:
            guard trigger == .manual else { return }
            showUpdateStatus(
                message: "暂时无法检查更新",
                details: "请稍后重试。"
            )
        }
    }

    private func presentUpdate(_ manifest: UpdateManifest) {
        let alert = NSAlert()
        alert.messageText = "发现新版本 \(manifest.version)"
        alert.informativeText = manifest.notes.isEmpty
            ? "可以打开官方发布页面查看详情。"
            : manifest.notes
        alert.alertStyle = .informational
        alert.addButton(withTitle: "查看更新")
        alert.addButton(withTitle: "稍后")

        let openReleasePage: (NSApplication.ModalResponse) -> Void = { response in
            guard response == .alertFirstButtonReturn else { return }
            NSWorkspace.shared.open(manifest.releaseURL)
        }

        if let window {
            alert.beginSheetModal(for: window, completionHandler: openReleasePage)
        } else {
            openReleasePage(alert.runModal())
        }
    }

    private func showUpdateStatus(message: String, details: String) {
        let alert = NSAlert()
        alert.messageText = message
        alert.informativeText = details
        alert.alertStyle = .informational
        alert.addButton(withTitle: "确定")

        if let window {
            alert.beginSheetModal(for: window)
        } else {
            alert.runModal()
        }
    }

    private var currentAppVersionDescription: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "未知"
    }
}
