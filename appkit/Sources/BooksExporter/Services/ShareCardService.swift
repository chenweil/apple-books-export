import AppKit
import CoreText
import Foundation

public enum ShareCardTheme: String, CaseIterable, Codable {
    case mistWash
    case sageLeaf
    case blushArcs
    case sandContours
    case lavenderStars
    case stoneTextile

    public var displayName: String {
        switch self {
        case .mistWash: return "雾洗"
        case .sageLeaf: return "鼠尾草"
        case .blushArcs: return "腮红"
        case .sandContours: return "沙纹"
        case .lavenderStars: return "星点"
        case .stoneTextile: return "石纹"
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

public struct ShareCardTemplate: Equatable, Codable {
    public let id: String
    public let theme: ShareCardTheme
    public let variant: Int

    public init(id: String, theme: ShareCardTheme, variant: Int) {
        self.id = id
        self.theme = theme
        self.variant = variant
    }
}

public struct ShareCard: Equatable {
    public let bookTitle: String
    public let author: String?
    public let primaryText: String
    public let supplementaryNote: String?
    public let template: ShareCardTemplate
    public let font: ShareCardFont

    public init(
        bookTitle: String,
        author: String?,
        primaryText: String,
        supplementaryNote: String?,
        template: ShareCardTemplate,
        font: ShareCardFont = .system
    ) {
        self.bookTitle = bookTitle
        self.author = author
        self.primaryText = primaryText
        self.supplementaryNote = supplementaryNote
        self.template = template
        self.font = font
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

    public init(index: Int, primaryText: String, supplementaryNote: String?, fontSize: CGFloat) {
        self.index = index
        self.primaryText = primaryText
        self.supplementaryNote = supplementaryNote
        self.fontSize = fontSize
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

    public var errorDescription: String? {
        switch self {
        case .emptyCardText: return "卡片没有可导出的正文。"
        case .imageEncodingFailed: return "无法生成 PNG 图片。"
        }
    }
}

/// Public generation-and-export seam for AppKit Share Cards.
public final class ShareCardService {
    public static let canvasSize = CGSize(width: 1200, height: 1600)
    public static let minimumReadableFontSize: CGFloat = 42

    private static let fontRegistrationLock = NSLock()
    private static var registeredFontResources = Set<String>()

    private let fileManager: FileManager
    private let folderRevealer: (URL) -> Void

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
            template: template(for: theme, variant: 0),
            font: font
        )
    }

    public func alternativeCards(for card: ShareCard) -> [ShareCard] {
        let themes: [ShareCardTheme] = [.sageLeaf, .blushArcs, .sandContours, .lavenderStars]
        return themes.enumerated().map { index, theme in
            ShareCard(
                bookTitle: card.bookTitle,
                author: card.author,
                primaryText: card.primaryText,
                supplementaryNote: card.supplementaryNote,
                template: template(for: theme, variant: index + 1),
                font: card.font
            )
        }
    }

    public func pages(for card: ShareCard) -> [ShareCardPage] {
        let text = card.primaryText
        guard !text.isEmpty else { return [] }

        let fontSize = fittingFontSize(for: text, card: card)
        let segments = split(text: text, card: card, fontSize: fontSize)
        return segments.enumerated().map { index, segment in
            ShareCardPage(
                index: index,
                primaryText: segment,
                supplementaryNote: index == segments.count - 1 ? card.supplementaryNote : nil,
                fontSize: fontSize
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

    public func export(
        _ card: ShareCard,
        to directoryURL: URL,
        openExportFolder: Bool = ShareCardPreferences.openExportFolder
    ) throws -> ShareCardExportResult {
        guard !card.primaryText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw ShareCardExportError.emptyCardText
        }

        try fileManager.createDirectory(at: directoryURL, withIntermediateDirectories: true)
        let pages = pages(for: card)
        let files = try pages.enumerated().map { index, page in
            let fileURL = directoryURL.appendingPathComponent(
                fileName(for: card, pageIndex: index, pageCount: pages.count)
            )
            let data = try pngData(for: card, page: page)
            try data.write(to: fileURL, options: .atomic)
            return fileURL
        }

        if openExportFolder {
            folderRevealer(directoryURL)
        }

        return ShareCardExportResult(
            files: files,
            pageCount: pages.count,
            didRevealExportFolder: openExportFolder
        )
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

        if let backgroundURL = resourceBundle.url(
            forResource: card.theme.resourceName,
            withExtension: "png"
        ), let background = NSImage(contentsOf: backgroundURL) {
            background.draw(
                in: NSRect(origin: .zero, size: Self.canvasSize),
                from: .zero,
                operation: .sourceOver,
                fraction: 1
            )
        }

        let textRect = textRect(for: card)
        let paragraphStyle = NSMutableParagraphStyle()
        paragraphStyle.alignment = card.template.variant == 2 ? .center : .left
        paragraphStyle.lineBreakMode = .byWordWrapping
        let attributes: [NSAttributedString.Key: Any] = [
            .font: font(for: card.font, size: page.fontSize),
            .foregroundColor: NSColor(calibratedWhite: 0.12, alpha: 1),
            .paragraphStyle: paragraphStyle
        ]
        NSAttributedString(string: page.primaryText, attributes: attributes)
            .draw(with: textRect, options: [.usesLineFragmentOrigin, .usesFontLeading])

        if let note = page.supplementaryNote {
            let noteAttributes: [NSAttributedString.Key: Any] = [
                .font: font(for: card.font, size: 38),
                .foregroundColor: NSColor(calibratedWhite: 0.22, alpha: 0.85),
                .paragraphStyle: paragraphStyle
            ]
            NSAttributedString(string: note, attributes: noteAttributes)
                .draw(with: NSRect(x: 130, y: 300, width: 940, height: 130),
                      options: [.usesLineFragmentOrigin, .usesFontLeading])
        }

        let footerParagraphStyle = NSMutableParagraphStyle()
        footerParagraphStyle.alignment = .left
        footerParagraphStyle.lineBreakMode = .byCharWrapping
        let footerAttributes: [NSAttributedString.Key: Any] = [
            .font: font(for: card.font, size: 30, weight: .medium),
            .foregroundColor: NSColor(calibratedWhite: 0.18, alpha: 0.78),
            .paragraphStyle: footerParagraphStyle
        ]
        NSAttributedString(string: card.attributionText, attributes: footerAttributes)
            .draw(
                with: NSRect(x: 130, y: 90, width: 940, height: 180),
                options: [.usesLineFragmentOrigin, .usesFontLeading]
            )

        guard let data = bitmap.representation(using: .png, properties: [:]) else {
            throw ShareCardExportError.imageEncodingFailed
        }
        return data
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

    private func template(for theme: ShareCardTheme, variant: Int) -> ShareCardTemplate {
        ShareCardTemplate(id: "\(theme.rawValue)-\(variant)", theme: theme, variant: variant)
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
        stride(from: CGFloat(72), through: Self.minimumReadableFontSize, by: -2)
            .first { textFits(text, card: card, fontSize: $0) }
            ?? Self.minimumReadableFontSize
    }

    private func textFits(_ text: String, card: ShareCard, fontSize: CGFloat) -> Bool {
        let framesetter = CTFramesetterCreateWithAttributedString(
            attributedText(text, fontSize: fontSize, font: card.font)
        )
        let path = CGPath(rect: textRect(for: card), transform: nil)
        let frame = CTFramesetterCreateFrame(
            framesetter,
            CFRange(location: 0, length: text.utf16.count),
            path,
            nil
        )
        let visible = CTFrameGetVisibleStringRange(frame)
        return visible.location + visible.length >= text.utf16.count
    }

    private func split(text: String, card: ShareCard, fontSize: CGFloat) -> [String] {
        let framesetter = CTFramesetterCreateWithAttributedString(
            attributedText(text, fontSize: fontSize, font: card.font)
        )
        let length = text.utf16.count
        var offset = 0
        var segments: [String] = []

        while offset < length {
            let path = CGPath(rect: textRect(for: card), transform: nil)
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

    private func attributedText(
        _ text: String,
        fontSize: CGFloat,
        font: ShareCardFont
    ) -> CFAttributedString {
        let paragraphStyle = NSMutableParagraphStyle()
        paragraphStyle.alignment = .left
        paragraphStyle.lineBreakMode = .byWordWrapping
        return NSAttributedString(
            string: text,
            attributes: [
                .font: self.font(for: font, size: fontSize),
                .paragraphStyle: paragraphStyle
            ]
        ) as CFAttributedString
    }

    private func textRect(for card: ShareCard) -> NSRect {
        switch card.template.variant % 4 {
        case 1: return NSRect(x: 150, y: 480, width: 900, height: 820)
        case 2: return NSRect(x: 130, y: 520, width: 940, height: 780)
        case 3: return NSRect(x: 170, y: 470, width: 860, height: 840)
        default: return NSRect(x: 130, y: 500, width: 940, height: 800)
        }
    }
}
