import AppKit
import CoreText
import Foundation

public enum ShareCardTheme: String, CaseIterable, Codable, Hashable {
    case mistWash
    case sageLeaf
    case blushArcs
    case sandContours
    case lavenderStars
    case stoneTextile
    case vintagePaper
    case pinkBlueWash
    case softStone
    case linePaper
    case ruledNote
    case collagePaper

    public var displayName: String {
        switch self {
        case .mistWash: return "雾洗"
        case .sageLeaf: return "鼠尾草"
        case .blushArcs: return "腮红"
        case .sandContours: return "沙纹"
        case .lavenderStars: return "星点"
        case .stoneTextile: return "石纹"
        case .vintagePaper: return "复古纸"
        case .pinkBlueWash: return "粉蓝水彩"
        case .softStone: return "柔和彩石"
        case .linePaper: return "线稿纸"
        case .ruledNote: return "横线便签"
        case .collagePaper: return "拼贴纸"
        }
    }

    fileprivate var resourceName: String {
        switch self {
        case .mistWash: return "mist-wash"
        case .sageLeaf: return "sage-leaf"
        case .blushArcs: return "blush-arcs"
        case .sandContours: return "sand-contours"
        case .lavenderStars: return "lavender-stars"
        case .stoneTextile: return "stone-textile"
        case .vintagePaper: return "vintage-paper"
        case .pinkBlueWash: return "pink-blue-wash"
        case .softStone: return "soft-stone"
        case .linePaper: return "line-paper"
        case .ruledNote: return "ruled-note"
        case .collagePaper: return "collage-paper"
        }
    }

    fileprivate var fallbackColor: NSColor {
        switch self {
        case .mistWash: return NSColor(calibratedRed: 0.86, green: 0.89, blue: 0.88, alpha: 1)
        case .sageLeaf: return NSColor(calibratedRed: 0.79, green: 0.85, blue: 0.77, alpha: 1)
        case .blushArcs: return NSColor(calibratedRed: 0.93, green: 0.80, blue: 0.80, alpha: 1)
        case .sandContours: return NSColor(calibratedRed: 0.89, green: 0.83, blue: 0.71, alpha: 1)
        case .lavenderStars: return NSColor(calibratedRed: 0.83, green: 0.80, blue: 0.90, alpha: 1)
        case .stoneTextile: return NSColor(calibratedWhite: 0.78, alpha: 1)
        case .vintagePaper: return NSColor(calibratedRed: 0.86, green: 0.70, blue: 0.48, alpha: 1)
        case .pinkBlueWash: return NSColor(calibratedRed: 0.88, green: 0.86, blue: 0.90, alpha: 1)
        case .softStone: return NSColor(calibratedRed: 0.80, green: 0.82, blue: 0.81, alpha: 1)
        case .linePaper: return NSColor(calibratedWhite: 0.94, alpha: 1)
        case .ruledNote: return NSColor(calibratedRed: 0.94, green: 0.91, blue: 0.82, alpha: 1)
        case .collagePaper: return NSColor(calibratedRed: 0.86, green: 0.82, blue: 0.75, alpha: 1)
        }
    }
}

public enum ShareCardFont: String, CaseIterable, Codable {
    case system
    case sourceHanSansSC
    case slideYouran
    case slideFu
    case sourceHanSerifSC
    case zcoolWenYiTi
    case pangMenZhengDao
    case huiwenMingChaoGBK
    case huiwenFangSong
    case huiwenZhengKai
    case huiwenGangHei
    case lxgwWenKai

    public var displayName: String {
        switch self {
        case .system: return "系统默认"
        case .sourceHanSansSC: return "思源黑体"
        case .slideYouran: return "演示悠然小楷"
        case .slideFu: return "演示佛系体"
        case .sourceHanSerifSC: return "思源宋体"
        case .zcoolWenYiTi: return "站酷文艺体"
        case .pangMenZhengDao: return "庞门正道粗书体"
        case .huiwenMingChaoGBK: return "汇文明朝体"
        case .huiwenFangSong: return "汇文仿宋"
        case .huiwenZhengKai: return "汇文正楷"
        case .huiwenGangHei: return "汇文港黑"
        case .lxgwWenKai: return "霞鹜文楷"
        }
    }

    fileprivate var resourceName: String? {
        switch self {
        case .system: return nil
        case .sourceHanSansSC: return "SourceHanSansSC-Regular"
        case .slideYouran: return "SlideYouran-Regular"
        case .slideFu: return "Slidefu-Regular"
        case .sourceHanSerifSC: return "SourceHanSerifSC-Regular"
        case .zcoolWenYiTi: return "ZcoolWenYiTi"
        case .pangMenZhengDao: return "PangMenZhengDao-Cu6.0-Regular"
        case .huiwenMingChaoGBK: return "HuiwenMingChaoGBK"
        case .huiwenFangSong: return "HuiwenFangSong"
        case .huiwenZhengKai: return "HuiwenZhengKai"
        case .huiwenGangHei: return "HuiwenGangHei"
        case .lxgwWenKai: return "LXGWWenKai-Regular"
        }
    }

    fileprivate var resourceExtension: String? {
        switch self {
        case .system: return nil
        case .sourceHanSansSC: return "otf"
        case .slideYouran, .slideFu, .zcoolWenYiTi, .pangMenZhengDao,
             .huiwenMingChaoGBK, .huiwenFangSong, .huiwenZhengKai, .huiwenGangHei,
             .lxgwWenKai:
            return "ttf"
        case .sourceHanSerifSC: return "otf"
        }
    }

