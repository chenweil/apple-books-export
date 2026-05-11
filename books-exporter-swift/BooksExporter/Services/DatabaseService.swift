import Foundation
import SQLite3

actor DatabaseService {
    private var bkLibraryDB: OpaquePointer?
    private var aeAnnotationDB: OpaquePointer?
    
    private var bkLibraryPath: String?
    private var aeAnnotationPath: String?
    
    init() {
        // Defer path resolution and throwing to first use
    }
    
    func open() throws {
        if bkLibraryPath == nil || aeAnnotationPath == nil {
            try resolveDatabasePaths()
        }
        
        if sqlite3_open_v2(bkLibraryPath!, &bkLibraryDB, SQLITE_OPEN_READONLY, nil) != SQLITE_OK {
            throw DatabaseError.openFailed("BKLibrary: \(String(cString: sqlite3_errmsg(bkLibraryDB)))")
        }
        
        if sqlite3_open_v2(aeAnnotationPath!, &aeAnnotationDB, SQLITE_OPEN_READONLY, nil) != SQLITE_OK {
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
    
    func getBooks() async throws -> [Book] {
        try open()
        defer { close() }
        
        let query = """
        SELECT 
            a.ZASSETID,
            a.ZTITLE,
            a.ZAUTHOR,
            SUM(CASE WHEN b.ZANNOTATIONTYPE IN (2,3) THEN 1 ELSE 0 END) as highlights,
            SUM(CASE WHEN b.ZANNOTATIONTYPE = 1 THEN 1 ELSE 0 END) as notes,
            SUM(CASE WHEN b.ZANNOTATIONTYPE = 0 THEN 1 ELSE 0 END) as bookmarks
        FROM ZBKLIBRARYASSET a
        INNER JOIN ZAEANNOTATION b ON a.ZASSETID = b.ZANNOTATIONASSETID
        WHERE (b.ZANNOTATIONDELETED IS NULL OR b.ZANNOTATIONDELETED = 0)
          AND (b.ZANNOTATIONSELECTEDTEXT IS NOT NULL OR b.ZANNOTATIONNOTE IS NOT NULL OR b.ZANNOTATIONTYPE = 0)
        GROUP BY a.ZASSETID, a.ZTITLE, a.ZAUTHOR
        ORDER BY COUNT(b.Z_PK) DESC
        """
        
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(bkLibraryDB, query, -1, &statement, nil) == SQLITE_OK else {
            throw DatabaseError.queryFailed(String(cString: sqlite3_errmsg(bkLibraryDB)))
        }
        defer { sqlite3_finalize(statement) }
        
        var books: [Book] = []
        
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let id = sqlite3_column_text(statement, 0),
                  let title = sqlite3_column_text(statement, 1),
                  let author = sqlite3_column_text(statement, 2) else {
                continue
            }
            
            let highlights = sqlite3_column_int(statement, 3)
            let notes = sqlite3_column_int(statement, 4)
            let bookmarks = sqlite3_column_int(statement, 5)
            let total = Int(highlights + notes + bookmarks)
            
            books.append(Book(
                id: String(cString: id),
                title: String(cString: title),
                author: String(cString: author),
                totalAnnotations: total,
                highlightsCount: Int(highlights),
                annotationsCount: 0,
                notesCount: Int(notes),
                bookmarksCount: Int(bookmarks)
            ))
        }
        
        return books
    }
    
    func getAnnotations(for bookId: String) async throws -> [Annotation] {
        try open()
        defer { close() }
        
        let query = """
        SELECT
            ZANNOTATIONUID,
            ZANNOTATIONTYPE,
            ZANNOTATIONSELECTEDTEXT,
            ZANNOTATIONNOTE,
            ZANNOTATIONCREATIONDATE,
            ZANNOTATIONLOCATION,
            ZFOLDERTYPE
        FROM ZAEANNOTATION
        WHERE ZANNOTATIONASSETID = ?
        AND (ZANNOTATIONDELETED IS NULL OR ZANNOTATIONDELETED = 0)
        AND (ZANNOTATIONSELECTEDTEXT IS NOT NULL OR ZANNOTATIONNOTE IS NOT NULL OR ZANNOTATIONTYPE = 0)
        ORDER BY ZANNOTATIONCREATIONDATE
        """
        
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(aeAnnotationDB, query, -1, &statement, nil) == SQLITE_OK else {
            throw DatabaseError.queryFailed(String(cString: sqlite3_errmsg(aeAnnotationDB)))
        }
        defer { sqlite3_finalize(statement) }
        
        sqlite3_bind_text(statement, 1, bookId, -1, nil)
        
        var annotations: [Annotation] = []
        
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let uid = sqlite3_column_text(statement, 0) else { continue }
            
            let typeRaw = sqlite3_column_int(statement, 1)
            let content = sqlite3_column_text(statement, 2).map { String(cString: $0) }
            let note = sqlite3_column_text(statement, 3).map { String(cString: $0) }
            let createdAtRaw = sqlite3_column_double(statement, 4)
            let location = sqlite3_column_text(statement, 5).map { String(cString: $0) } ?? ""
            
            let type: AnnotationType
            switch Int(typeRaw) {
            case 0: type = .bookmark
            case 1: type = .note
            case 2, 3: type = .highlight
            default: continue
            }
            
            var createdAt = Date()
            if createdAtRaw > 0 {
                let appleEpoch = Date(timeIntervalSinceReferenceDate: 0).addingTimeInterval(-978278400)
                createdAt = appleEpoch.addingTimeInterval(TimeInterval(createdAtRaw))
            }
            
            annotations.append(Annotation(
                id: String(cString: uid),
                type: type,
                chapterTitle: "",
                locationInfo: String(cString: location),
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
