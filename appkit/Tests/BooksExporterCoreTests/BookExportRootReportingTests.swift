import Foundation
import XCTest
@testable import BooksExporterCore

/// `BookService.exportToMarkdown` returns the directory the book's Markdown
/// actually landed in, and the speech panel stores that as the export root for
/// `speech export`.
///
/// The load-bearing case is the full-book one. The Rust exporter creates
/// `outputDirectory/<safe title>/` and writes the notes *inside* it, so the
/// export root is one level **below** the directory the user picked in the open
/// panel. Returning the picked directory instead would put `assets/audio/` and
/// `manifest.json` one level above the notes -- which no failure reports: the
/// export succeeds, the receipt is a success, and `resolve_export_links` looks
/// in `book_dir`, so the Markdown silently never links the audio. That is the
/// same orphan the panel is being fixed to prevent, produced one level up.
final class BookExportRootReportingTests: XCTestCase {
    private let book = Book(
        id: "asset-1",
        title: "Test Book",
        author: "Test Author",
        totalAnnotations: 2,
        highlightsCount: 1,
        notesCount: 1
    )

    func testFullBookExportReportsTheBookDirectoryNotTheChosenOutputDirectory() async throws {
        let client = makeClient { _ in
            .success(#"{"schema_version":1,"receipt":{"asset_id":"asset-1","title":"Test Book","annotation_count":2,"format":"obsidian","output_directory":"/tmp/books","generated_files":["/tmp/books/Test Book/Test Book.md"]}}"#)
        }
        let service = BookService(rustCLIClient: client)

        let root = try await service.exportToMarkdown(
            book: book,
            annotations: [annotation(id: "a-1"), annotation(id: "a-2")],
            outputURL: URL(fileURLWithPath: "/tmp/books")
        )

        XCTAssertEqual(root?.path, "/tmp/books/Test Book")
    }

    /// The receipt is the only source of the real location, so a receipt without
    /// one must yield nothing rather than a guess. Guessing here is what would
    /// reintroduce a second copy of the Rust title sanitiser.
    func testFullBookExportReportsNothingWhenTheReceiptNamesNoFile() async throws {
        let client = makeClient { _ in
            .success(#"{"schema_version":1,"receipt":{"asset_id":"asset-1","title":"Test Book","annotation_count":2,"format":"obsidian","output_directory":"/tmp/books","generated_files":[]}}"#)
        }
        let service = BookService(rustCLIClient: client)

        let root = try await service.exportToMarkdown(
            book: book,
            annotations: [annotation(id: "a-1"), annotation(id: "a-2")],
            outputURL: URL(fileURLWithPath: "/tmp/books")
        )

        XCTAssertNil(root)
    }

    /// The filtered path writes `outputURL/<title>.md` directly, so its export
    /// root is the chosen directory itself -- the opposite of the full-book
    /// path. Reporting one rule for both would move one of them to the wrong
    /// level.
    func testFilteredExportReportsTheDirectoryItWroteInto() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("filtered-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: directory) }

        let failing = makeClient { arguments in
            .failure(status: 1, stderr: "the filtered path must not call the CLI: \(arguments)")
        }
        let service = BookService(rustCLIClient: failing)

        let root = try await service.exportToMarkdown(
            book: book,
            annotations: [annotation(id: "a-1")],
            outputURL: directory
        )

        // Compared by path, not by URL: `deletingLastPathComponent` returns a
        // URL carrying a trailing slash while `appendingPathComponent` does not,
        // so URL equality here would be testing Foundation's representation
        // rather than the answer. The path is the answer -- it is what gets
        // handed to `speech export --output` and what the store persists.
        XCTAssertEqual(root?.path, directory.path)
        XCTAssertTrue(
            FileManager.default.fileExists(
                atPath: directory.appendingPathComponent("Test Book.md").path
            ),
            "precondition: the note really was written there"
        )
    }

    // MARK: - Helpers

    private func annotation(id: String) -> Annotation {
        Annotation(
            id: id,
            type: .highlight,
            chapterTitle: "",
            locationInfo: "1",
            contentText: "highlight",
            noteText: nil,
            createdAt: nil
        )
    }

    private func makeClient(
        result: @escaping @Sendable ([String]) -> RustCLICommandResult
    ) -> RustCLIClient {
        RustCLIClient(
            executableURL: URL(fileURLWithPath: "/tmp/apple-books-exporter"),
            runner: { _, arguments, _ in result(arguments) }
        )
    }
}