    fileprivate var postScriptName: String? {
        switch self {
        case .system: return nil
        case .sourceHanSansSC: return "SourceHanSansSC-Regular"
        case .slideYouran: return "slideyouran-Regular"
        case .slideFu: return "Slidefu-Regular"
        case .sourceHanSerifSC: return "SourceHanSerifSC-Regular"
        case .zcoolWenYiTi: return "zcoolwenyiti"
        case .pangMenZhengDao: return "PangMenZhengDao-Cu6.0"
        case .huiwenMingChaoGBK: return "Huiwen-MinchoGBK-Regular"
        case .huiwenFangSong: return "HuiwenFangsong-Regular"
        case .huiwenZhengKai: return "HuiwenZhengkai-Regular"
        case .huiwenGangHei: return "HuiwenHKHei-Regular"
        case .lxgwWenKai: return "LXGWWenKai-Regular"
        }
    }
}

public enum ShareCardFontSizeMode: String, CaseIterable, Codable {
    case automatic
    case fixed

    public var displayName: String {
        switch self {
        case .automatic: return "自动字号"
        case .fixed: return "固定字号"
        }
    }
}

public enum ShareCardHorizontalAlignment: String, CaseIterable, Codable {
    case left
    case center
    case right

    public var displayName: String {
        switch self {
        case .left: return "左对齐"
        case .center: return "居中"
        case .right: return "右对齐"
        }
    }
}

public enum ShareCardVerticalAlignment: String, CaseIterable, Codable {
    case top
    case center
    case bottom

    public var displayName: String {
        switch self {
        case .top: return "靠上"
        case .center: return "居中"
        case .bottom: return "靠下"
        }
    }
}

public struct ShareCardTypography: Equatable, Codable {
    public static let minimumReadableFontSize: CGFloat = 42
    public static let maximumReadableFontSize: CGFloat = 72

    public let font: ShareCardFont
    public let sizeMode: ShareCardFontSizeMode
    public let fontSize: CGFloat
    public let horizontalAlignment: ShareCardHorizontalAlignment
    public let verticalAlignment: ShareCardVerticalAlignment

    public init(
        font: ShareCardFont = .system,
        sizeMode: ShareCardFontSizeMode = .automatic,
        fontSize: CGFloat = 56,
        horizontalAlignment: ShareCardHorizontalAlignment = .left,
        verticalAlignment: ShareCardVerticalAlignment = .center
    ) {
        self.font = font
        self.sizeMode = sizeMode
        let normalizedFontSize = fontSize.isFinite ? fontSize : Self.minimumReadableFontSize
        self.fontSize = min(
            max(normalizedFontSize, Self.minimumReadableFontSize),
            Self.maximumReadableFontSize
        )
        self.horizontalAlignment = horizontalAlignment
        self.verticalAlignment = verticalAlignment
    }

    public static let `default` = ShareCardTypography()
}

public struct ShareCardColor: Equatable, Codable {
    public let red: CGFloat
    public let green: CGFloat
    public let blue: CGFloat
    public let alpha: CGFloat

    public init(red: CGFloat, green: CGFloat, blue: CGFloat, alpha: CGFloat = 1) {
        self.red = red
        self.green = green
        self.blue = blue
        self.alpha = alpha
    }

    fileprivate var nsColor: NSColor {
        NSColor(calibratedRed: red, green: green, blue: blue, alpha: alpha)
    }

    fileprivate func withAlpha(_ alpha: CGFloat) -> ShareCardColor {
        ShareCardColor(red: red, green: green, blue: blue, alpha: alpha)
    }

    public static let nearBlack = ShareCardColor(red: 0.12, green: 0.12, blue: 0.12)
}

public struct ShareCardTemplate: Equatable, Codable {
    public let id: String
    public let theme: ShareCardTheme
    public let textSafeArea: CGRect
    public let noteArea: CGRect
    public let attributionArea: CGRect
    public let primaryTextColor: ShareCardColor
    public let supplementaryNoteColor: ShareCardColor
    public let attributionColor: ShareCardColor

    public init(
        id: String,
        theme: ShareCardTheme,
        textSafeArea: CGRect = CGRect(x: 130, y: 500, width: 940, height: 800),
        noteArea: CGRect = CGRect(x: 130, y: 300, width: 940, height: 130),
        attributionArea: CGRect = CGRect(x: 130, y: 90, width: 940, height: 180),
        primaryTextColor: ShareCardColor = .nearBlack,
        supplementaryNoteColor: ShareCardColor? = nil,
        attributionColor: ShareCardColor = ShareCardColor(red: 0.18, green: 0.18, blue: 0.18)
    ) {
        self.id = id
        self.theme = theme
        self.textSafeArea = textSafeArea
        self.noteArea = noteArea
        self.attributionArea = attributionArea
        self.primaryTextColor = primaryTextColor
        self.supplementaryNoteColor = supplementaryNoteColor ?? primaryTextColor
        self.attributionColor = attributionColor
    }
}

public struct ShareCard: Equatable {
    public let bookTitle: String
    public let author: String?
    public let primaryText: String
    public let supplementaryNote: String?
    public let template: ShareCardTemplate
    public let typography: ShareCardTypography

    public init(
        bookTitle: String,
        author: String?,
        primaryText: String,
        supplementaryNote: String?,
        template: ShareCardTemplate,
        font: ShareCardFont = .system
    ) {
        self.init(
            bookTitle: bookTitle,
            author: author,
            primaryText: primaryText,
            supplementaryNote: supplementaryNote,
            template: template,
            typography: ShareCardTypography(font: font)
        )
    }

