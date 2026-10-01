import Foundation

/// Resolves the environment the speech commands run with.
///
/// The split matters and is the whole reason this type exists. The Rust side
/// knows the *name* of the variable to read -- `speech profile show --json`
/// returns `api_key_env`, currently `SENSEAUDIO_API_KEY` -- and knows nothing
/// about where the value comes from. The value lives in the Keychain, because
/// ADR 0007 keeps secrets out of the config file entirely. Neither side holds
/// both halves, so the app is the only place they can be joined.
///
/// An actor rather than a lock: the cached name is filled by an `await`, and
/// taking an `NSLock` on either side of a suspension point is both a Swift 6
/// error and a way to interleave two profile lookups.
actor SpeechCredentialResolver {
    private let client: RustCLIClient
    private let keychain: KeychainStoring
    private var cachedEnvironmentName: String?

    /// Used only when the CLI has not yet told us the variable name. A wrong
    /// guess produces a `SPEECH_AUTH_FAILED` naming the right variable, so the
    /// failure is legible rather than mysterious.
    static let defaultEnvironmentName = "SENSEAUDIO_API_KEY"

    init(client: RustCLIClient, keychain: KeychainStoring) {
        self.client = client
        self.keychain = keychain
    }

    /// The name of the environment variable the CLI will read.
    func environmentName() async throws -> String {
        if let cached = cachedEnvironmentName {
            return cached
        }

        // `profile show` never needs a credential, so this works on a fresh
        // install and is the only way to learn the real name.
        let response = try await SpeechService(client: client).profile()
        let name = response.receipt.apiKeyEnv ?? Self.defaultEnvironmentName
        cachedEnvironmentName = name
        return name
    }

    /// The environment to launch speech commands with, or nil when no key is
    /// stored -- in which case the CLI still runs and reports
    /// `SPEECH_AUTH_FAILED` with a remediation naming the variable, instead of
    /// the app guessing or pre-empting that message.
    func environment() async throws -> [String: String]? {
        let name = try await environmentName()
        guard let value = try keychain.secret(forKey: name), !value.isEmpty else {
            return nil
        }
        return [name: value]
    }

    func store(_ value: String) async throws {
        let name = try await environmentName()
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty {
            try keychain.removeSecret(forKey: name)
        } else {
            try keychain.setSecret(trimmed, forKey: name)
        }
    }

    func clear() async throws {
        let name = try await environmentName()
        try keychain.removeSecret(forKey: name)
    }

    /// Whether a key is stored. Reports presence only -- never the value, and
    /// never a length, which would be a slow oracle.
    func hasStoredSecret() async -> Bool {
        guard let name = try? await environmentName() else { return false }
        guard let value = try? keychain.secret(forKey: name) else { return false }
        return !value.isEmpty
    }
}

/// AppKit's typed access to the speech command family.
///
/// This is a thin, faithful pass-through. It does not decide voices, does not
/// pick a fallback, and does not retry: those are product decisions that
/// ADR 0007 puts in the GUI layer, and the CLI already encodes the rules this
/// type only surfaces.
struct SpeechService {
    let client: RustCLIClient
    private let credentialResolver: SpeechCredentialResolver?

    init(client: RustCLIClient, credentialResolver: SpeechCredentialResolver? = nil) {
        self.client = client
        self.credentialResolver = credentialResolver
    }

    private static let supportedSchemaVersion = 1

    private func environment() async throws -> [String: String]? {
        guard let credentialResolver else { return nil }
        return try await credentialResolver.environment()
    }

