import Foundation
import SQLite3
import XCTest
@testable import BooksExporterCore

final class DatabaseServiceTests: XCTestCase {
    func testReadsActiveAnnotationsCommittedToWAL() throws {
        let rootURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("books-exporter-database-" + UUID().uuidString)
        let bkLibraryURL = rootURL.appendingPathComponent("BKLibrary", isDirectory: true)
        let aeAnnotationURL = rootURL.appendingPathComponent("AEAnnotation", isDirectory: true)
        try FileManager.default.createDirectory(at: bkLibraryURL, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: aeAnnotationURL, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: rootURL) }

        let bkLibraryPath = bkLibraryURL.appendingPathComponent("BKLibrary.sqlite").path
        let aeAnnotationPath = aeAnnotationURL.appendingPathComponent("AEAnnotation.sqlite").path
        try createBookDatabase(at: bkLibraryPath)
        try createAnnotationDatabase(at: aeAnnotationPath)

        let writer = try SQLiteConnection(path: aeAnnotationPath)
        defer { writer.close() }
        try writer.execute("PRAGMA journal_mode = WAL;")
        try writer.execute("""
            INSERT INTO ZAEANNOTATION (
                Z_PK, ZANNOTATIONUUID, ZANNOTATIONTYPE, ZANNOTATIONSELECTEDTEXT,
                ZANNOTATIONNOTE, ZANNOTATIONCREATIONDATE, ZANNOTATIONLOCATION,
                ZANNOTATIONASSETID, ZANNOTATIONDELETED
            ) VALUES (2, 'new-annotation', 3, 'new passage', 'new note', 807498224.176409, '2', 'asset-1', 0);
            """)

        let service = DatabaseService(
            bkLibraryPath: bkLibraryPath,
            aeAnnotationPath: aeAnnotationPath
        )
        let books = try service.getBooks()
        let annotations = try service.getAnnotations(for: "asset-1")

        XCTAssertEqual(books.first?.totalAnnotations, 2)
        XCTAssertEqual(books.first?.highlightsCount, 1)
        XCTAssertEqual(books.first?.notesCount, 1)
        XCTAssertEqual(annotations.map(\.id), ["old-annotation", "new-annotation"])
        XCTAssertEqual(annotations.last?.noteText, "new note")
    }

    private func createBookDatabase(at path: String) throws {
        let database = try SQLiteConnection(path: path)
        try database.execute("""
            CREATE TABLE ZBKLIBRARYASSET (
                Z_PK INTEGER PRIMARY KEY,
                ZASSETID TEXT UNIQUE,
                ZTITLE TEXT,
                ZAUTHOR TEXT
            );
            INSERT INTO ZBKLIBRARYASSET (Z_PK, ZASSETID, ZTITLE, ZAUTHOR)
            VALUES (1, 'asset-1', 'Test Book', 'Test Author');
            """)
    }

    private func createAnnotationDatabase(at path: String) throws {
        let database = try SQLiteConnection(path: path)
        try database.execute("""
            CREATE TABLE ZAEANNOTATION (
                Z_PK INTEGER PRIMARY KEY,
                ZANNOTATIONUUID TEXT,
                ZANNOTATIONTYPE INTEGER,
                ZANNOTATIONSELECTEDTEXT TEXT,
                ZANNOTATIONNOTE TEXT,
                ZANNOTATIONCREATIONDATE REAL,
                ZANNOTATIONLOCATION TEXT,
                ZANNOTATIONASSETID TEXT,
                ZANNOTATIONDELETED INTEGER
            );
            INSERT INTO ZAEANNOTATION (
                Z_PK, ZANNOTATIONUUID, ZANNOTATIONTYPE, ZANNOTATIONSELECTEDTEXT,
                ZANNOTATIONNOTE, ZANNOTATIONCREATIONDATE, ZANNOTATIONLOCATION,
                ZANNOTATIONASSETID, ZANNOTATIONDELETED
            ) VALUES (1, 'old-annotation', 3, 'old passage', NULL, 807000000, '1', 'asset-1', 0);
            """)
    }
}

private final class SQLiteConnection {
    private var database: OpaquePointer?

    init(path: String) throws {
        guard sqlite3_open_v2(path, &database, SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE, nil) == SQLITE_OK else {
            throw NSError(domain: "BooksExporterCoreTests", code: 1, userInfo: [
                NSLocalizedDescriptionKey: String(cString: sqlite3_errmsg(database))
            ])
        }
    }

    func execute(_ sql: String) throws {
        var errorMessage: UnsafeMutablePointer<CChar>?
        defer {
            if let errorMessage {
                sqlite3_free(errorMessage)
            }
        }

        guard sqlite3_exec(database, sql, nil, nil, &errorMessage) == SQLITE_OK else {
            let message = errorMessage.map { String(cString: $0) } ?? "unknown SQLite error"
            throw NSError(domain: "BooksExporterCoreTests", code: 2, userInfo: [
                NSLocalizedDescriptionKey: message
            ])
        }
    }

    func close() {
        guard let database else { return }
        sqlite3_close(database)
        self.database = nil
    }

    deinit {
        close()
    }
}