    public init(
        bookTitle: String,
        author: String?,
        primaryText: String,
        supplementaryNote: String?,
        template: ShareCardTemplate,
        typography: ShareCardTypography
    ) {
        self.bookTitle = bookTitle
        self.author = author
        self.primaryText = primaryText
        self.supplementaryNote = supplementaryNote
        self.template = template
        self.typography = typography
    }

    public var font: ShareCardFont {
        typography.font
    }

    public var theme: ShareCardTheme {
        template.theme
    }

    public var attributionText: String {
        guard let author, !author.isEmpty else { return bookTitle }
        return "\(bookTitle) · \(author)"
    }
}

public struct ShareCardPage: Equatable {
    public let index: Int
    public let primaryText: String
    public let supplementaryNote: String?
    public let fontSize: CGFloat
    public let primaryTextFrame: CGRect
    public let supplementaryNoteFrame: CGRect?
    public let supplementaryNoteFontSize: CGFloat
    public let attributionFrame: CGRect

    public init(
        index: Int,
        primaryText: String,
        supplementaryNote: String?,
        fontSize: CGFloat,
        primaryTextFrame: CGRect = .zero,
        supplementaryNoteFrame: CGRect? = nil,
        supplementaryNoteFontSize: CGFloat = 0,
        attributionFrame: CGRect = .zero
    ) {
        self.index = index
        self.primaryText = primaryText
        self.supplementaryNote = supplementaryNote
        let normalizedFontSize = fontSize.isFinite
            ? fontSize
            : ShareCardTypography.minimumReadableFontSize
        self.fontSize = min(
            max(normalizedFontSize, ShareCardTypography.minimumReadableFontSize),
            ShareCardTypography.maximumReadableFontSize
        )
        self.primaryTextFrame = primaryTextFrame
        self.supplementaryNoteFrame = supplementaryNoteFrame
        self.supplementaryNoteFontSize = supplementaryNoteFontSize
        self.attributionFrame = attributionFrame
    }
}

public struct ShareCardRenderedPage {
    public let page: ShareCardPage
    public let image: NSImage

    public init(page: ShareCardPage, image: NSImage) {
        self.page = page
        self.image = image
    }
}

public struct ShareCardExportResult: Equatable {
    public let files: [URL]
    public let pageCount: Int
    public let didRevealExportFolder: Bool

    public init(files: [URL], pageCount: Int, didRevealExportFolder: Bool) {
        self.files = files
        self.pageCount = pageCount
        self.didRevealExportFolder = didRevealExportFolder
    }
}

public enum ShareCardPreferences {
    public static let openExportFolderKey = "appkit.shareCard.openExportFolder"

    public static var openExportFolder: Bool {
        UserDefaults.standard.bool(forKey: openExportFolderKey)
    }
}

public enum ShareCardExportError: LocalizedError {
    case emptyCardText
    case imageEncodingFailed
    case textRenderingFailed

    public var errorDescription: String? {
        switch self {
        case .emptyCardText: return "卡片没有可导出的正文。"
        case .imageEncodingFailed: return "无法生成 PNG 图片。"
        case .textRenderingFailed: return "文字无法完整放入卡片安全区域。"
        }
    }
}

/// Public generation-and-export seam for AppKit Share Cards.
public final class ShareCardService {
    public static let canvasSize = CGSize(width: 1200, height: 1600)
    public static let minimumReadableFontSize = ShareCardTypography.minimumReadableFontSize
    public static let maximumReadableFontSize = ShareCardTypography.maximumReadableFontSize
    public static let lineHeightMultiple: CGFloat = 1.4
    public static let minimumSupplementaryNoteFontSize: CGFloat = 30
    public static let supplementaryNoteScale: CGFloat = 0.6

    public static func supplementaryNoteFontSize(for primaryFontSize: CGFloat) -> CGFloat {
        let boundedFontSize = primaryFontSize.isFinite
            ? primaryFontSize
            : minimumReadableFontSize
        let normalizedFontSize = min(
            max(boundedFontSize, minimumReadableFontSize),
            maximumReadableFontSize
        )
        return max(
            minimumSupplementaryNoteFontSize,
            normalizedFontSize * supplementaryNoteScale
        )
    }

    private static let fontRegistrationLock = NSLock()
    private static var registeredFontResources = Set<String>()

    private let fileManager: FileManager
    private let folderRevealer: (URL) -> Void
    private let alternativeCursorLock = NSLock()
    private var alternativeCursor = 0
    private let backgroundImageLock = NSLock()
    private var backgroundImageCache: [ShareCardTheme: NSImage] = [:]

    public init(
        fileManager: FileManager = .default,
        folderRevealer: @escaping (URL) -> Void = { folderURL in
            NSWorkspace.shared.activateFileViewerSelecting([folderURL])
        }
    ) {
        self.fileManager = fileManager
        self.folderRevealer = folderRevealer
    }

    public func makeCard(
        for book: Book,
        annotation: Annotation,
        includeNote: Bool = false,
        textOverride: String? = nil,
        theme: ShareCardTheme = .mistWash,
        font: ShareCardFont = .system
    ) -> ShareCard {
        makeCard(
            for: book,
            annotation: annotation,
            includeNote: includeNote,
            textOverride: textOverride,
            theme: theme,
            typography: ShareCardTypography(font: font)
        )
    }

