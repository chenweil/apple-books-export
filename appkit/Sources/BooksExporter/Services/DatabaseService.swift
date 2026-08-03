import Foundation
import SQLite3

class DatabaseService {
    private var bkLibraryDB: OpaquePointer?
    private var aeAnnotationDB: OpaquePointer?
    private let sqliteTransient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
    
    private var bkLibraryPath: String?
    private var aeAnnotationPath: String?
    
    init() {
        // Defer path resolution and throwing to first use
    }
    
    func open() throws {
        if bkLibraryPath == nil || aeAnnotationPath == nil {
            try resolveDatabasePaths()
        }

        let bkURI = "file:\(bkLibraryPath!)?immutable=1"
        if sqlite3_open_v2(bkURI, &bkLibraryDB, SQLITE_OPEN_READONLY | SQLITE_OPEN_URI, nil) != SQLITE_OK {
            throw DatabaseError.openFailed("BKLibrary: \(String(cString: sqlite3_errmsg(bkLibraryDB)))")
        }

        let aeURI = "file:\(aeAnnotationPath!)?immutable=1"
        if sqlite3_open_v2(aeURI, &aeAnnotationDB, SQLITE_OPEN_READONLY | SQLITE_OPEN_URI, nil) != SQLITE_OK {
            throw DatabaseError.openFailed("AEAnnotation: \(String(cString: sqlite3_errmsg(aeAnnotationDB)))")
        }
    }
    
    private func resolveDatabasePaths() throws {
        let homeDir = FileManager.default.homeDirectoryForCurrentUser
        let dataPath = homeDir.appendingPathComponent("Library/Containers/com.apple.iBooksX/Data/Documents")
        
        let bkLibraryDir = dataPath.appendingPathComponent("BKLibrary")
        let aeAnnotationDir = dataPath.appendingPathComponent("AEAnnotation")
        
        guard let bkLibraryFile = try? FileManager.default.contentsOfDirectory(at: bkLibraryDir, includingPropertiesForKeys: nil)
            .first(where: { $0.pathExtension == "sqlite" }) else {
            throw DatabaseError.databaseNotFound("BKLibrary")
        }
        
        guard let aeAnnotationFile = try? FileManager.default.contentsOfDirectory(at: aeAnnotationDir, includingPropertiesForKeys: nil)
            .first(where: { $0.pathExtension == "sqlite" }) else {
            throw DatabaseError.databaseNotFound("AEAnnotation")
        }
        
        self.bkLibraryPath = bkLibraryFile.path
        self.aeAnnotationPath = aeAnnotationFile.path
    }
    
    func close() {
        sqlite3_close(bkLibraryDB)
        sqlite3_close(aeAnnotationDB)
        bkLibraryDB = nil
        aeAnnotationDB = nil
    }
    