    /// Run one speech command and return its receipt.
    ///
    /// Both streams are consulted, because the contract puts success JSON on
    /// stdout and failure JSON on stderr. An empty stdout is treated as a
    /// malformed response rather than as an empty result.
    /// Copies a verified clip into the book's export directory and refreshes the
    /// Speech Export Manifest.
    ///
    /// Not credentialed, and not because the key is optional: ADR 0007 makes
    /// every export path local -- `provider_called` is always false and the
    /// receipt says so, so a client can assert it rather than assume it. The
    /// directory is the book's export root, not the audio folder inside it;
    /// that is the CLI's parameter contract and the panel passes it through
    /// unchanged.
    func export(
        clipID: String,
        to exportRoot: String,
        overwrite: Bool = false
    ) async throws -> SpeechExportReceipt {
        var arguments = [
            "speech", "export", "--json",
            "--clip-id", clipID,
            "--output", exportRoot,
        ]
        if overwrite { arguments.append("--overwrite") }
        let result = try await receipt(
            SpeechExportResponse.self,
            arguments: arguments,
            credentialed: false
        )
        // The contract states this is always false. Asserting it costs nothing
        // and turns a contract change into a visible failure rather than a
        // surprise bill.
        guard !result.receipt.providerCalled else {
            throw SpeechServiceError.malformedResponse(
                "speech export 报告联系了语音供应商，导出路径不应联网"
            )
        }
        return result.receipt
    }

    private func receipt<Response: Decodable>(
        _ type: Response.Type,
        arguments: [String],
        credentialed: Bool
    ) async throws -> Response {
        let environment = credentialed ? try await environment() : nil
        let result = try await client.execute(arguments: arguments, environment: environment)

        guard result.terminationStatus == 0 else {
            throw SpeechServiceError.commandFailed(try parseSpeechError(from: result.stderr))
        }
        return try decode(type, from: result.stdout)
    }

