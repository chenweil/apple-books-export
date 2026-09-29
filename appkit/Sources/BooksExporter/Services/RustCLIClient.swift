import Foundation

/// The result of one Rust CLI invocation.
///
/// Keeping stdout, stderr, and the exit status as separate values is
/// intentional.  The machine protocol uses stdout for successful JSON and
/// stderr for structured failures; combining the streams would make the
/// AppKit error handling depend on human-facing output.
struct RustCLICommandResult: Sendable {
    let stdout: Data
    let stderr: Data
    let terminationStatus: Int32

    init(stdout: Data = Data(), stderr: Data = Data(), terminationStatus: Int32) {
        self.stdout = stdout
        self.stderr = stderr
        self.terminationStatus = terminationStatus
    }

    static func success(_ stdout: String) -> Self {
        Self(
            stdout: Data(stdout.utf8),
            terminationStatus: 0
        )
    }

    static func failure(status: Int32, stderr: String, stdout: String = "") -> Self {
        Self(
            stdout: Data(stdout.utf8),
            stderr: Data(stderr.utf8),
            terminationStatus: status
        )
    }
}

struct RustExportReceipt: Equatable, Sendable {
    let assetID: String
    let title: String
    let annotationCount: Int
    let format: String
    let outputDirectory: String
    let generatedFiles: [String]
}

enum RustCLIError: LocalizedError, Sendable {
    case binaryNotFound(path: String)
    case binaryIncompatible(details: String)
    case launchFailed(details: String)
    case commandFailed(
        status: Int32,
        code: String,
        message: String,
        remediation: String?
    )
    case invalidResponse(String)
    case unsupportedSchemaVersion(Int)

    var stableCode: String {
        switch self {
        case .binaryNotFound:
            return "BINARY_NOT_FOUND"
        case .binaryIncompatible:
            return "BINARY_INCOMPATIBLE"
        case .launchFailed:
            return "BACKEND_UNAVAILABLE"
        case .commandFailed(_, let code, _, _):
            return code
        case .invalidResponse:
            return "BACKEND_UNAVAILABLE"
        case .unsupportedSchemaVersion:
            return "UNSUPPORTED_SCHEMA_VERSION"
        }
    }

    var remediation: String? {
        switch self {
        case .binaryNotFound:
            return "Build apple-books-exporter and set APPLE_BOOKS_EXPORTER_BIN, or bundle it at Contents/Resources/apple-books-exporter."
        case .binaryIncompatible:
            return "Use a native macOS arm64 or x86_64 apple-books-exporter binary that matches this Mac."
        case .launchFailed:
            return "Verify that the bundled apple-books-exporter binary exists and is executable, then retry."
        case .commandFailed(_, _, _, let remediation):
            return remediation
        case .invalidResponse:
            return "Use a compatible apple-books-exporter binary and retry."
        case .unsupportedSchemaVersion:
            return "Update Books Exporter or use a compatible apple-books-exporter binary."
        }
    }

    var errorDescription: String? {
        switch self {
        case .binaryNotFound(let path):
            return "未找到 Rust 导出器 binary：\(path)"
        case .binaryIncompatible(let details):
            return "Rust 导出器 binary 与当前 Mac 不兼容：\(details)"
        case .launchFailed(let details):
            return "无法启动 Rust 导出器：\(details)"
        case .commandFailed(_, _, let message, _):
            return message
        case .invalidResponse(let message):
            return "Rust 导出器返回了无效的机器响应：\(message)"
        case .unsupportedSchemaVersion(let version):
            return "Rust 导出器机器协议版本 \(version) 不受支持"
        }
    }

    var userFacingDescription: String {
        var description = "[\(stableCode)] \(localizedDescription)"
        if let remediation, !remediation.isEmpty {
            description += "\n\n\(remediation)"
        }
        return description
    }
}

/// AppKit's boundary to the canonical Rust data core.
///
/// The runner is injected so protocol behavior can be tested without
/// launching a process or touching a user's Apple Books database.  Production
/// instances use `Process` and keep the process streams separate.
final class RustCLIClient: @unchecked Sendable {
    typealias Runner = @Sendable (URL, [String]) async throws -> RustCLICommandResult

