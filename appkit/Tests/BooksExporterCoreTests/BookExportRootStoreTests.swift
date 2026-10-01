import Foundation
import XCTest
@testable import BooksExporterCore

/// The store is the only thing standing between "the panel remembered where
/// this book went" and "the panel silently exported into an orphan directory",
/// so the cases that matter are the ones where a *recorded* path must not be
/// offered: a book that was never exported, and a book whose directory has since
/// moved or turned into a file.
final class BookExportRootStoreTests: XCTestCase {
    private var defaults: UserDefaults!
    private var suiteName: String!

    override func setUp() {
        super.setUp()
        suiteName = "books-exporter-export-roots-\(UUID().uuidString)"
        defaults = UserDefaults(suiteName: suiteName)
    }

    override func tearDown() {
        defaults.removePersistentDomain(forName: suiteName)
        defaults = nil
        suiteName = nil
        super.tearDown()
    }

    private func makeStore() -> BookExportRootStore {
        BookExportRootStore(defaults: defaults)
    }

    func testARecordedRootComesBackForTheSameBook() {
        let store = makeStore()
        let root = URL(fileURLWithPath: "/Users/someone/books-exported/100 Go Mistakes")

        store.record(assetID: "asset-1", exportRoot: root)

        XCTAssertEqual(store.exportRoot(forAssetID: "asset-1"), root)
    }

    func testABookThatWasNeverExportedHasNoRoot() {
        XCTAssertNil(makeStore().exportRoot(forAssetID: "asset-unknown"))
    }

    /// The key is the asset ID, not the title: two books can share a title, and
    /// a lookup that fell back to one would hand book A the directory of book B
    /// -- two manifests, one directory, and whichever exported last owns it.
    func testRootsAreKeyedByAssetIDAndDoNotLeakBetweenBooks() {
        let store = makeStore()
        let first = URL(fileURLWithPath: "/tmp/books-exported/Shared Title")
        let second = URL(fileURLWithPath: "/tmp/other-export/Shared Title")

        store.record(assetID: "asset-1", exportRoot: first)
        store.record(assetID: "asset-2", exportRoot: second)

        XCTAssertEqual(store.exportRoot(forAssetID: "asset-1"), first)
        XCTAssertEqual(store.exportRoot(forAssetID: "asset-2"), second)
    }

    func testAnEmptyAssetIDIsNeitherRecordedNorFound() {
        let store = makeStore()
        store.record(assetID: "", exportRoot: URL(fileURLWithPath: "/tmp/anywhere"))
        XCTAssertNil(store.exportRoot(forAssetID: ""))
    }

    func testAnEmptyStoredPathReadsAsNoRoot() {
        defaults.set(["asset-1": ""], forKey: BookExportRootStore.storageKey)
        XCTAssertNil(makeStore().exportRoot(forAssetID: "asset-1"))
    }

    /// A value of the wrong shape must not crash the panel on open -- it reads
    /// the store while building its view.
    func testAValueOfTheWrongShapeReadsAsEmptyRatherThanCrashing() {
        defaults.set("not-a-dictionary", forKey: BookExportRootStore.storageKey)
        let store = makeStore()
        XCTAssertNil(store.exportRoot(forAssetID: "asset-1"))

        // ... and a store that read garbage can still record over it.
        store.record(assetID: "asset-1", exportRoot: URL(fileURLWithPath: "/tmp/books"))
        XCTAssertEqual(
            store.exportRoot(forAssetID: "asset-1")?.path,
            "/tmp/books"
        )
    }

    func testForgetRemovesOnlyTheNamedBook() {
        let store = makeStore()
        store.record(assetID: "asset-1", exportRoot: URL(fileURLWithPath: "/tmp/one"))
        store.record(assetID: "asset-2", exportRoot: URL(fileURLWithPath: "/tmp/two"))

        store.forget(assetID: "asset-1")

        XCTAssertNil(store.exportRoot(forAssetID: "asset-1"))
        XCTAssertEqual(store.exportRoot(forAssetID: "asset-2")?.path, "/tmp/two")
    }

    // MARK: - existingExportRoot

    /// The recorded path is only offered while it is still a directory that
    /// exists. Offering a moved or deleted one sends the export somewhere the
    /// book's notes are not, which is the orphan this store exists to prevent.
    func testARecordedRootThatNoLongerExistsIsNotOffered() {
        let store = makeStore()
        let missing = FileManager.default.temporaryDirectory
            .appendingPathComponent("gone-\(UUID().uuidString)")
        store.record(assetID: "asset-1", exportRoot: missing)

        XCTAssertNotNil(store.exportRoot(forAssetID: "asset-1"), "precondition: it was recorded")
        XCTAssertNil(store.existingExportRoot(forAssetID: "asset-1"))
    }

    func testARecordedPathThatIsAFileIsNotOffered() throws {
        let store = makeStore()
        let file = FileManager.default.temporaryDirectory
            .appendingPathComponent("not-a-dir-\(UUID().uuidString)")
        try Data("x".utf8).write(to: file)
        addTeardownBlock { try? FileManager.default.removeItem(at: file) }
        store.record(assetID: "asset-1", exportRoot: file)

        XCTAssertNil(store.existingExportRoot(forAssetID: "asset-1"))
    }

    func testAnExistingDirectoryIsOffered() throws {
        let store = makeStore()
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("books-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: directory) }
        store.record(assetID: "asset-1", exportRoot: directory)

        // By path, not by URL: the directory URL built here and the one the
        // store rebuilds from a stored string differ in trailing slash, and only
        // the path is what the panel hands to `speech export --output`.
        XCTAssertEqual(store.existingExportRoot(forAssetID: "asset-1")?.path, directory.path)
    }
}