    public func makeCard(
        for book: Book,
        annotation: Annotation,
        includeNote: Bool = false,
        textOverride: String? = nil,
        theme: ShareCardTheme = .mistWash,
        typography: ShareCardTypography
    ) -> ShareCard {
        let highlight = nonEmpty(annotation.contentText)
        let note = nonEmpty(annotation.noteText)
        let primaryText = textOverride ?? highlight ?? note ?? ""
        let author = nonEmpty(book.author)
        let supplementaryNote = highlight == nil || !includeNote ? nil : note

        return ShareCard(
            bookTitle: book.title,
            author: author,
            primaryText: primaryText,
            supplementaryNote: supplementaryNote,
            template: template(for: theme),
            typography: typography
        )
    }

    public func alternativeCards(for card: ShareCard) -> [ShareCard] {
        let themes = ShareCardTheme.allCases
        guard !themes.isEmpty else { return [] }

        alternativeCursorLock.lock()
        let cursor = alternativeCursor % themes.count
        alternativeCursor = (cursor + 4) % themes.count
        alternativeCursorLock.unlock()

        var candidateThemes: [ShareCardTheme] = []
        var offset = 1
        while candidateThemes.count < min(4, themes.count - 1) && offset < themes.count {
            let theme = themes[(cursor + offset) % themes.count]
            if theme != card.theme {
                candidateThemes.append(theme)
            }
            offset += 1
        }

        return candidateThemes.map { theme in
            ShareCard(
                bookTitle: card.bookTitle,
                author: card.author,
                primaryText: card.primaryText,
                supplementaryNote: card.supplementaryNote,
                template: template(for: theme),
                typography: card.typography
            )
        }
    }

    public func pages(for card: ShareCard) -> [ShareCardPage] {
        let text = card.primaryText
        guard !text.isEmpty else { return [] }

        let fontSize = fittingFontSize(for: text, card: card)
        let segments = split(text: text, card: card, fontSize: fontSize)
        var pageContents: [(primaryText: String, supplementaryNote: String?)] = segments.map {
            (primaryText: $0, supplementaryNote: nil)
        }
        if let note = card.supplementaryNote, !note.isEmpty {
            let noteFontSize = Self.supplementaryNoteFontSize(for: fontSize)
            let noteSegments = splitSupplementaryNote(
                text: note,
                card: card,
                fontSize: noteFontSize
            )
            if let firstNoteSegment = noteSegments.first {
                pageContents[pageContents.count - 1].supplementaryNote = firstNoteSegment
                for noteSegment in noteSegments.dropFirst() {
                    pageContents.append((primaryText: "", supplementaryNote: noteSegment))
                }
            }
        }

        return pageContents.enumerated().map { index, segment in
            let supplementaryNote = segment.supplementaryNote
            let supplementaryNoteArea = segment.primaryText.isEmpty
                ? card.template.textSafeArea
                : card.template.noteArea
            let supplementaryNoteFontSize = supplementaryNote.map { _ in
                Self.supplementaryNoteFontSize(for: fontSize)
            } ?? 0
            return ShareCardPage(
                index: index,
                primaryText: segment.primaryText,
                supplementaryNote: supplementaryNote,
                fontSize: fontSize,
                primaryTextFrame: segment.primaryText.isEmpty
                    ? .zero
                    : primaryTextFrame(for: segment.primaryText, card: card, fontSize: fontSize),
                supplementaryNoteFrame: supplementaryNote.map {
                    textFrame(
                        for: $0,
                        in: supplementaryNoteArea,
                        card: card,
                        fontSize: supplementaryNoteFontSize
                    )
                },
                supplementaryNoteFontSize: supplementaryNoteFontSize,
                attributionFrame: card.template.attributionArea
            )
        }
    }

    public func previewImage(for card: ShareCard, pageIndex: Int = 0) throws -> NSImage {
        let cardPages = pages(for: card)
        guard cardPages.indices.contains(pageIndex) else {
            throw ShareCardExportError.emptyCardText
        }
        let data = try pngData(for: card, page: cardPages[pageIndex])
        guard let image = NSImage(data: data) else {
            throw ShareCardExportError.imageEncodingFailed
        }
        return image
    }

    public func render(for card: ShareCard) throws -> [ShareCardRenderedPage] {
        let cardPages = pages(for: card)
        return try cardPages.map { page in
            let data = try pngData(for: card, page: page)
            guard let image = NSImage(data: data) else {
                throw ShareCardExportError.imageEncodingFailed
            }
            return ShareCardRenderedPage(page: page, image: image)
        }
    }

    public func temporaryPNGURL(for card: ShareCard, pageIndex: Int) throws -> URL {
        let cardPages = pages(for: card)
        guard cardPages.indices.contains(pageIndex) else {
            throw ShareCardExportError.emptyCardText
        }

        return try temporaryPNGURL(
            for: card,
            page: cardPages[pageIndex],
            pageCount: cardPages.count
        )
    }

    public func temporaryPNGURL(
        for card: ShareCard,
        page: ShareCardPage,
        pageCount: Int
    ) throws -> URL {
        guard pageCount > 0, page.index >= 0, page.index < pageCount else {
            throw ShareCardExportError.emptyCardText
        }

        let data = try pngData(for: card, page: page)
        return try writeTemporaryPNG(
            data,
            for: card,
            pageIndex: page.index,
            pageCount: pageCount
        )
    }