    private static let supportedSchemaVersion = 1
    private static let defaultRunner: Runner = { executableURL, arguments in
        try await RustCLIProcessRunner.run(
            executableURL: executableURL,
            arguments: arguments
        )
    }
    private let executableURL: URL?
    private let runner: Runner

    init(executableURL: URL?, runner: Runner? = nil) {
        self.executableURL = executableURL
        self.runner = runner ?? Self.defaultRunner
    }

    /// Resolve the bundled binary for the running AppKit application.
    ///
    /// `APPLE_BOOKS_EXPORTER_BIN` is deliberately an explicit development
    /// override.  A packaged application otherwise resolves the binary from
    /// its Resources directory, and finally from PATH for local development.
    static func makeForCurrentApp(
        environment: [String: String] = ProcessInfo.processInfo.environment,
        bundle: Bundle = .main
    ) -> RustCLIClient {
        if let override = environment["APPLE_BOOKS_EXPORTER_BIN"], !override.isEmpty {
            return RustCLIClient(executableURL: URL(fileURLWithPath: override))
        }

        if let bundled = bundle.url(forResource: "apple-books-exporter", withExtension: nil) {
            return RustCLIClient(executableURL: bundled)
        }

        if let executableDirectory = bundle.executableURL?.deletingLastPathComponent() {
            let adjacent = executableDirectory.appendingPathComponent("apple-books-exporter")
            if FileManager.default.fileExists(atPath: adjacent.path) {
                return RustCLIClient(executableURL: adjacent)
            }
        }

        if let path = environment["PATH"] {
            for directory in path.split(separator: ":") {
                let candidate = URL(fileURLWithPath: String(directory))
                    .appendingPathComponent("apple-books-exporter")
                if FileManager.default.isExecutableFile(atPath: candidate.path) {
                    return RustCLIClient(executableURL: candidate)
                }
            }
        }

        return RustCLIClient(executableURL: nil)
    }

    func listBooks() async throws -> [Book] {
        let result = try await run(arguments: ["list", "--json"])
        let response: BookListResponse = try decodeResponse(from: result.stdout)
        return try response.books.map { book in
            guard book.noteCount >= 0 else {
                throw RustCLIError.invalidResponse("note_count 不能为负数")
            }

            return Book(
                id: book.assetID,
                title: book.title,
                author: book.author,
                totalAnnotations: book.noteCount,
                // The machine list contract intentionally only promises the
                // total.  Per-type counts are filled from annotation details
                // by the detail view after the second request.
                highlightsCount: 0,
                notesCount: 0
            )
        }
    }

    func annotations(for assetID: String) async throws -> [Annotation] {
        let result = try await run(arguments: [
            "annotations", "--asset-id", assetID, "--json"
        ])
        let response: AnnotationResponse = try decodeResponse(from: result.stdout)

        guard response.assetID == assetID else {
            throw RustCLIError.invalidResponse(
                "响应 asset_id 与请求不一致：\(response.assetID)"
            )
        }
        guard response.annotationCount >= 0,
              response.annotationCount == response.annotations.count else {
            throw RustCLIError.invalidResponse("annotation_count 与 annotations 数量不一致")
        }

        return try response.annotations.map { annotation in
            guard let type = AnnotationType(rawValue: annotation.type) else {
                throw RustCLIError.invalidResponse(
                    "不支持的标注类型：\(annotation.type)"
                )
            }

            return Annotation(
                id: annotation.id,
                type: type,
                chapterTitle: annotation.chapterTitle ?? "",
                locationInfo: annotation.location ?? "",
                contentText: annotation.contentText,
                noteText: annotation.noteText,
                createdAt: try parseDate(annotation.createdAt)
            )
        }
    }