    /// The one place speech failures are turned into a value. Kept separate
    /// from `RustCLIClient.parseCommandError` because speech errors carry
    /// `details`, and a non-JSON stderr must still produce a usable error
    /// rather than a decode message about a missing `code`.
    private func parseSpeechError(from stderr: Data) throws -> SpeechError {
        guard !stderr.isEmpty else {
            return SpeechError(
                code: "BACKEND_UNAVAILABLE",
                message: "语音命令以非零状态退出，但没有返回错误信息。",
                remediation: "检查 bundled Rust binary 与 Apple Books 权限后重试。",
                details: nil
            )
        }

        do {
            let response = try JSONDecoder().decode(SpeechErrorResponse.self, from: stderr)
            return SpeechError(
                code: response.error.code,
                message: response.error.message,
                remediation: response.error.remediation,
                details: response.error.details
            )
        } catch {
            let text = String(data: stderr, encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            return SpeechError(
                code: "BACKEND_UNAVAILABLE",
                message: text.isEmpty ? "语音命令返回了无法解析的错误。" : text,
                remediation: "检查 bundled Rust binary 与 Apple Books 权限后重试。",
                details: nil
            )
        }
    }

    /// The voice catalog. This is the only speech command besides `generate`
    /// that touches the network, and it is free.
    func voices(refresh: Bool = false) async throws -> VoiceCatalogResponse {
        var arguments = ["speech", "voices", "--json"]
        if refresh {
            arguments.append("--refresh")
        }
        return try await receipt(
            VoiceCatalogResponse.self,
            arguments: arguments,
            credentialed: true
        )
    }

    /// The stored Voice Profile. Never needs a credential, so it is also the
    /// probe for whether the CLI is reachable at all.
    func profile() async throws -> SpeechProfileResponse {
        try await receipt(
            SpeechProfileResponse.self,
            arguments: ["speech", "profile", "show", "--json"],
            credentialed: false
        )
    }

    /// Generates and caches one Speech Clip.
    ///
    /// This is the one billed operation. `parameters` map onto the CLI's
    /// override flags; passing a different voice or tone yields a different
    /// `clip_id` and therefore a separate charge rather than a re-render, so
    /// callers show the resolved parameters before calling.
    func generate(
        assetID: String,
        annotationID: String,
        contentKind: String,
        voiceID: String? = nil,
        speed: Double? = nil,
        volume: Double? = nil,
        pitch: Int? = nil,
        regenerate: Bool = false,
        exportRoot: String? = nil
    ) async throws -> SpeechGenerateResponse {
        var arguments = [
            "speech", "generate", "--json",
            "--asset-id", assetID,
            "--annotation-id", annotationID,
            "--content", contentKind
        ]
        if let voiceID { arguments.append(contentsOf: ["--voice-id", voiceID]) }
        if let speed { arguments.append(contentsOf: ["--speed", Self.format(speed)]) }
        if let volume { arguments.append(contentsOf: ["--volume", Self.format(volume)]) }
        if let pitch { arguments.append(contentsOf: ["--pitch", String(pitch)]) }
        if regenerate { arguments.append("--regenerate") }
        if let exportRoot { arguments.append(contentsOf: ["--export-root", exportRoot]) }

        return try await receipt(
            SpeechGenerateResponse.self,
            arguments: arguments,
            credentialed: true
        )
    }

    /// The read-only cache view: every clip on this machine, with the identity
    /// the panel needs to answer "has this annotation been generated before".
    ///
    /// Not credentialed, and the `cache status` command is not a network path
    /// at all -- the contract states zero provider calls. That matters for the
    /// panel: listing what already exists must work before a key is configured
    /// and without spending anything.
    func cacheStatus() async throws -> [SpeechCacheEntry] {
        let result = try await receipt(
            SpeechCacheStatusResponse.self,
            arguments: ["speech", "cache", "status", "--json"],
            credentialed: false
        )
        return result.receipt.entries
    }

    /// Resolves a clip to a verified audio path. Does not play: the machine
    /// entry point is contractually silent and `played` is always false.
    func play(clipID: String, exportRoot: String? = nil) async throws -> SpeechPlayReceipt {
        var arguments = ["speech", "play", "--json", "--clip-id", clipID]
        if let exportRoot { arguments.append(contentsOf: ["--export-root", exportRoot]) }
        let result = try await receipt(
            SpeechPlayResponse.self,
            arguments: arguments,
            credentialed: false
        )
        guard !result.receipt.played else {
            // A machine-mode receipt that claims to have played audio would mean
            // the contract changed under us. Refusing is safer than silently
            // starting a second playback path.
            throw SpeechServiceError.malformedResponse(
                "speech play --json 报告了已播放，机器模式不应发声"
            )
        }
        return result.receipt
    }

    private func decode<Response: Decodable>(_ type: Response.Type, from data: Data) throws -> Response {
        guard !data.isEmpty else {
            throw SpeechServiceError.emptyResponse
        }

        do {
            let envelope = try JSONDecoder().decode(SpeechEnvelopeProbe.self, from: data)
            guard envelope.schemaVersion == Self.supportedSchemaVersion else {
                throw SpeechServiceError.unsupportedSchemaVersion(envelope.schemaVersion)
            }
            guard envelope.hasReceipt, !envelope.hasError else {
                throw SpeechServiceError.malformedResponse(
                    envelope.hasError
                        ? "speech 命令返回了错误信封但退出码为 0"
                        : "speech 命令返回了成功信封但退出码非 0"
                )
            }
            return try JSONDecoder().decode(Response.self, from: data)
        } catch let error as SpeechServiceError {
            throw error
        } catch {
            throw SpeechServiceError.malformedResponse(error.localizedDescription)
        }
    }

    /// Numbers go out in the CLI's own accepted form: at most two decimals, no
    /// exponent, no locale separator. `1,5` is rejected by the CLI as a profile
    /// error, which is a far worse report than never sending it.
    private static func format(_ value: Double) -> String {
        var text = String(format: "%.2f", value)
        while text.hasSuffix("0") { text.removeLast() }
        if text.hasSuffix(".") { text.removeLast() }
        return text
    }
}

enum SpeechServiceError: LocalizedError, Equatable {
    case emptyResponse
    case malformedResponse(String)
    case unsupportedSchemaVersion(Int)
    case commandFailed(SpeechError)

    var errorDescription: String? {
        switch self {
        case .emptyResponse:
            return "语音命令没有返回内容"
        case .malformedResponse(let details):
            return "语音命令返回了无法解析的机器响应：\(details)"
        case .unsupportedSchemaVersion(let version):
            return "语音机器协议版本 \(version) 不受支持"
        case .commandFailed(let error):
            return error.userFacingDescription
        }
    }
}