    public func temporaryPNGURL(
        for card: ShareCard,
        renderedPage: ShareCardRenderedPage,
        pageCount: Int
    ) throws -> URL {
        let page = renderedPage.page
        guard !card.primaryText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              pageCount > 0,
              page.index >= 0,
              page.index < pageCount else {
            throw ShareCardExportError.emptyCardText
        }

        let data = try pngData(for: renderedPage.image)
        return try writeTemporaryPNG(
            data,
            for: card,
            pageIndex: page.index,
            pageCount: pageCount
        )
    }

    public func export(
        _ card: ShareCard,
        to directoryURL: URL,
        openExportFolder: Bool = ShareCardPreferences.openExportFolder
    ) throws -> ShareCardExportResult {
        return try export(
            card,
            pages: pages(for: card),
            to: directoryURL,
            openExportFolder: openExportFolder
        )
    }

    public func export(
        _ card: ShareCard,
        pages: [ShareCardPage],
        to directoryURL: URL,
        openExportFolder: Bool = ShareCardPreferences.openExportFolder
    ) throws -> ShareCardExportResult {
        return try export(
            card,
            pageCount: pages.count,
            to: directoryURL,
            openExportFolder: openExportFolder
        ) { index in
            try pngData(for: card, page: pages[index])
        }
    }

    public func export(
        _ card: ShareCard,
        renderedPages: [ShareCardRenderedPage],
        to directoryURL: URL,
        openExportFolder: Bool = ShareCardPreferences.openExportFolder
    ) throws -> ShareCardExportResult {
        return try export(
            card,
            pageCount: renderedPages.count,
            to: directoryURL,
            openExportFolder: openExportFolder
        ) { index in
            try pngData(for: renderedPages[index].image)
        }
    }

    private func export(
        _ card: ShareCard,
        pageCount: Int,
        to directoryURL: URL,
        openExportFolder: Bool,
        dataForPage: (Int) throws -> Data
    ) throws -> ShareCardExportResult {
        guard !card.primaryText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              pageCount > 0 else {
            throw ShareCardExportError.emptyCardText
        }

        try fileManager.createDirectory(at: directoryURL, withIntermediateDirectories: true)
        let files = try (0..<pageCount).map { index in
            let fileURL = directoryURL.appendingPathComponent(
                fileName(for: card, pageIndex: index, pageCount: pageCount)
            )
            let data = try dataForPage(index)
            try data.write(to: fileURL, options: .atomic)
            return fileURL
        }

        if openExportFolder {
            folderRevealer(directoryURL)
        }

        return ShareCardExportResult(
            files: files,
            pageCount: pageCount,
            didRevealExportFolder: openExportFolder
        )
    }

    public func template(for theme: ShareCardTheme) -> ShareCardTemplate {
        makeTemplate(for: theme)
    }

    public func backgroundImage(for theme: ShareCardTheme) -> NSImage? {
        backgroundImageLock.lock()
        if let cached = backgroundImageCache[theme] {
            backgroundImageLock.unlock()
            return cached
        }
        backgroundImageLock.unlock()

        guard let backgroundURL = resourceBundle.url(
            forResource: theme.resourceName,
            withExtension: "png"
        ), let background = NSImage(contentsOf: backgroundURL) else {
            return nil
        }

        backgroundImageLock.lock()
        backgroundImageCache[theme] = background
        backgroundImageLock.unlock()
        return background
    }