    func getBooks() throws -> [Book] {
        try open()
        defer { close() }

        // Step 1: 逐条取「有无批注 / 有无正文」,在 Swift 侧用 AnnotationClassifier 归类。
        // 不在 SQL 里写分类规则,否则这里和 getAnnotations 会各写一份、迟早漂移。
        let countQuery = """
        SELECT ZANNOTATIONASSETID,
               CASE WHEN COALESCE(ZANNOTATIONNOTE, '') <> '' THEN 1 ELSE 0 END,
               CASE WHEN COALESCE(ZANNOTATIONSELECTEDTEXT, '') <> '' THEN 1 ELSE 0 END
        FROM ZAEANNOTATION
        WHERE (ZANNOTATIONDELETED IS NULL OR ZANNOTATIONDELETED = 0)
        """

        var countStatement: OpaquePointer?
        guard sqlite3_prepare_v2(aeAnnotationDB, countQuery, -1, &countStatement, nil) == SQLITE_OK else {
            throw DatabaseError.queryFailed(String(cString: sqlite3_errmsg(aeAnnotationDB)))
        }
        defer { sqlite3_finalize(countStatement) }

        struct AnnotationCount {
            var highlights = 0
            var notes = 0
            var total: Int { highlights + notes }
        }

        var countsByAssetId: [String: AnnotationCount] = [:]
        while sqlite3_step(countStatement) == SQLITE_ROW {
            guard let assetId = sqlite3_column_text(countStatement, 0) else { continue }
            let hasNote = sqlite3_column_int(countStatement, 1) == 1
            let hasSelectedText = sqlite3_column_int(countStatement, 2) == 1

            guard let type = AnnotationClassifier.classify(hasNote: hasNote, hasSelectedText: hasSelectedText) else {
                continue
            }

            let key = String(cString: assetId)
            var count = countsByAssetId[key] ?? AnnotationCount()
            switch type {
            case .highlight: count.highlights += 1
            case .note: count.notes += 1
            }
            countsByAssetId[key] = count
        }

        guard !countsByAssetId.isEmpty else { return [] }

        // Step 2: Query BKLibrary for book details
        let assetIds = Array(countsByAssetId.keys)
        var books: [Book] = []

        for assetId in assetIds {
            let bookQuery = "SELECT ZASSETID, ZTITLE, ZAUTHOR FROM ZBKLIBRARYASSET WHERE ZASSETID = ?"
            var bookStatement: OpaquePointer?
            guard sqlite3_prepare_v2(bkLibraryDB, bookQuery, -1, &bookStatement, nil) == SQLITE_OK else {
                continue
            }
            defer { sqlite3_finalize(bookStatement) }

            sqlite3_bind_text(bookStatement, 1, assetId, -1, sqliteTransient)

            if sqlite3_step(bookStatement) == SQLITE_ROW {
                let title = sqlite3_column_text(bookStatement, 1).map { String(cString: $0) } ?? "未知书名"
                let author = sqlite3_column_text(bookStatement, 2).map { String(cString: $0) } ?? ""
                let count = countsByAssetId[assetId]!

                books.append(Book(
                    id: assetId,
                    title: title,
                    author: author,
                    totalAnnotations: count.total,
                    highlightsCount: count.highlights,
                    notesCount: count.notes
                ))
            }
        }

        books.sort { $0.totalAnnotations > $1.totalAnnotations }
        return books
    }
    
    func getAnnotations(for bookId: String) throws -> [Annotation] {
        try open()
        defer { close() }
        
        let query = """
        SELECT
            COALESCE(ZANNOTATIONUUID, CAST(Z_PK AS TEXT)),
            ZANNOTATIONTYPE,
            ZANNOTATIONSELECTEDTEXT,
            ZANNOTATIONNOTE,
            ZANNOTATIONCREATIONDATE,
            ZANNOTATIONLOCATION
        FROM ZAEANNOTATION
        WHERE ZANNOTATIONASSETID = ?
        AND (ZANNOTATIONDELETED IS NULL OR ZANNOTATIONDELETED = 0)
        ORDER BY ZANNOTATIONCREATIONDATE
        """
        
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(aeAnnotationDB, query, -1, &statement, nil) == SQLITE_OK else {
            throw DatabaseError.queryFailed(String(cString: sqlite3_errmsg(aeAnnotationDB)))
        }
        defer { sqlite3_finalize(statement) }
        
        sqlite3_bind_text(statement, 1, bookId, -1, sqliteTransient)
        
        var annotations: [Annotation] = []
        
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let uid = sqlite3_column_text(statement, 0) else { continue }

            let content = sqlite3_column_text(statement, 2).map { String(cString: $0) }
            let note = sqlite3_column_text(statement, 3).map { String(cString: $0) }
            let createdAtRaw = sqlite3_column_double(statement, 4)
            let location = sqlite3_column_text(statement, 5).map { String(cString: $0) } ?? ""

            // 没有正文也没有批注的行只会渲染成一个孤零零的位置,丢弃。
            guard let type = AnnotationClassifier.classify(
                hasNote: !(note ?? "").isEmpty,
                hasSelectedText: !(content ?? "").isEmpty
            ) else { continue }

            let createdAt = Date(timeIntervalSinceReferenceDate: TimeInterval(createdAtRaw))
            
            annotations.append(Annotation(
                id: String(cString: uid),
                type: type,
                chapterTitle: "",
                locationInfo: location,
                contentText: content,
                noteText: note,
                createdAt: createdAt
            ))
        }
        
        return annotations
    }
}

enum DatabaseError: LocalizedError {
    case databaseNotFound(String)
    case openFailed(String)
    case queryFailed(String)
    
    var errorDescription: String? {
        switch self {
        case .databaseNotFound(let name):
            return "未找到数据库：\(name)。请确保已安装 Apple Books 并打开过。"
        case .openFailed(let details):
            return "打开数据库失败：\(details)"
        case .queryFailed(let details):
            return "查询失败：\(details)"
        }
    }
}
