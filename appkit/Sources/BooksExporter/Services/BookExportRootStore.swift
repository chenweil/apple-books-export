import Foundation

/// Remembers the directory each book's Markdown export actually landed in.
///
/// The speech panel needs this because `speech export` accepts **any** writable
/// directory: it creates `assets/audio/` and commits a manifest without asking
/// whether that directory is this book's export root. Picking `~/Downloads`
/// there succeeds, writes a manifest, and produces an orphan -- audio that no
/// exported note will ever link to, because `resolve_export_links` only looks
/// in `book_dir`, the directory the Markdown exporter wrote the book into.
///
/// So the panel needs to offer the real one. The stored path is whatever the
/// export wrote to, never a path derived from the book title: deriving it here
/// would need a second copy of the Rust `safe_path_component` sanitiser, and
/// the copies would disagree the first time a title contains a character they
/// treat differently -- which is exactly the case where a wrong export root
/// silently orphans the audio. `BookService.exportToMarkdown` returns the real
/// directory from the receipt it already has, and that is what gets recorded.
///
/// A miss is not an error. It means "this app has not exported this book yet",
/// and the panel says so rather than guessing a location.
final class BookExportRootStore {
    static let shared = BookExportRootStore()

    static let storageKey = "appkit.bookExportRoots"

    private let defaults: UserDefaults

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    /// The recorded export root for a book, or nil when there is no usable one.
    ///
    /// Rebuilt without `isDirectory: true` on purpose: that flag makes Foundation
    /// keep a trailing slash, so the value read back is not `==` to the URL that
    /// was recorded even though the path is the same. A store whose round trip
    /// is not the identity is a store callers start working around.
    func exportRoot(forAssetID assetID: String) -> URL? {
        guard !assetID.isEmpty else { return nil }
        guard let path = stored()[assetID], !path.isEmpty else { return nil }
        return URL(fileURLWithPath: path)
    }

    func record(assetID: String, exportRoot: URL) {
        guard !assetID.isEmpty else { return }
        var all = stored()
        all[assetID] = exportRoot.path
        defaults.set(all, forKey: Self.storageKey)
    }

    /// Forgets one book, so a book whose export directory was moved or deleted
    /// stops offering a path that no longer holds its notes.
    func forget(assetID: String) {
        guard !assetID.isEmpty else { return }
        var all = stored()
        guard all.removeValue(forKey: assetID) != nil else { return }
        defaults.set(all, forKey: Self.storageKey)
    }

    /// A book root is a directory, not a file path. `FileManager` says so
    /// directly, and the speech panel asks before writing into it -- a recorded
    /// path that is gone, or that turned into a file, must read as "no export
    /// root" instead of sending the export somewhere else.
    func existingExportRoot(forAssetID assetID: String) -> URL? {
        guard let root = exportRoot(forAssetID: assetID) else { return nil }
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: root.path, isDirectory: &isDirectory),
              isDirectory.boolValue else {
            return nil
        }
        return root
    }

    private func stored() -> [String: String] {
        defaults.dictionary(forKey: Self.storageKey) as? [String: String] ?? [:]
    }
}
