import AppKit

/// Starts the AppKit application from the thin executable target.
public func runBooksExporter() {
    let application = NSApplication.shared
    let delegate = AppDelegate()
    application.delegate = delegate
    application.setActivationPolicy(.regular)
    application.run()
}