    func export(
        assetID: String,
        outputDirectory: URL,
        format: String = "obsidian",
        overwrite: Bool = false
    ) async throws -> RustExportReceipt {
        var arguments = [
            "export", "--asset-id", assetID, "--json",
            "--output", outputDirectory.path,
            "--format", format
        ]
        if overwrite {
            arguments.append("--overwrite")
        }

        let result = try await run(arguments: arguments)
        let response: ExportResponse = try decodeResponse(from: result.stdout)
        guard response.receipt.assetID == assetID else {
            throw RustCLIError.invalidResponse(
                "导出 receipt 的 asset_id 与请求不一致：\(response.receipt.assetID)"
            )
        }
        guard response.receipt.annotationCount >= 0 else {
            throw RustCLIError.invalidResponse("receipt annotation_count 不能为负数")
        }

        return RustExportReceipt(
            assetID: response.receipt.assetID,
            title: response.receipt.title,
            annotationCount: response.receipt.annotationCount,
            format: response.receipt.format,
            outputDirectory: response.receipt.outputDirectory,
            generatedFiles: response.receipt.generatedFiles
        )
    }

    private func run(arguments: [String]) async throws -> RustCLICommandResult {
        guard let executableURL else {
            throw RustCLIError.binaryNotFound(path: "(未配置)")
        }

        do {
            let result = try await runner(executableURL, arguments)
            guard result.terminationStatus == 0 else {
                throw parseCommandError(result)
            }
            return result
        } catch let error as RustCLIError {
            throw error
        } catch {
            throw RustCLIError.launchFailed(details: error.localizedDescription)
        }
    }

    private func decodeResponse<Response: Decodable>(from data: Data) throws -> Response {
        guard !data.isEmpty else {
            throw RustCLIError.invalidResponse("stdout 为空")
        }

        do {
            let envelope = try JSONDecoder().decode(SchemaEnvelope.self, from: data)
            guard envelope.schemaVersion == Self.supportedSchemaVersion else {
                throw RustCLIError.unsupportedSchemaVersion(envelope.schemaVersion)
            }
            return try JSONDecoder().decode(Response.self, from: data)
        } catch let error as RustCLIError {
            throw error
        } catch {
            throw RustCLIError.invalidResponse(error.localizedDescription)
        }
    }

    private func parseCommandError(_ result: RustCLICommandResult) -> RustCLIError {
        let fallbackMessage = String(data: result.stderr, encoding: .utf8)?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard let fallbackMessage, !fallbackMessage.isEmpty else {
            return .commandFailed(
                status: result.terminationStatus,
                code: "BACKEND_UNAVAILABLE",
                message: "Rust 导出器退出码为 \(result.terminationStatus)，但没有返回错误信息。",
                remediation: "检查 bundled Rust binary 和 Apple Books 权限后重试。"
            )
        }

        do {
            let envelope = try JSONDecoder().decode(SchemaEnvelope.self, from: result.stderr)
            guard envelope.schemaVersion == Self.supportedSchemaVersion else {
                return .unsupportedSchemaVersion(envelope.schemaVersion)
            }

            let response = try JSONDecoder().decode(ErrorResponse.self, from: result.stderr)
            guard !response.error.code.isEmpty, !response.error.message.isEmpty else {
                throw RustCLIError.invalidResponse("结构化错误缺少 code 或 message")
            }
            return .commandFailed(
                status: result.terminationStatus,
                code: response.error.code,
                message: response.error.message,
                remediation: response.error.remediation
            )
        } catch let error as RustCLIError {
            switch error {
            case .unsupportedSchemaVersion:
                return error
            default:
                break
            }
        } catch {
            // A non-zero process with non-JSON stderr is still a backend
            // failure.  It must not be treated as a successful empty result.
        }

        return .commandFailed(
            status: result.terminationStatus,
            code: "BACKEND_UNAVAILABLE",
            message: fallbackMessage,
            remediation: "检查 bundled Rust binary 和 Apple Books 权限后重试。"
        )
    }

    private func parseDate(_ value: String?) throws -> Date? {
        guard let value else { return nil }

        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime]
        if let date = formatter.date(from: value) {
            return date
        }

        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        guard let date = formatter.date(from: value) else {
            throw RustCLIError.invalidResponse("created_at 不是有效的 ISO-8601 时间：\(value)")
        }
        return date
    }
}

