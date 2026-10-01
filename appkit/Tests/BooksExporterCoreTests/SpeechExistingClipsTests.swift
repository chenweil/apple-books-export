import XCTest
@testable import BooksExporterCore

/// The panel's answer to "what audio does this annotation already have?".
///
/// The filter is the load-bearing part, and it is a *filter* because the cache
/// report is a whole-machine view: one report carries every clip the machine
/// has ever cached, for every book. Getting it wrong does not fail loudly --
/// the panel lists a neighbour's audio under this annotation, and the user
/// plays and exports the wrong file. So each rejection rule gets its own case,
/// including the shapes that only appear when something has already gone wrong.
final class SpeechExistingClipsTests: XCTestCase {

    // MARK: - Fixtures

    private func entry(
        clipID: String,
        assetID: String?,
        annotationID: String?,
        contentKind: String?,
        durationMs: Int? = 11_412,
        status: String = "ready",
        accepted: Bool = true
    ) -> SpeechCacheEntry {
        // Built through the decoder rather than the memberwise initialiser:
        // these are the field names the contract actually uses, so a rename on
        // either side shows up here rather than as a silent default.
        let kind = contentKind.map { "\"\($0)\"" } ?? "null"
        let json = """
        {"clip_id":"\(clipID)","asset_id":\(assetID.map { "\"\($0)\"" } ?? "null"),\
        "annotation_id":\(annotationID.map { "\"\($0)\"" } ?? "null"),\
        "content_kind":\(kind),\
        "duration_ms":\(durationMs.map(String.init) ?? "null"),\
        "status":"\(status)","accepted":\(accepted)}
        """
        return try! JSONDecoder().decode(SpeechCacheEntry.self, from: Data(json.utf8))
    }

    private let mine = "a-mine"
    private let other = "a-other"
    private let book = "b-mine"

    // MARK: - What the panel keeps

    func testKeepsOnlyThisBookAndAnnotationsReadyClips() throws {
        let kept = entry(clipID: "c1", assetID: book, annotationID: mine,
                         contentKind: "highlight")

        let reports = [
            entry(clipID: "other-book", assetID: "b-somewhere-else",
                  annotationID: mine, contentKind: "highlight"),
            entry(clipID: "other-note", assetID: book, annotationID: other,
                  contentKind: "highlight"),
            entry(clipID: "absent", assetID: book, annotationID: mine,
                  contentKind: "highlight", durationMs: nil, status: "absent",
                  accepted: false),
            entry(clipID: "corrupt", assetID: book, annotationID: mine,
                  contentKind: "highlight", durationMs: nil, status: "corrupt",
                  accepted: false),
        ]

        let result = ([kept] + reports).playable(forAssetID: book, annotationID: mine)

        XCTAssertEqual(result.map(\.clipID), ["c1"])
    }

    /// The rejection that must not be softened.
    ///
    /// A `corrupt` entry has no trustworthy identity -- Rust deliberately does
    /// not fall back to the state file, because a borrowed identity contradicts
    /// the `status` sitting on the same row. So the three are `null`. Filing it
    /// under some annotation anyway is the worst outcome available: the panel
    /// would offer to play and export audio that belongs to a different book, or
    /// to nothing at all, and nothing in the UI would say so.
    func testUnattributableEntriesAreNeverFiledUnderAnAnnotation() {
        let unowned = entry(clipID: "orphan", assetID: nil, annotationID: nil,
                            contentKind: nil, durationMs: nil, status: "corrupt",
                            accepted: false)

        XCTAssertTrue([unowned].playable(forAssetID: book, annotationID: mine).isEmpty)

        // And a half-known entry is no better: an annotation ID with no book is
        // not a match either, and `nil` must not compare equal to `nil` in a
        // way that lets it through.
        let halfKnown = entry(clipID: "half", assetID: book, annotationID: nil,
                              contentKind: "highlight")
        XCTAssertTrue([halfKnown].playable(forAssetID: book, annotationID: mine).isEmpty)
    }

    /// A `ready` entry with no content kind cannot be rendered as a row, and
    /// guessing 「高亮」 would put a note's audio under the wrong half of the
    /// annotation.
    func testAReadyEntryWithoutAContentKindIsDropped() {
        let kindless = entry(clipID: "c-kindless", assetID: book, annotationID: mine,
                             contentKind: nil)

        XCTAssertTrue([kindless].playable(forAssetID: book, annotationID: mine).isEmpty)
    }