    private func pngData(for card: ShareCard, page: ShareCardPage) throws -> Data {
        let width = Int(Self.canvasSize.width)
        let height = Int(Self.canvasSize.height)
        guard let bitmap = NSBitmapImageRep(
            bitmapDataPlanes: nil,
            pixelsWide: width,
            pixelsHigh: height,
            bitsPerSample: 8,
            samplesPerPixel: 4,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: .deviceRGB,
            bitmapFormat: [],
            bytesPerRow: 0,
            bitsPerPixel: 0
        ), let bitmapData = bitmap.bitmapData,
        let colorSpace = CGColorSpace(name: CGColorSpace.sRGB),
        let context = CGContext(
            data: bitmapData,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: bitmap.bytesPerRow,
            space: colorSpace,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else {
            throw ShareCardExportError.imageEncodingFailed
        }

        let graphicsContext = NSGraphicsContext(cgContext: context, flipped: false)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = graphicsContext
        defer { NSGraphicsContext.restoreGraphicsState() }

        card.theme.fallbackColor.setFill()
        NSRect(origin: .zero, size: Self.canvasSize).fill()

        if let background = backgroundImage(for: card.theme) {
            background.draw(
                in: NSRect(origin: .zero, size: Self.canvasSize),
                from: .zero,
                operation: .sourceOver,
                fraction: 1
            )
        }

        guard drawText(
            page.primaryText,
            in: page.primaryTextFrame,
            fontSize: page.fontSize,
            font: card.font,
            color: card.template.primaryTextColor,
            alignment: card.typography.horizontalAlignment,
            lineBreakMode: .byWordWrapping,
            context: context
        ) else {
            throw ShareCardExportError.textRenderingFailed
        }

        if let note = page.supplementaryNote {
            guard drawText(
                note,
                in: page.supplementaryNoteFrame ?? card.template.noteArea,
                fontSize: page.supplementaryNoteFontSize,
                font: card.font,
                color: card.template.supplementaryNoteColor,
                alignment: card.typography.horizontalAlignment,
                lineBreakMode: .byWordWrapping,
                context: context
            ) else {
                throw ShareCardExportError.textRenderingFailed
            }
        }

        guard drawText(
            card.attributionText,
            in: page.attributionFrame,
            fontSize: 30,
            font: card.font,
            color: card.template.attributionColor,
            alignment: .left,
            lineBreakMode: .byCharWrapping,
            weight: .medium,
            context: context
        ) else {
            throw ShareCardExportError.textRenderingFailed
        }

        guard let data = bitmap.representation(using: .png, properties: [:]) else {
            throw ShareCardExportError.imageEncodingFailed
        }
        return data
    }

    private func pngData(for image: NSImage) throws -> Data {
        let width = Int(Self.canvasSize.width)
        let height = Int(Self.canvasSize.height)
        guard let bitmap = NSBitmapImageRep(
            bitmapDataPlanes: nil,
            pixelsWide: width,
            pixelsHigh: height,
            bitsPerSample: 8,
            samplesPerPixel: 4,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: .deviceRGB,
            bitmapFormat: [],
            bytesPerRow: 0,
            bitsPerPixel: 0
        ) else {
            throw ShareCardExportError.imageEncodingFailed
        }

        let graphicsContext = NSGraphicsContext(bitmapImageRep: bitmap)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = graphicsContext
        defer { NSGraphicsContext.restoreGraphicsState() }

        NSColor.clear.setFill()
        NSRect(origin: .zero, size: Self.canvasSize).fill()
        graphicsContext?.imageInterpolation = .high
        image.draw(
            in: NSRect(origin: .zero, size: Self.canvasSize),
            from: NSRect(origin: .zero, size: image.size),
            operation: .copy,
            fraction: 1
        )

        guard let data = bitmap.representation(using: .png, properties: [:]) else {
            throw ShareCardExportError.imageEncodingFailed
        }
        return data
    }

    private func writeTemporaryPNG(
        _ data: Data,
        for card: ShareCard,
        pageIndex: Int,
        pageCount: Int
    ) throws -> URL {
        let directoryURL = fileManager.temporaryDirectory
            .appendingPathComponent("BooksExporter-ShareCard-\(UUID().uuidString)", isDirectory: true)
        try fileManager.createDirectory(at: directoryURL, withIntermediateDirectories: true)

        let fileURL = directoryURL.appendingPathComponent(
            fileName(for: card, pageIndex: pageIndex, pageCount: pageCount)
        )
        try data.write(to: fileURL, options: .atomic)
        return fileURL
    }

    private func drawText(
        _ text: String,
        in rect: CGRect,
        fontSize: CGFloat,
        font: ShareCardFont,
        color: ShareCardColor,
        alignment: ShareCardHorizontalAlignment,
        lineBreakMode: NSLineBreakMode,
        weight: NSFont.Weight = .regular,
        context: CGContext
    ) -> Bool {
        guard !text.isEmpty, !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return true
        }
        guard rect.width > 0, rect.height > 0 else { return false }

        let string = attributedText(
            text,
            fontSize: fontSize,
            font: font,
            alignment: alignment,
            color: color,
            weight: weight,
            lineBreakMode: lineBreakMode
        )
        let framesetter = CTFramesetterCreateWithAttributedString(string as CFAttributedString)
        let path = CGPath(rect: rect, transform: nil)
        let frame = CTFramesetterCreateFrame(
            framesetter,
            CFRange(location: 0, length: string.length),
            path,
            nil
        )
        let visibleRange = CTFrameGetVisibleStringRange(frame)
        guard visibleRange.location == 0, visibleRange.length == string.length else {
            return false
        }
        CTFrameDraw(frame, context)
        return true
    }

    private func fileName(for card: ShareCard, pageIndex: Int, pageCount: Int) -> String {
        let title = sanitized(card.bookTitle)
        let baseName: String
        if let author = card.author {
            baseName = "\(title) - \(sanitized(author))"
        } else {
            baseName = title
        }
        return pageCount == 1
            ? "\(baseName).png"
            : "\(baseName) \(pageIndex + 1).png"
    }

    private func sanitized(_ value: String) -> String {
        let invalidCharacters = CharacterSet(charactersIn: "/\\:*?\"<>|")
        let sanitized = value.components(separatedBy: invalidCharacters).joined(separator: "-")
        let trimmed = sanitized.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? "Share Card" : trimmed
    }

    private func nonEmpty(_ value: String?) -> String? {
        guard let value else { return nil }
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : value
    }