private struct SchemaEnvelope: Decodable {
    let schemaVersion: Int

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
    }
}

private struct BookListResponse: Decodable {
    let schemaVersion: Int
    let books: [BookDTO]

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case books
    }
}

private struct BookDTO: Decodable {
    let assetID: String
    let title: String
    let author: String
    let noteCount: Int

    enum CodingKeys: String, CodingKey {
        case assetID = "asset_id"
        case title
        case author
        case noteCount = "note_count"
    }
}

private struct AnnotationResponse: Decodable {
    let schemaVersion: Int
    let assetID: String
    let title: String
    let author: String
    let annotationCount: Int
    let annotations: [AnnotationDTO]

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case assetID = "asset_id"
        case title
        case author
        case annotationCount = "annotation_count"
        case annotations
    }
}

private struct AnnotationDTO: Decodable {
    let id: String
    let type: String
    let contentText: String?
    let noteText: String?
    let chapterTitle: String?
    let location: String?
    let createdAt: String?

    enum CodingKeys: String, CodingKey {
        case id
        case type
        case contentText = "content_text"
        case noteText = "note_text"
        case chapterTitle = "chapter_title"
        case location
        case createdAt = "created_at"
    }
}

private struct ErrorResponse: Decodable {
    let schemaVersion: Int
    let error: MachineErrorDTO

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case error
    }
}

private struct MachineErrorDTO: Decodable {
    let code: String
    let message: String
    let remediation: String?
}

private struct ExportResponse: Decodable {
    let schemaVersion: Int
    let receipt: ExportReceiptDTO

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case receipt
    }
}

private struct ExportReceiptDTO: Decodable {
    let assetID: String
    let title: String
    let annotationCount: Int
    let format: String
    let outputDirectory: String
    let generatedFiles: [String]

    enum CodingKeys: String, CodingKey {
        case assetID = "asset_id"
        case title
        case annotationCount = "annotation_count"
        case format
        case outputDirectory = "output_directory"
        case generatedFiles = "generated_files"
    }
}

private enum RustCLIProcessRunner {
    static func run(
        executableURL: URL,
        arguments: [String]
    ) async throws -> RustCLICommandResult {
        try await Task.detached(priority: .userInitiated) {
            try runSynchronously(executableURL: executableURL, arguments: arguments)
        }.value
    }

    private static func runSynchronously(
        executableURL: URL,
        arguments: [String]
    ) throws -> RustCLICommandResult {
        guard FileManager.default.isExecutableFile(atPath: executableURL.path) else {
            throw RustCLIError.binaryNotFound(path: executableURL.path)
        }

        let process = Process()
        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        process.executableURL = executableURL
        process.arguments = arguments
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe

        do {
            try process.run()
        } catch {
            let details = error.localizedDescription
            let lowercased = details.lowercased()
            if lowercased.contains("bad cpu")
                || lowercased.contains("exec format")
                || lowercased.contains("wrong architecture") {
                throw RustCLIError.binaryIncompatible(details: details)
            }
            throw RustCLIError.launchFailed(details: details)
        }

        let group = DispatchGroup()
        let stdoutBox = DataBox()
        let stderrBox = DataBox()

        group.enter()
        DispatchQueue.global(qos: .userInitiated).async {
            stdoutBox.data = stdoutPipe.fileHandleForReading.readDataToEndOfFile()
            group.leave()
        }

        group.enter()
        DispatchQueue.global(qos: .userInitiated).async {
            stderrBox.data = stderrPipe.fileHandleForReading.readDataToEndOfFile()
            group.leave()
        }

        process.waitUntilExit()
        group.wait()

        return RustCLICommandResult(
            stdout: stdoutBox.data,
            stderr: stderrBox.data,
            terminationStatus: process.terminationStatus
        )
    }
}

private final class DataBox: @unchecked Sendable {
    var data = Data()
}
