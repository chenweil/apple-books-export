import AppKit
import XCTest
@testable import BooksExporterCore

final class ShareCardServiceTests: XCTestCase {
    func testHighlightCreatesReadableDefaultCardAndPNG() throws {
        let directoryURL = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: directoryURL) }

        let book = Book(
            id: "book-1",
            title: "The Quiet Book",
            author: "A. Reader",
            totalAnnotations: 1,
            highlightsCount: 1,
            notesCount: 0
        )
        let annotation = Annotation(
            id: "annotation-1",
            type: .highlight,
            chapterTitle: "Chapter One",
            locationInfo: "12",
            contentText: "A meaningful passage worth keeping.",
            noteText: nil,
            createdAt: Date(timeIntervalSinceReferenceDate: 0)
        )
        let service = ShareCardService()

        let card = service.makeCard(for: book, annotation: annotation)
        XCTAssertEqual(card.primaryText, "A meaningful passage worth keeping.")
        XCTAssertNil(card.supplementaryNote)
        XCTAssertEqual(card.attributionText, "The Quiet Book · A. Reader")

        let result = try service.export(card, to: directoryURL)
        XCTAssertEqual(result.files.count, 1)
        XCTAssertEqual(result.files[0].lastPathComponent, "The Quiet Book - A. Reader.png")

        let data = try Data(contentsOf: result.files[0])
        XCTAssertEqual(Array(data.prefix(8)), [137, 80, 78, 71, 13, 10, 26, 10])

        let bitmap = try XCTUnwrap(NSBitmapImageRep(data: data))
        XCTAssertEqual(bitmap.pixelsWide, 1200)
        XCTAssertEqual(bitmap.pixelsHigh, 1600)
        XCTAssertNotEqual(bitmap.colorAt(x: 0, y: 0), bitmap.colorAt(x: 600, y: 800))

        if let inspectionPath = ProcessInfo.processInfo.environment["SHARE_CARD_INSPECTION_DIR"] {
            let inspectionDirectory = URL(fileURLWithPath: inspectionPath, isDirectory: true)
            try FileManager.default.createDirectory(at: inspectionDirectory, withIntermediateDirectories: true)
            let inspectionURL = inspectionDirectory.appendingPathComponent("share-card.png")
            try? FileManager.default.removeItem(at: inspectionURL)
            try FileManager.default.copyItem(at: result.files[0], to: inspectionURL)
        }
    }

    func testNoteOnlyAnnotationUsesNoteAsPrimaryText() {
        let book = makeBook(author: "")
        let annotation = makeAnnotation(content: nil, note: "A note without a highlighted passage.")

        let card = ShareCardService().makeCard(for: book, annotation: annotation)

        XCTAssertEqual(card.primaryText, "A note without a highlighted passage.")
        XCTAssertNil(card.supplementaryNote)
        XCTAssertEqual(card.attributionText, "The Quiet Book")
    }

    func testAuthorlessBookUsesTitleOnlyForExportName() throws {
        let directoryURL = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: directoryURL) }

        let card = ShareCardService().makeCard(
            for: makeBook(author: ""),
            annotation: makeAnnotation(content: "A short passage.", note: nil)
        )

        let result = try ShareCardService().export(card, to: directoryURL)

        XCTAssertEqual(result.files.map { $0.lastPathComponent }, ["The Quiet Book.png"])
    }

    func testHighlightNoteIsOptionalAndCardEditDoesNotMutateAnnotation() {
        let book = makeBook(author: "A. Reader")
        let annotation = makeAnnotation(content: "The highlighted passage.", note: "My interpretation.")
        let service = ShareCardService()

        let defaultCard = service.makeCard(for: book, annotation: annotation)
        XCTAssertEqual(defaultCard.primaryText, "The highlighted passage.")
        XCTAssertNil(defaultCard.supplementaryNote)

        let editedCard = service.makeCard(
            for: book,
            annotation: annotation,
            includeNote: true,
            textOverride: "A shorter card edit."
        )
        XCTAssertEqual(editedCard.primaryText, "A shorter card edit.")
        XCTAssertEqual(editedCard.supplementaryNote, "My interpretation.")
        XCTAssertEqual(annotation.contentText, "The highlighted passage.")
        XCTAssertEqual(annotation.noteText, "My interpretation.")
    }

    func testLongTextUsesConsecutivePagesAtReadableMinimumWithoutTruncation() throws {
        let text = String(repeating: "A long passage that must remain intact. ", count: 260)
        let card = ShareCardService().makeCard(
            for: makeBook(author: "A. Reader"),
            annotation: makeAnnotation(content: text, note: nil)
        )
        let service = ShareCardService()

        let pages = service.pages(for: card)

        XCTAssertGreaterThan(pages.count, 1)
        XCTAssertTrue(pages.allSatisfy { $0.fontSize >= ShareCardService.minimumReadableFontSize })
        XCTAssertEqual(pages.map { $0.primaryText }.joined(), text)

        let directoryURL = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: directoryURL) }
        let result = try service.export(card, to: directoryURL)
        XCTAssertEqual(result.pageCount, pages.count)
        XCTAssertEqual(result.files.count, pages.count)
        XCTAssertTrue(result.files.allSatisfy { FileManager.default.fileExists(atPath: $0.path) })
        XCTAssertNotEqual(result.files.first?.lastPathComponent, result.files.last?.lastPathComponent)
    }

    func testExportFolderPreferenceIsOffByDefaultAndCanRevealAfterSaving() throws {
        let defaults = UserDefaults.standard
        let originalValue = defaults.object(forKey: ShareCardPreferences.openExportFolderKey)
        defer {
            if let originalValue {
                defaults.set(originalValue, forKey: ShareCardPreferences.openExportFolderKey)
            } else {
                defaults.removeObject(forKey: ShareCardPreferences.openExportFolderKey)
            }
        }

        let card = ShareCardService().makeCard(
            for: makeBook(author: "A. Reader"),
            annotation: makeAnnotation(content: "A short passage.", note: nil)
        )
        var revealedURL: URL?
        let service = ShareCardService(folderRevealer: { revealedURL = $0 })

        defaults.removeObject(forKey: ShareCardPreferences.openExportFolderKey)
        let firstDirectory = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: firstDirectory) }
        let firstResult = try service.export(card, to: firstDirectory)
        XCTAssertFalse(firstResult.didRevealExportFolder)
        XCTAssertNil(revealedURL)

        defaults.set(true, forKey: ShareCardPreferences.openExportFolderKey)
        let secondDirectory = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: secondDirectory) }
        let secondResult = try service.export(card, to: secondDirectory)
        XCTAssertTrue(secondResult.didRevealExportFolder)
        XCTAssertEqual(revealedURL, secondDirectory)
    }

    func testAlternativeCardsUseFourCompleteTemplatesAndSixCuratedThemesExist() {
        let service = ShareCardService()
        let card = service.makeCard(
            for: makeBook(author: "A. Reader"),
            annotation: makeAnnotation(content: "A short passage.", note: nil)
        )

        XCTAssertEqual(ShareCardTheme.allCases.count, 6)
        let alternatives = service.alternativeCards(for: card)

        XCTAssertEqual(alternatives.count, 4)
        XCTAssertEqual(Set(alternatives.map { $0.template.id }).count, 4)
        XCTAssertTrue(alternatives.allSatisfy { $0.primaryText == card.primaryText })
        XCTAssertTrue(alternatives.allSatisfy { $0.attributionText == card.attributionText })
        XCTAssertEqual(Set(alternatives.map { $0.theme }).count, 4)
    }

    private func makeTemporaryDirectory() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("books-exporter-share-card-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    private func makeBook(author: String) -> Book {
        Book(
            id: "book-1",
            title: "The Quiet Book",
            author: author,
            totalAnnotations: 1,
            highlightsCount: 1,
            notesCount: 1
        )
    }

    private func makeAnnotation(content: String?, note: String?) -> Annotation {
        Annotation(
            id: "annotation-1",
            type: content == nil ? .note : .highlight,
            chapterTitle: "Chapter One",
            locationInfo: "12",
            contentText: content,
            noteText: note,
            createdAt: Date(timeIntervalSinceReferenceDate: 0)
        )
    }
}