    private func makeTemplate(for theme: ShareCardTheme) -> ShareCardTemplate {
        let regions: (textSafeArea: CGRect, noteArea: CGRect, attributionArea: CGRect)
        switch theme {
        case .mistWash:
            regions = (
                CGRect(x: 130, y: 500, width: 940, height: 800),
                CGRect(x: 130, y: 300, width: 940, height: 130),
                CGRect(x: 130, y: 90, width: 940, height: 180)
            )
        case .sageLeaf:
            regions = (
                CGRect(x: 150, y: 480, width: 900, height: 820),
                CGRect(x: 150, y: 300, width: 900, height: 130),
                CGRect(x: 150, y: 90, width: 900, height: 180)
            )
        case .blushArcs:
            regions = (
                CGRect(x: 130, y: 520, width: 940, height: 780),
                CGRect(x: 130, y: 330, width: 940, height: 120),
                CGRect(x: 130, y: 100, width: 940, height: 170)
            )
        case .sandContours:
            regions = (
                CGRect(x: 170, y: 470, width: 860, height: 840),
                CGRect(x: 170, y: 300, width: 860, height: 130),
                CGRect(x: 170, y: 80, width: 860, height: 190)
            )
        case .lavenderStars:
            regions = (
                CGRect(x: 140, y: 500, width: 920, height: 800),
                CGRect(x: 140, y: 300, width: 920, height: 140),
                CGRect(x: 140, y: 90, width: 920, height: 180)
            )
        case .stoneTextile:
            regions = (
                CGRect(x: 160, y: 480, width: 880, height: 820),
                CGRect(x: 160, y: 290, width: 880, height: 140),
                CGRect(x: 160, y: 80, width: 880, height: 180)
            )
        case .vintagePaper:
            regions = (
                CGRect(x: 145, y: 500, width: 910, height: 800),
                CGRect(x: 145, y: 300, width: 910, height: 130),
                CGRect(x: 145, y: 90, width: 910, height: 180)
            )
        case .pinkBlueWash:
            regions = (
                CGRect(x: 150, y: 480, width: 900, height: 820),
                CGRect(x: 150, y: 300, width: 900, height: 130),
                CGRect(x: 150, y: 90, width: 900, height: 180)
            )
        case .softStone:
            regions = (
                CGRect(x: 130, y: 520, width: 940, height: 780),
                CGRect(x: 130, y: 330, width: 940, height: 120),
                CGRect(x: 130, y: 100, width: 940, height: 170)
            )
        case .linePaper:
            regions = (
                CGRect(x: 150, y: 450, width: 900, height: 850),
                CGRect(x: 150, y: 280, width: 900, height: 130),
                CGRect(x: 150, y: 80, width: 900, height: 180)
            )
        case .ruledNote:
            regions = (
                CGRect(x: 130, y: 480, width: 940, height: 820),
                CGRect(x: 130, y: 300, width: 940, height: 130),
                CGRect(x: 130, y: 90, width: 940, height: 180)
            )
        case .collagePaper:
            regions = (
                CGRect(x: 260, y: 650, width: 680, height: 450),
                CGRect(x: 180, y: 300, width: 840, height: 130),
                CGRect(x: 180, y: 450, width: 840, height: 180)
            )
        }
        let primaryTextColor = primaryTextColor(for: theme)
        return ShareCardTemplate(
            id: theme.rawValue,
            theme: theme,
            textSafeArea: regions.textSafeArea,
            noteArea: regions.noteArea,
            attributionArea: regions.attributionArea,
            primaryTextColor: primaryTextColor,
            attributionColor: primaryTextColor
        )
    }

    private func primaryTextColor(for theme: ShareCardTheme) -> ShareCardColor {
        switch theme {
        case .mistWash: return .nearBlack
        case .sageLeaf: return ShareCardColor(red: 0.10, green: 0.20, blue: 0.12)
        case .blushArcs: return ShareCardColor(red: 0.26, green: 0.10, blue: 0.12)
        case .sandContours: return ShareCardColor(red: 0.22, green: 0.15, blue: 0.07)
        case .lavenderStars: return ShareCardColor(red: 0.15, green: 0.10, blue: 0.25)
        case .stoneTextile: return ShareCardColor(red: 0.10, green: 0.10, blue: 0.10)
        case .vintagePaper: return ShareCardColor(red: 0.22, green: 0.12, blue: 0.06)
        case .pinkBlueWash: return ShareCardColor(red: 0.06, green: 0.20, blue: 0.22)
        case .softStone: return ShareCardColor(red: 0.08, green: 0.08, blue: 0.08)
        case .linePaper: return ShareCardColor(red: 0.12, green: 0.12, blue: 0.12)
        case .ruledNote: return ShareCardColor(red: 0.08, green: 0.15, blue: 0.22)
        case .collagePaper: return ShareCardColor(red: 0.22, green: 0.12, blue: 0.07)
        }
    }

    private var resourceBundle: Bundle {
#if SWIFT_PACKAGE
        return Bundle.module
#else
        return Bundle(for: ShareCardService.self)
#endif
    }

    private func font(
        for selection: ShareCardFont,
        size: CGFloat,
        weight: NSFont.Weight = .regular
    ) -> NSFont {
        guard let postScriptName = selection.postScriptName else {
            return NSFont.systemFont(ofSize: size, weight: weight)
        }

        register(selection)
        return NSFont(name: postScriptName, size: size)
            ?? NSFont.systemFont(ofSize: size, weight: weight)
    }

    private func register(_ selection: ShareCardFont) {
        guard let resourceName = selection.resourceName,
              let resourceExtension = selection.resourceExtension,
              let url = resourceBundle.url(forResource: resourceName, withExtension: resourceExtension) else {
            return
        }

        let key = url.standardizedFileURL.path
        Self.fontRegistrationLock.lock()
        defer { Self.fontRegistrationLock.unlock() }

        guard !Self.registeredFontResources.contains(key) else { return }
        _ = CTFontManagerRegisterFontsForURL(url as CFURL, .process, nil)
        Self.registeredFontResources.insert(key)
    }

    private func fittingFontSize(for text: String, card: ShareCard) -> CGFloat {
        if card.typography.sizeMode == .fixed {
            return card.typography.fontSize
        }

        return stride(from: Self.maximumReadableFontSize, through: Self.minimumReadableFontSize, by: -2)
            .first { textFits(text, card: card, fontSize: $0) }
            ?? Self.minimumReadableFontSize
    }

    private func textFits(_ text: String, card: ShareCard, fontSize: CGFloat) -> Bool {
        let framesetter = CTFramesetterCreateWithAttributedString(
            attributedText(
                text,
                fontSize: fontSize,
                font: card.font,
                alignment: card.typography.horizontalAlignment
            ) as CFAttributedString
        )
        let path = CGPath(rect: card.template.textSafeArea, transform: nil)
        let frame = CTFramesetterCreateFrame(
            framesetter,
            CFRange(location: 0, length: text.utf16.count),
            path,
            nil
        )
        let visible = CTFrameGetVisibleStringRange(frame)
        return visible.location + visible.length >= text.utf16.count
    }

