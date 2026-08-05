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

    func testDefaultCardUsesAutomaticTypographyWithReadableAlignmentDefaults() {
        let card = ShareCardService().makeCard(
            for: makeBook(author: "A. Reader"),
            annotation: makeAnnotation(content: "A short passage.", note: nil)
        )

        XCTAssertEqual(card.typography.font, .system)
        XCTAssertEqual(card.typography.sizeMode, .automatic)
        XCTAssertEqual(card.typography.horizontalAlignment, .left)
        XCTAssertEqual(card.typography.verticalAlignment, .center)
        XCTAssertEqual(card.font, card.typography.font)

        let unreadableFixedSize = ShareCardTypography(sizeMode: .fixed, fontSize: 12)
        XCTAssertEqual(unreadableFixedSize.fontSize, ShareCardTypography.minimumReadableFontSize)
        let oversizedFixedSize = ShareCardTypography(sizeMode: .fixed, fontSize: 999)
        XCTAssertEqual(oversizedFixedSize.fontSize, ShareCardTypography.maximumReadableFontSize)
        let manualPage = ShareCardPage(index: 0, primaryText: "text", supplementaryNote: nil, fontSize: 12)
        XCTAssertEqual(manualPage.fontSize, ShareCardTypography.minimumReadableFontSize)
    }

    func testFixedTypographyKeepsReadableSizeAndPaginatesMixedLanguageWithoutTruncation() throws {
        let text = String(repeating: "中英文 mixed passage，必须完整保留。 ", count: 220)
        let typography = ShareCardTypography(
            font: .sourceHanSansSC,
            sizeMode: .fixed,
            fontSize: 64,
            horizontalAlignment: .center,
            verticalAlignment: .bottom
        )
        let service = ShareCardService()
        let card = service.makeCard(
            for: makeBook(author: "A. Reader"),
            annotation: makeAnnotation(content: text, note: nil),
            typography: typography
        )

        let pages = service.pages(for: card)

        XCTAssertGreaterThan(pages.count, 1)
        XCTAssertTrue(pages.allSatisfy { $0.fontSize == 64 })
        XCTAssertEqual(pages.map(\.primaryText).joined(), text)
        XCTAssertTrue(pages.allSatisfy { page in
            card.template.textSafeArea.contains(page.primaryTextFrame)
        })

        let preview = try service.previewImage(for: card, pageIndex: pages.count - 1)
        XCTAssertEqual(preview.size, ShareCardService.canvasSize)
    }

    func testTypographyAlignmentChangesRenderedOutputAndVerticalFrame() throws {
        let service = ShareCardService()
        let baseArguments = (
            book: makeBook(author: "A. Reader"),
            annotation: makeAnnotation(content: "Alignment check", note: nil)
        )
        let topLeft = ShareCardTypography(
            font: .system,
            sizeMode: .fixed,
            fontSize: 64,
            horizontalAlignment: .left,
            verticalAlignment: .top
        )
        let bottomRight = ShareCardTypography(
            font: .system,
            sizeMode: .fixed,
            fontSize: 64,
            horizontalAlignment: .right,
            verticalAlignment: .bottom
        )

        let topCard = service.makeCard(
            for: baseArguments.book,
            annotation: baseArguments.annotation,
            typography: topLeft
        )
        let bottomCard = service.makeCard(
            for: baseArguments.book,
            annotation: baseArguments.annotation,
            typography: bottomRight
        )
        let topPage = try XCTUnwrap(service.pages(for: topCard).first)
        let bottomPage = try XCTUnwrap(service.pages(for: bottomCard).first)
        let topImage = try service.previewImage(for: topCard)
        let bottomImage = try service.previewImage(for: bottomCard)

        XCTAssertEqual(topPage.primaryTextFrame.maxY, topCard.template.textSafeArea.maxY, accuracy: 1)
        XCTAssertEqual(bottomPage.primaryTextFrame.minY, bottomCard.template.textSafeArea.minY, accuracy: 1)
        XCTAssertNotEqual(topImage.tiffRepresentation, bottomImage.tiffRepresentation)
    }

    func testPageLayoutKeepsSupplementaryNoteAndAttributionSeparatedFromPrimaryText() throws {
        let service = ShareCardService()
        let card = service.makeCard(
            for: makeBook(author: "A. Reader"),
            annotation: makeAnnotation(content: "A highlighted passage.", note: "A smaller supplementary note."),
            includeNote: true
        )

        let page = try XCTUnwrap(service.pages(for: card).first)
        let noteFrame = try XCTUnwrap(page.supplementaryNoteFrame)

        XCTAssertFalse(page.primaryTextFrame.intersects(noteFrame))
        XCTAssertFalse(noteFrame.intersects(page.attributionFrame))
        XCTAssertEqual(
            page.supplementaryNoteFontSize,
            ShareCardService.supplementaryNoteFontSize(for: page.fontSize)
        )
        XCTAssertEqual(card.template.primaryTextColor, card.template.supplementaryNoteColor)
    }

    func testLongSupplementaryNotePaginatesWithoutTruncation() {
        let note = String(repeating: "这是一段必须完整保留的补充笔记。 ", count: 80)
        let typography = ShareCardTypography(sizeMode: .fixed, fontSize: 42)
        let card = ShareCardService().makeCard(
            for: makeBook(author: "A. Reader"),
            annotation: makeAnnotation(content: "A highlighted passage.", note: note),
            includeNote: true,
            typography: typography
        )

        let pages = ShareCardService().pages(for: card)

        XCTAssertEqual(pages.compactMap(\.supplementaryNote).joined(), note)
        XCTAssertGreaterThan(pages.count, 2)
        XCTAssertEqual(pages.first?.supplementaryNoteFontSize, 30)
        XCTAssertTrue(pages.first?.supplementaryNoteFrame.map(card.template.noteArea.contains) ?? false)
        XCTAssertTrue(pages.dropFirst().compactMap(\.supplementaryNoteFrame).allSatisfy {
            card.template.textSafeArea.contains($0)
        })
    }

    func testSupplementaryNoteContinuationUsesTheFullTextSafeArea() throws {
        let note = String(repeating: "续页笔记应使用完整正文区域。 ", count: 120)
        let service = ShareCardService()
        let card = service.makeCard(
            for: makeBook(author: "A. Reader"),
            annotation: makeAnnotation(content: "正文", note: note),
            includeNote: true,
            typography: ShareCardTypography(sizeMode: .fixed, fontSize: 42)
        )

        let notePages = service.pages(for: card).filter { $0.supplementaryNote != nil }
        let firstNote = try XCTUnwrap(notePages.first?.supplementaryNote)
        let firstContinuation = try XCTUnwrap(notePages.dropFirst().first?.supplementaryNote)

        XCTAssertEqual(notePages.first?.primaryText, "正文")
        XCTAssertGreaterThan(firstContinuation.utf16.count, firstNote.utf16.count * 3)
        XCTAssertTrue(notePages.dropFirst().allSatisfy {
            card.template.textSafeArea.contains($0.supplementaryNoteFrame ?? .zero)
        })
    }

    func testSupplementaryNoteFontSizeScalesWithPrimaryFontSize() {
        let card = ShareCardService().makeCard(
            for: makeBook(author: "A. Reader"),
            annotation: makeAnnotation(content: "A highlighted passage.", note: "A proportional note."),
            includeNote: true,
            typography: ShareCardTypography(sizeMode: .fixed, fontSize: 72)
        )

        let page = ShareCardService().pages(for: card)[0]

        XCTAssertEqual(page.supplementaryNoteFontSize, 72 * ShareCardService.supplementaryNoteScale)
        XCTAssertEqual(
            ShareCardService.supplementaryNoteFontSize(for: 999),
            ShareCardService.maximumReadableFontSize * ShareCardService.supplementaryNoteScale
        )
    }

    func testAutomaticTypographyPaginatesAfterReachingMinimumReadableSize() {
        let text = String(repeating: "自动字号达到下限后仍然分页。 ", count: 260)
        let card = ShareCardService().makeCard(
            for: makeBook(author: "A. Reader"),
            annotation: makeAnnotation(content: text, note: nil),
            typography: ShareCardTypography(sizeMode: .automatic)
        )

        let pages = ShareCardService().pages(for: card)

        XCTAssertGreaterThan(pages.count, 1)
        XCTAssertTrue(pages.allSatisfy { $0.fontSize == ShareCardService.minimumReadableFontSize })
        XCTAssertEqual(pages.map(\.primaryText).joined(), text)
    }

    func testLineHeightAndMixedLanguagePaginationAreStableAcrossFonts() {
        let text = String(repeating: "中文 English 123，行高和分页必须保持一致。 ", count: 80)
        let service = ShareCardService()

        XCTAssertEqual(ShareCardService.lineHeightMultiple, 1.4)
        let lineTypography = ShareCardTypography(sizeMode: .fixed, fontSize: 42)
        let oneLineCard = service.makeCard(
            for: makeBook(author: "A. Reader"),
            annotation: makeAnnotation(content: "one line", note: nil),
            typography: lineTypography
        )
        let twoLineCard = service.makeCard(
            for: makeBook(author: "A. Reader"),
            annotation: makeAnnotation(content: "one line\ntwo line", note: nil),
            typography: lineTypography
        )
        let oneLineHeight = service.pages(for: oneLineCard)[0].primaryTextFrame.height
        let twoLineHeight = service.pages(for: twoLineCard)[0].primaryTextFrame.height
        XCTAssertEqual(twoLineHeight - oneLineHeight, 42 * ShareCardService.lineHeightMultiple, accuracy: 2)

        for font in ShareCardFont.allCases {
            let card = service.makeCard(
                for: makeBook(author: "A. Reader"),
                annotation: makeAnnotation(content: text, note: nil),
                typography: ShareCardTypography(
                    font: font,
                    sizeMode: .fixed,
                    fontSize: 42,
                    horizontalAlignment: .center,
                    verticalAlignment: .center
                )
            )

            let pages = service.pages(for: card)

            XCTAssertFalse(pages.isEmpty, "字体没有生成页面: \(font.rawValue)")
            XCTAssertEqual(pages.map(\.primaryText).joined(), text, "字体分页截断: \(font.rawValue)")
            XCTAssertTrue(pages.allSatisfy { page in
                card.template.textSafeArea.contains(page.primaryTextFrame)
            }, "字体正文越过安全区: \(font.rawValue)")
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

            let image: NSImage
            do {
                image = try service.previewImage(for: card)
            } catch {
                XCTFail("字体渲染失败: \(font.rawValue), \(error)")
                continue
            }
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

    func testRenderAndTemporaryPageURLUseTheSelectedPageContract() throws {
        let service = ShareCardService()
        let card = service.makeCard(
            for: makeBook(author: "A. Reader"),
            annotation: makeAnnotation(
                content: String(repeating: "A page that must remain available. ", count: 180),
                note: nil
            )
        )

        let renderedPages = try service.render(for: card)
        XCTAssertGreaterThan(renderedPages.count, 1)
        XCTAssertEqual(renderedPages.map(\.page.primaryText).joined(), card.primaryText)
        XCTAssertTrue(renderedPages.allSatisfy { $0.image.size == ShareCardService.canvasSize })

        let pageURL = try service.temporaryPNGURL(for: card, pageIndex: 1)
        defer { try? FileManager.default.removeItem(at: pageURL.deletingLastPathComponent()) }
        let bitmap = try XCTUnwrap(NSBitmapImageRep(data: Data(contentsOf: pageURL)))
        XCTAssertEqual(bitmap.pixelsWide, 1200)
        XCTAssertEqual(bitmap.pixelsHigh, 1600)

        let directoryURL = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: directoryURL) }
        let export = try service.export(card, pages: renderedPages.map(\.page), to: directoryURL)
        XCTAssertEqual(export.pageCount, renderedPages.count)
        XCTAssertEqual(export.files.count, renderedPages.count)
    }

    func testRenderingRejectsTextThatCannotFitItsSafeArea() {
        let template = ShareCardTemplate(
            id: "tiny-safe-area",
            theme: .mistWash,
            textSafeArea: CGRect(x: 0, y: 0, width: 1, height: 1)
        )
        let card = ShareCard(
            bookTitle: "The Quiet Book",
            author: "A. Reader",
            primaryText: "This text cannot fit.",
            supplementaryNote: nil,
            template: template,
            typography: ShareCardTypography(sizeMode: .fixed, fontSize: 72)
        )

        XCTAssertThrowsError(try ShareCardService().previewImage(for: card)) { error in
            guard case ShareCardExportError.textRenderingFailed = error else {
                return XCTFail("unexpected error: \(error)")
            }
        }
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
        for file in result.files {
            let bitmap = try XCTUnwrap(NSBitmapImageRep(data: Data(contentsOf: file)))
            XCTAssertEqual(bitmap.pixelsWide, 1200)
            XCTAssertEqual(bitmap.pixelsHigh, 1600)
        }
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

    func testAlternativeCardsUseFourCompleteTemplatesAndTwelveCuratedThemesExist() {
        let service = ShareCardService()
        let card = service.makeCard(
            for: makeBook(author: "A. Reader"),
            annotation: makeAnnotation(content: "A short passage.", note: nil)
        )

        XCTAssertEqual(ShareCardTheme.allCases.count, 12)
        let alternatives = service.alternativeCards(for: card)

        XCTAssertEqual(alternatives.count, 4)
        XCTAssertEqual(Set(alternatives.map { $0.template.id }).count, 4)
        XCTAssertTrue(alternatives.allSatisfy { $0.primaryText == card.primaryText })
        XCTAssertTrue(alternatives.allSatisfy { $0.attributionText == card.attributionText })
        XCTAssertEqual(Set(alternatives.map { $0.theme }).count, 4)
    }

    func testAllThemesRenderBundledBackgroundsAndReadableTextContrast() throws {
        let service = ShareCardService()
        let card = service.makeCard(
            for: makeBook(author: "A. Reader"),
            annotation: makeAnnotation(content: "一段用于检查主题背景和文字对比度的书摘。", note: nil)
        )

        for theme in ShareCardTheme.allCases {
            let template = service.template(for: theme)
            let background = try XCTUnwrap(service.backgroundImage(for: theme), "缺少主题背景: \(theme.rawValue)")
            let bitmap = try XCTUnwrap(
                background.representations.compactMap { $0 as? NSBitmapImageRep }.first,
                "主题背景无法读取: \(theme.rawValue)"
            )
            XCTAssertEqual(bitmap.pixelsWide * 4, bitmap.pixelsHigh * 3, "主题背景不是 3:4: \(theme.rawValue)")

            let themedCard = service.makeCard(
                for: makeBook(author: "A. Reader"),
                annotation: makeAnnotation(content: card.primaryText, note: nil),
                theme: theme
            )
            let image = try service.previewImage(for: themedCard)
            XCTAssertEqual(image.size, ShareCardService.canvasSize)

            let regions = [
                (name: "正文", area: template.textSafeArea, color: template.primaryTextColor),
                (name: "笔记", area: template.noteArea, color: template.supplementaryNoteColor),
                (name: "署名", area: template.attributionArea, color: template.attributionColor)
            ]
            for region in regions {
                let luminanceValues = luminances(
                    in: background,
                    area: region.area,
                    sampleStride: 12
                )
                XCTAssertFalse(
                    luminanceValues.isEmpty,
                    "主题\(region.name)安全区没有可采样像素: \(theme.rawValue)"
                )
                let contrast = minimumContrast(
                    in: background,
                    area: region.area,
                    foreground: region.color,
                    sampleStride: 1
                )
                XCTAssertGreaterThanOrEqual(
                    contrast,
                    4.5,
                    "主题\(region.name)文字对比度不足: \(theme.rawValue), \(contrast)"
                )
            }
        }
    }

    func testEveryTemplateProtectsAllRegionsForLongMixedContent() throws {
        let service = ShareCardService()
        let canvas = CGRect(origin: .zero, size: ShareCardService.canvasSize)
        let primaryText = String(repeating: "中文 English 123，模板安全区必须保持完整。 ", count: 90)
        let noteText = String(repeating: "笔记内容也必须完整保留。 ", count: 24)
        let typography = ShareCardTypography(
            sizeMode: .fixed,
            fontSize: 48,
            horizontalAlignment: .left,
            verticalAlignment: .center
        )

        for theme in ShareCardTheme.allCases {
            let template = service.template(for: theme)
            XCTAssertTrue(canvas.contains(template.textSafeArea), "正文安全区越界: \(theme.rawValue)")
            XCTAssertTrue(canvas.contains(template.noteArea), "笔记安全区越界: \(theme.rawValue)")
            XCTAssertTrue(canvas.contains(template.attributionArea), "署名安全区越界: \(theme.rawValue)")
            XCTAssertFalse(template.textSafeArea.intersects(template.noteArea), "正文和笔记区域重叠: \(theme.rawValue)")
            XCTAssertFalse(template.textSafeArea.intersects(template.attributionArea), "正文和署名区域重叠: \(theme.rawValue)")
            XCTAssertFalse(template.noteArea.intersects(template.attributionArea), "笔记和署名区域重叠: \(theme.rawValue)")

            let card = service.makeCard(
                for: makeBook(author: "A. Reader"),
                annotation: makeAnnotation(content: primaryText, note: noteText),
                includeNote: true,
                theme: theme,
                typography: typography
            )
            let pages = service.pages(for: card)

            XCTAssertGreaterThan(pages.count, 1, "长内容没有分页: \(theme.rawValue)")
            XCTAssertEqual(pages.map(\.primaryText).joined(), primaryText, "正文被截断: \(theme.rawValue)")
            XCTAssertEqual(pages.compactMap(\.supplementaryNote).joined(), noteText, "笔记被截断: \(theme.rawValue)")
            XCTAssertTrue(pages.allSatisfy { page in
                let primaryIsSafe = page.primaryText.isEmpty || template.textSafeArea.contains(page.primaryTextFrame)
                let noteIsSafe = page.supplementaryNoteFrame.map { frame in
                    let area = page.primaryText.isEmpty ? template.textSafeArea : template.noteArea
                    return area.contains(frame) && !frame.intersects(template.attributionArea)
                } ?? true
                return primaryIsSafe && noteIsSafe && template.attributionArea.contains(page.attributionFrame)
            }, "文字超出模板安全区: \(theme.rawValue)")

            let renderedPages = try service.render(for: card)
            XCTAssertEqual(renderedPages.count, pages.count, "渲染页数不一致: \(theme.rawValue)")
            XCTAssertTrue(
                renderedPages.allSatisfy { $0.image.size == ShareCardService.canvasSize },
                "渲染画布尺寸不一致: \(theme.rawValue)"
            )
        }
    }

    func testAlternativeCardsUsePersistentFourStepRotationAndSkipCurrentTheme() {
        let service = ShareCardService()
        let card = service.makeCard(
            for: makeBook(author: "A. Reader"),
            annotation: makeAnnotation(content: "A short passage.", note: nil)
        )

        let firstCards = service.alternativeCards(for: card)
        let first = firstCards.map(\.theme)
        let second = service.alternativeCards(for: card).map(\.theme)
        let afterSelection = service.alternativeCards(for: firstCards[0]).map(\.theme)

        XCTAssertEqual(first, [.sageLeaf, .blushArcs, .sandContours, .lavenderStars])
        XCTAssertEqual(second, [.stoneTextile, .vintagePaper, .pinkBlueWash, .softStone])
        XCTAssertFalse(first.contains(card.theme))
        XCTAssertEqual(afterSelection, [.linePaper, .ruledNote, .collagePaper, .mistWash])
    }

    func testAlternativeTemplatesPreserveAllTypographyChoices() {
        let typography = ShareCardTypography(
            font: .sourceHanSerifSC,
            sizeMode: .fixed,
            fontSize: 64,
            horizontalAlignment: .right,
            verticalAlignment: .top
        )
        let service = ShareCardService()
        let card = service.makeCard(
            for: makeBook(author: "A. Reader"),
            annotation: makeAnnotation(content: "A short passage.", note: nil),
            typography: typography
        )

        XCTAssertTrue(service.alternativeCards(for: card).allSatisfy { $0.typography == typography })
    }

    private func makeTemporaryDirectory() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("books-exporter-share-card-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    private func luminances(in image: NSImage, area: CGRect, sampleStride: Int) -> [CGFloat] {
        guard let bitmap = image.representations.compactMap({ $0 as? NSBitmapImageRep }).first else {
            return []
        }
        let xStart = max(0, Int(CGFloat(bitmap.pixelsWide) * area.minX / ShareCardService.canvasSize.width))
        let xEnd = min(bitmap.pixelsWide, Int(CGFloat(bitmap.pixelsWide) * area.maxX / ShareCardService.canvasSize.width))
        let yStart = max(0, Int(CGFloat(bitmap.pixelsHigh) * area.minY / ShareCardService.canvasSize.height))
        let yEnd = min(bitmap.pixelsHigh, Int(CGFloat(bitmap.pixelsHigh) * area.maxY / ShareCardService.canvasSize.height))

        var values: [CGFloat] = []
        for y in stride(from: yStart, to: yEnd, by: sampleStride) {
            for x in stride(from: xStart, to: xEnd, by: sampleStride) {
                guard let color = bitmap.colorAt(x: x, y: y) else { continue }
                values.append(relativeLuminance(color))
            }
        }
        return values
    }

    private func minimumContrast(
        in image: NSImage,
        area: CGRect,
        foreground: ShareCardColor,
        sampleStride: Int
    ) -> CGFloat {
        guard let bitmap = image.representations.compactMap({ $0 as? NSBitmapImageRep }).first else {
            return 0
        }
        let xStart = max(0, Int(CGFloat(bitmap.pixelsWide) * area.minX / ShareCardService.canvasSize.width))
        let xEnd = min(bitmap.pixelsWide, Int(CGFloat(bitmap.pixelsWide) * area.maxX / ShareCardService.canvasSize.width))
        let yStart = max(0, Int(CGFloat(bitmap.pixelsHigh) * area.minY / ShareCardService.canvasSize.height))
        let yEnd = min(bitmap.pixelsHigh, Int(CGFloat(bitmap.pixelsHigh) * area.maxY / ShareCardService.canvasSize.height))

        var minimum = CGFloat.greatestFiniteMagnitude
        for y in stride(from: yStart, to: yEnd, by: sampleStride) {
            for x in stride(from: xStart, to: xEnd, by: sampleStride) {
                guard let background = bitmap.colorAt(x: x, y: y),
                      let rgb = background.usingColorSpace(.sRGB) else {
                    continue
                }
                var red: CGFloat = 0
                var green: CGFloat = 0
                var blue: CGFloat = 0
                var alpha: CGFloat = 0
                rgb.getRed(&red, green: &green, blue: &blue, alpha: &alpha)
                let foregroundRed = foreground.red * foreground.alpha + red * (1 - foreground.alpha)
                let foregroundGreen = foreground.green * foreground.alpha + green * (1 - foreground.alpha)
                let foregroundBlue = foreground.blue * foreground.alpha + blue * (1 - foreground.alpha)
                let foregroundLuminance = relativeLuminance(
                    red: foregroundRed,
                    green: foregroundGreen,
                    blue: foregroundBlue
                )
                let backgroundLuminance = relativeLuminance(red: red, green: green, blue: blue)
                let contrast = (max(foregroundLuminance, backgroundLuminance) + 0.05)
                    / (min(foregroundLuminance, backgroundLuminance) + 0.05)
                minimum = min(minimum, contrast)
            }
        }
        return minimum == .greatestFiniteMagnitude ? 0 : minimum
    }

    private func relativeLuminance(_ color: ShareCardColor) -> CGFloat {
        let red = color.red
        let green = color.green
        let blue = color.blue
        return relativeLuminance(red: red, green: green, blue: blue)
    }

    private func relativeLuminance(_ color: NSColor) -> CGFloat {
        guard let rgb = color.usingColorSpace(.sRGB) else { return 1 }
        var red: CGFloat = 0
        var green: CGFloat = 0
        var blue: CGFloat = 0
        var alpha: CGFloat = 0
        rgb.getRed(&red, green: &green, blue: &blue, alpha: &alpha)
        return relativeLuminance(red: red, green: green, blue: blue)
    }

    private func relativeLuminance(red: CGFloat, green: CGFloat, blue: CGFloat) -> CGFloat {
        func linearize(_ value: CGFloat) -> CGFloat {
            value <= 0.03928 ? value / 12.92 : pow((value + 0.055) / 1.055, 2.4)
        }
        return 0.2126 * linearize(red) + 0.7152 * linearize(green) + 0.0722 * linearize(blue)
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