    /// Highlight and note are two independent clips of the same annotation, and
    /// the panel has to be able to tell them apart -- that is the whole reason
    /// the contract reports `content_kind` instead of leaving it to be inferred.
    func testHighlightAndNoteBothSurviveAsSeparateClips() throws {
        let highlight = entry(clipID: "c-high", assetID: book, annotationID: mine,
                              contentKind: "highlight")
        let note = entry(clipID: "c-note", assetID: book, annotationID: mine,
                         contentKind: "note", durationMs: 8_000)

        let result = [highlight, note].playable(forAssetID: book, annotationID: mine)

        XCTAssertEqual(result.count, 2)
        XCTAssertEqual(Set(result.map(\.contentKind)), ["highlight", "note"])
        XCTAssertEqual(result.first { $0.contentKind == "note" }?.durationMs, 8_000)
    }

    /// The order the report arrives in is the order the panel shows, so
    /// reopening the panel does not reshuffle the list under the user's cursor.
    func testOrderFollowsTheReport() {
        let entries = ["c-c", "c-a", "c-b"].map {
            entry(clipID: $0, assetID: book, annotationID: mine, contentKind: "highlight")
        }

        XCTAssertEqual(
            entries.playable(forAssetID: book, annotationID: mine).map(\.clipID),
            ["c-c", "c-a", "c-b"]
        )
    }

    func testAnAnnotationWithNothingCachedProducesAnEmptyList() {
        XCTAssertTrue(
            [SpeechCacheEntry]().playable(forAssetID: book, annotationID: mine).isEmpty
        )
    }

    // MARK: - The call itself

    private static let cacheStatusJSON = """
    {"schema_version":1,"receipt":{"operation":"cache_status","budget_bytes":1073741824,\
    "safety_margin_bytes":10485760,"usable_budget_bytes":1063256064,"used_bytes":1024,\
    "accepted_entries":1,"absent_entries":0,"blocked_entries":0,"corrupt_entries":0,\
    "locked_entries":0,"reclaimable_versions":0,\
    "entries":[{"clip_id":"c-1","asset_id":"b-1","annotation_id":"a-1",\
    "content_kind":"highlight","duration_ms":11412,"status":"ready","accepted":true,\
    "generation_blocked":false,"in_use":null,"used_bytes":1024,\
    "reclaimable_versions":0,"last_used_at":"2026-10-01T00:00:00Z"}],\
    "warnings":[]}}
    """

    private final class CacheRecorder: @unchecked Sendable {
        private let lock = NSLock()
        private var seen: [(arguments: [String], environment: [String: String]?)] = []
        func record(_ arguments: [String], _ environment: [String: String]?) -> RustCLICommandResult {
            lock.lock()
            seen.append((arguments, environment))
            lock.unlock()
            return .success(SpeechExistingClipsTests.cacheStatusJSON)
        }
        var recorded: [(arguments: [String], environment: [String: String]?)] {
            lock.lock()
            defer { lock.unlock() }
            return seen
        }
    }

    func testCacheStatusDecodesTheContractShape() async throws {
        let recorder = CacheRecorder()
        let service = SpeechService(
            client: RustCLIClient(
                executableURL: URL(fileURLWithPath: "/tmp/apple-books-exporter"),
                runner: { _, arguments, environment in recorder.record(arguments, environment) }
            )
        )

        let entries = try await service.cacheStatus()

        let clip = try XCTUnwrap(entries.first)
        XCTAssertEqual(clip.clipID, "c-1")
        XCTAssertEqual(clip.assetID, "b-1")
        XCTAssertEqual(clip.annotationID, "a-1")
        XCTAssertEqual(clip.contentKind, "highlight")
        XCTAssertEqual(clip.durationMs, 11_412)
        XCTAssertEqual(clip.status, "ready")

        let call = try XCTUnwrap(recorder.recorded.first)
        XCTAssertEqual(call.arguments, ["speech", "cache", "status", "--json"])
    }

    /// `cache status` is a local read. If it ever acquired a credential, the
    /// panel would be asking the keychain for a secret in order to *list files
    /// the user already owns* -- and 重新检查 on a machine with no key would
    /// start failing.
    func testCacheStatusTakesNoCredential() async throws {
        let recorder = CacheRecorder()
        let service = SpeechService(
            client: RustCLIClient(
                executableURL: URL(fileURLWithPath: "/tmp/apple-books-exporter"),
                runner: { _, arguments, environment in recorder.record(arguments, environment) }
            )
        )

        _ = try await service.cacheStatus()

        XCTAssertNil(recorder.recorded.first?.environment)
    }
}