    private func split(
        text: String,
        card: ShareCard,
        fontSize: CGFloat,
        area: CGRect? = nil
    ) -> [String] {
        let framesetter = CTFramesetterCreateWithAttributedString(
            attributedText(
                text,
                fontSize: fontSize,
                font: card.font,
                alignment: card.typography.horizontalAlignment
            ) as CFAttributedString
        )
        let length = text.utf16.count
        let textArea = area ?? card.template.textSafeArea
        var offset = 0
        var segments: [String] = []

        while offset < length {
            let path = CGPath(rect: textArea, transform: nil)
            let frame = CTFramesetterCreateFrame(
                framesetter,
                CFRange(location: offset, length: 0),
                path,
                nil
            )
            let visible = CTFrameGetVisibleStringRange(frame)
            let end = visible.location + visible.length
            guard end > offset else {
                let fallback = String(decoding: text.utf16.dropFirst(offset).prefix(1), as: UTF16.self)
                segments.append(fallback)
                offset += max(fallback.utf16.count, 1)
                continue
            }

            let segment = String(decoding: text.utf16.dropFirst(offset).prefix(end - offset), as: UTF16.self)
            segments.append(segment)
            offset = end
        }

        return segments
    }

    private func splitSupplementaryNote(
        text: String,
        card: ShareCard,
        fontSize: CGFloat
    ) -> [String] {
        let firstSegment = split(
            text: text,
            card: card,
            fontSize: fontSize,
            area: card.template.noteArea
        ).first
        guard let firstSegment, !firstSegment.isEmpty else { return [] }

        let remaining = String(decoding: text.utf16.dropFirst(firstSegment.utf16.count), as: UTF16.self)
        guard !remaining.isEmpty else { return [firstSegment] }

        return [firstSegment] + split(
            text: remaining,
            card: card,
            fontSize: fontSize,
            area: card.template.textSafeArea
        )
    }

    private func attributedText(
        _ text: String,
        fontSize: CGFloat,
        font: ShareCardFont,
        alignment: ShareCardHorizontalAlignment,
        color: ShareCardColor? = nil,
        weight: NSFont.Weight = .regular,
        lineBreakMode: NSLineBreakMode = .byWordWrapping
    ) -> NSAttributedString {
        let paragraphStyle = NSMutableParagraphStyle()
        paragraphStyle.alignment = alignment.nsTextAlignment
        paragraphStyle.lineBreakMode = lineBreakMode
        paragraphStyle.lineHeightMultiple = Self.lineHeightMultiple
        let lineHeight = fontSize * Self.lineHeightMultiple
        paragraphStyle.minimumLineHeight = lineHeight
        paragraphStyle.maximumLineHeight = lineHeight
        var attributes: [NSAttributedString.Key: Any] = [
            .font: self.coreTextFont(for: font, size: fontSize, weight: weight),
            .paragraphStyle: paragraphStyle
        ]
        if let color {
            attributes[.foregroundColor] = color.nsColor
        }
        return NSAttributedString(
            string: text,
            attributes: attributes
        )
    }

    private func coreTextFont(
        for selection: ShareCardFont,
        size: CGFloat,
        weight: NSFont.Weight
    ) -> CTFont {
        if let postScriptName = selection.postScriptName {
            register(selection)
            return CTFontCreateWithName(postScriptName as CFString, size, nil)
        }

        return self.font(for: selection, size: size, weight: weight) as CTFont
    }

    private func primaryTextFrame(for text: String, card: ShareCard, fontSize: CGFloat) -> CGRect {
        textFrame(for: text, in: card.template.textSafeArea, card: card, fontSize: fontSize)
    }

    private func textFrame(
        for text: String,
        in safeArea: CGRect,
        card: ShareCard,
        fontSize: CGFloat
    ) -> CGRect {
        let measuredHeight = measuredHeight(for: text, card: card, fontSize: fontSize, width: safeArea.width)
        let height = min(
            safeArea.height,
            max(measuredHeight + 4, fontSize * Self.lineHeightMultiple)
        )
        let y: CGFloat
        switch card.typography.verticalAlignment {
        case .top:
            y = safeArea.maxY - height
        case .center:
            y = safeArea.midY - height / 2
        case .bottom:
            y = safeArea.minY
        }
        return CGRect(x: safeArea.minX, y: y, width: safeArea.width, height: height)
    }

    private func measuredHeight(
        for text: String,
        card: ShareCard,
        fontSize: CGFloat,
        width: CGFloat
    ) -> CGFloat {
        let framesetter = CTFramesetterCreateWithAttributedString(
            attributedText(
                text,
                fontSize: fontSize,
                font: card.font,
                alignment: card.typography.horizontalAlignment
            ) as CFAttributedString
        )
        var fitRange = CFRange()
        let size = CTFramesetterSuggestFrameSizeWithConstraints(
            framesetter,
            CFRange(location: 0, length: text.utf16.count),
            nil,
            CGSize(width: width, height: CGFloat.greatestFiniteMagnitude),
            &fitRange
        )
        return ceil(size.height)
    }
}

private extension ShareCardHorizontalAlignment {
    var nsTextAlignment: NSTextAlignment {
        switch self {
        case .left: return .left
        case .center: return .center
        case .right: return .right
        }
    }
}
