import Foundation
import XCTest
@testable import BooksExporterCore

final class RustCLIServiceTests: XCTestCase {
    func testBookServiceUsesRustClientForListAndFullExport() async throws {
        let capture = CommandCapture()
        let client = makeClient(capture: capture) { arguments in
            switch arguments.first {
            case "list":
                return .success(#"{"schema_version":1,"books":[{"asset_id":"asset-1","title":"Test Book","author":"Test Author","note_count":1}]}"#)
            case "export":
                return .success(#"{"schema_version":1,"receipt":{"asset_id":"asset-1","title":"Test Book","annotation_count":1,"format":"obsidian","output_directory":"/tmp/books","generated_files":["/tmp/books/Test Book.md"]}}"#)
            default:
                XCTFail("unexpected command: \(arguments)")
                return .failure(status: 99, stderr: "unexpected command")
            }
        }
        let service = BookService(rustCLIClient: client)

        let books = await service.listBooks()
        XCTAssertEqual(books.first?.id, "asset-1")

        let annotation = Annotation(
            id: "annotation-1",
            type: .highlight,
            chapterTitle: "",
            locationInfo: "1",
            contentText: "highlight",
            noteText: nil,
            createdAt: nil
        )
        try await service.exportToMarkdown(
            book: try XCTUnwrap(books.first),
            annotations: [annotation],
            outputURL: URL(fileURLWithPath: "/tmp/books")
        )

        XCTAssertEqual(capture.arguments, [
            ["list", "--json"],
            [
                "export", "--asset-id", "asset-1", "--json",
                "--output", "/tmp/books", "--format", "obsidian"
            ]
        ])
    }

    func testProcessRunnerKeepsSuccessfulJSONOnStdoutSeparateFromStderr() async throws {
        let scriptURL = try makeExecutableScript(
            """
            #!/bin/sh
            printf '%s' '{"schema_version":1,"books":[{"asset_id":"asset-1","title":"Test Book","author":"Test Author","note_count":1}]}'
            printf '%s' 'diagnostic output' >&2
            """
        )
        defer { try? FileManager.default.removeItem(at: scriptURL) }

        let books = try await RustCLIClient(executableURL: scriptURL).listBooks()

        XCTAssertEqual(books.first?.id, "asset-1")
    }

    func testListBooksUsesMachineJSONAndMapsStableAssetIdentity() async throws {
        let capture = CommandCapture()
        let client = makeClient(capture: capture) { _ in
            .success(#"{"schema_version":1,"books":[{"asset_id":"asset-1","title":"Test Book","author":"Test Author","note_count":3}]}"#)
        }

        let books = try await client.listBooks()

        XCTAssertEqual(capture.arguments, [["list", "--json"]])
        XCTAssertEqual(books, [
            Book(
                id: "asset-1",
                title: "Test Book",
                author: "Test Author",
                totalAnnotations: 3,
                highlightsCount: 0,
                notesCount: 0
            )
        ])
    }

    func testAnnotationsUseAssetIDAndPreserveNullableMachineFields() async throws {
        let capture = CommandCapture()
        let client = makeClient(capture: capture) { _ in
            .success(#"{"schema_version":1,"asset_id":"asset-1","title":"Test Book","author":"Test Author","annotation_count":1,"annotations":[{"id":"annotation-1","type":"note","content_text":"highlight","note_text":"remember this","chapter_title":null,"location":"epubcfi(/6/2)","created_at":"2026-01-02T03:04:05Z"}]}"#)
        }

        let annotations = try await client.annotations(for: "asset-1")

        XCTAssertEqual(capture.arguments, [["annotations", "--asset-id", "asset-1", "--json"]])
        XCTAssertEqual(annotations.count, 1)
        XCTAssertEqual(annotations[0].id, "annotation-1")
        XCTAssertEqual(annotations[0].type, .note)
        XCTAssertEqual(annotations[0].contentText, "highlight")
        XCTAssertEqual(annotations[0].noteText, "remember this")
        XCTAssertEqual(annotations[0].chapterTitle, "")
        XCTAssertEqual(annotations[0].locationInfo, "epubcfi(/6/2)")
        XCTAssertEqual(annotations[0].createdAt, ISO8601DateFormatter().date(from: "2026-01-02T03:04:05Z"))
    }

    func testExportReturnsReceiptAndUsesExplicitOutputDirectory() async throws {
        let capture = CommandCapture()
        let client = makeClient(capture: capture) { _ in
            .success(#"{"schema_version":1,"receipt":{"asset_id":"asset-1","title":"Test Book","annotation_count":2,"format":"obsidian","output_directory":"/tmp/books","generated_files":["/tmp/books/Test Book.md"]}}"#)
        }
        let outputURL = URL(fileURLWithPath: "/tmp/books")

        let receipt = try await client.export(
            assetID: "asset-1",
            outputDirectory: outputURL,
            format: "obsidian",
            overwrite: false
        )

        XCTAssertEqual(
            capture.arguments,
            [[
                "export", "--asset-id", "asset-1", "--json",
                "--output", "/tmp/books", "--format", "obsidian"
            ]]
        )
        XCTAssertEqual(receipt.assetID, "asset-1")
        XCTAssertEqual(receipt.annotationCount, 2)
        XCTAssertEqual(receipt.generatedFiles, ["/tmp/books/Test Book.md"])
    }

    func testUnsupportedSchemaVersionFailsExplicitly() async {
        let client = makeClient(capture: CommandCapture()) { _ in
            .success(#"{"schema_version":2,"books":[]}"#)
        }

        do {
            _ = try await client.listBooks()
            XCTFail("expected unsupported schema error")
        } catch let error as RustCLIError {
            XCTAssertEqual(error.stableCode, "UNSUPPORTED_SCHEMA_VERSION")
        } catch {
            XCTFail("unexpected error: \(error)")
        }
    }

    func testNonzeroExitPreservesStableErrorAndRemediationFromStderr() async {
        let client = makeClient(capture: CommandCapture()) { _ in
            .failure(
                status: 1,
                stderr: #"{"schema_version":1,"error":{"code":"FULL_DISK_ACCESS_REQUIRED","message":"Permission denied","remediation":"Grant Full Disk Access and retry."}}"#
            )
        }

        do {
            _ = try await client.listBooks()
            XCTFail("expected command failure")
        } catch let error as RustCLIError {
            XCTAssertEqual(error.stableCode, "FULL_DISK_ACCESS_REQUIRED")
            XCTAssertEqual(error.remediation, "Grant Full Disk Access and retry.")
            XCTAssertTrue(error.localizedDescription.contains("Permission denied"))
            XCTAssertTrue(error.userFacingDescription.contains("FULL_DISK_ACCESS_REQUIRED"))
            XCTAssertTrue(error.userFacingDescription.contains("Grant Full Disk Access"))
        } catch {
            XCTFail("unexpected error: \(error)")
        }
    }

    func testNonzeroExitWithoutStructuredErrorIsNotSilentlyAccepted() async {
        let client = makeClient(capture: CommandCapture()) { _ in
            .failure(status: 2, stderr: "plain stderr")
        }

        do {
            _ = try await client.listBooks()
            XCTFail("expected command failure")
        } catch let error as RustCLIError {
            XCTAssertEqual(error.stableCode, "BACKEND_UNAVAILABLE")
        } catch {
            XCTFail("unexpected error: \(error)")
        }
    }

    private func makeClient(
        capture: CommandCapture,
        result: @escaping @Sendable ([String]) -> RustCLICommandResult
    ) -> RustCLIClient {
        RustCLIClient(
            executableURL: URL(fileURLWithPath: "/tmp/apple-books-exporter"),
            runner: { _, arguments in
                capture.record(arguments)
                return result(arguments)
            }
        )
    }

    private func makeExecutableScript(_ contents: String) throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("books-exporter-rust-cli-" + UUID().uuidString)
        try Data(contents.utf8).write(to: url)
        try FileManager.default.setAttributes(
            [.posixPermissions: NSNumber(value: 0o755)],
            ofItemAtPath: url.path
        )
        return url
    }
}

private final class CommandCapture: @unchecked Sendable {
    private let lock = NSLock()
    private var recordedArguments: [[String]] = []

    var arguments: [[String]] {
        lock.lock()
        defer { lock.unlock() }
        return recordedArguments
    }

    func record(_ arguments: [String]) {
        lock.lock()
        recordedArguments.append(arguments)
        lock.unlock()
    }
}
