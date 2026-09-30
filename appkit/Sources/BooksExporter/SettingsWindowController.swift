import AppKit

final class SettingsWindowController: NSWindowController {
    init(
        settingsStore: AppSettingsStore = .shared,
        rustClient: RustCLIClient = .makeForCurrentApp()
    ) {
        // The window grew a speech section, so it is taller than the previous
        // single-row layout. Sized for the longest line the note can wrap to
        // rather than for the first line, so the section is not clipped at the
        // bottom on a narrow window.
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 560, height: 360),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )
        window.title = "设置"
        window.contentMinSize = NSSize(width: 520, height: 320)
        window.isReleasedWhenClosed = false
        window.center()

        let keychain = KeychainStore()
        let resolver = SpeechCredentialResolver(client: rustClient, keychain: keychain)

        super.init(window: window)
        window.contentViewController = SettingsViewController(
            settingsStore: settingsStore,
            speechClient: rustClient,
            credentialResolver: resolver
        )
    }

    required init?(coder: NSCoder) {
        fatalError("不支持从 Interface Builder 加载")
    }
}
