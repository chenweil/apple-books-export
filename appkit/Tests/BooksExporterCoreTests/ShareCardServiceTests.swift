import AppKit
import CoreText
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

    func testBundledFontsRenderAndSurviveAlternativeCards() throws {
        let service = ShareCardService()
        let bundledFonts: [(ShareCardFont, String)] = [
            (.sourceHanSansSC, "SourceHanSansSC-Regular"),
            (.slideYouran, "slideyouran-Regular"),
            (.slideFu, "Slidefu-Regular"),
            (.sourceHanSerifSC, "SourceHanSerifSC-Regular"),
            (.zcoolWenYiTi, "zcoolwenyiti"),
            (.pangMenZhengDao, "PangMenZhengDao-Cu6.0"),
            (.huiwenMingChaoGBK, "Huiwen-MinchoGBK-Regular"),
            (.huiwenFangSong, "HuiwenFangsong-Regular"),
            (.huiwenZhengKai, "HuiwenZhengkai-Regular"),
            (.huiwenGangHei, "HuiwenHKHei-Regular"),
            (.lxgwWenKai, "LXGWWenKai-Regular")
        ]
        XCTAssertEqual(bundledFonts.map(\.0), ShareCardFont.allCases.filter { $0 != .system })

        let representativeText = "纳瓦尔宝典（硅谷投资人纳瓦尔十年人生智慧，教你如何获得财富与幸福。新时代创业者必读）我看重纳瓦尔，是因为他：对近乎一切都持怀疑态度；从第一性原理出发进行思考；可以对人对事进行有效测试；从不自我欺骗；不时调整自己的观点和看法；经常开怀大笑；有大局观；眼光长远；不把自己太当回事。"
        let characters = Array(representativeText.utf16)

        for (font, postScriptName) in bundledFonts {
            let card = service.makeCard(
                for: makeBook(author: "A. Reader"),
                annotation: makeAnnotation(content: "一段需要保留的中文书摘。", note: nil),
                font: font
            )

            XCTAssertEqual(card.font, font)
            XCTAssertEqual(service.pages(for: card).map(\.primaryText).joined(), card.primaryText)

            let image = try service.previewImage(for: card)
            XCTAssertEqual(image.size, ShareCardService.canvasSize)
            let registeredFont = try XCTUnwrap(NSFont(name: postScriptName, size: 42),
                                               "字体未注册: \(font.rawValue)")
            XCTAssertEqual(registeredFont.fontName, postScriptName)

            var glyphs = [CGGlyph](repeating: 0, count: characters.count)
            let rendersRepresentativeText = characters.withUnsafeBufferPointer { characterBuffer in
                glyphs.withUnsafeMutableBufferPointer { glyphBuffer in
                    CTFontGetGlyphsForCharacters(
                        registeredFont as CTFont,
                        characterBuffer.baseAddress!,
                        glyphBuffer.baseAddress!,
                        characters.count
                    )
                }
            }
            XCTAssertTrue(rendersRepresentativeText, "字体缺少 Share Card 长文本中的字形: \(font.rawValue)")

            let alternatives = service.alternativeCards(for: card)
            XCTAssertTrue(alternatives.allSatisfy { $0.font == font })
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

    func testLongAttributionWrapsAcrossFooterLines() throws {
        let book = Book(
            id: "book-long-attribution",
            title: "纳瓦尔宝典（硅谷投资人纳瓦尔十年人生智慧，教你如何获得财富与幸福。新时代创业者必读）",
            author: "长期思考与独立判断的阅读者",
            totalAnnotations: 1,
            highlightsCount: 1,
            notesCount: 0
        )
        let card = ShareCardService().makeCard(
            for: book,
            annotation: makeAnnotation(content: "A short passage.", note: nil)
        )
        let image = try ShareCardService().previewImage(for: card)
        let bitmap = try XCTUnwrap(image.representations.compactMap { $0 as? NSBitmapImageRep }.first)

        let footerRows = (1300..<1580).map { y in
            let darkPixels = (120..<1080).reduce(into: 0) { count, x in
                guard let color = bitmap.colorAt(x: x, y: y),
                      let converted = color.usingColorSpace(.deviceRGB) else { return }
                var red: CGFloat = 0
                var green: CGFloat = 0
                var blue: CGFloat = 0
                var alpha: CGFloat = 0
                converted.getRed(&red, green: &green, blue: &blue, alpha: &alpha)
                if alpha > 0.5 && red < 0.5 && green < 0.5 && blue < 0.5 {
                    count += 1
                }
            }
            return darkPixels > 80
        }
        var lineGroups = 0
        var inLine = false
        for hasDarkPixels in footerRows {
            if hasDarkPixels && !inLine {
                lineGroups += 1
            }
            inLine = hasDarkPixels
        }

        XCTAssertGreaterThanOrEqual(lineGroups, 2)
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
