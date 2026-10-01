import Foundation

// A literal mirror of the Rust speech machine protocol.
//
// Every type here exists because `src/speech/machine.rs` emits it. The field
// names are the contract; renaming one here without renaming it there is a
// silent decode failure, not a compile error, so the Rust side is the reference
// and this file quotes it rather than paraphrasing it.
//
// Two properties of the contract shape the whole file and are easy to get
// wrong:
//
// 1. Success JSON goes to stdout, failure JSON to stderr, and failure exits
//    non-zero. There is no `ok` boolean -- success and failure are
//    discriminated by the presence of `receipt` versus `error`. An empty
//    stdout is therefore not evidence of success, and not evidence of failure.
// 2. `speech play --json` never plays. The CLI injects a player that traps if
//    called, so `played` is always false and the receipt's `path` is what a
//    client acts on. Playback itself is the client's job.

/// The discriminant between the two response shapes.
struct SpeechEnvelopeProbe: Decodable {
    let schemaVersion: Int
    let hasReceipt: Bool
    let hasError: Bool

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case receipt
        case error
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        schemaVersion = try container.decode(Int.self, forKey: .schemaVersion)
        hasReceipt = container.contains(.receipt)
        hasError = container.contains(.error)
    }
}

// MARK: - Errors

/// A speech failure. `outcome` is the field that decides whether money was
/// spent, and it is deliberately not derivable from `code`.
struct SpeechError: LocalizedError, Equatable {
    /// The contract's `outcome` discriminator.
    ///
    /// `failed` means the provider did not produce a billable result.
    /// `unknown` and `provider_succeeded_artifact_missing` both mean it may
    /// already have been billed -- the provider ran, and the audio either never
    /// arrived or arrived but could not be committed. Retrying either without
    /// explicit consent risks paying twice.
    enum Outcome: String, Equatable, Decodable {
        case failed
        case unknown
        case providerSucceededArtifactMissing = "provider_succeeded_artifact_missing"
    }

    let code: String
    let message: String
    let remediation: String?
    let details: SpeechErrorDetails?

    /// True when the provider may already have billed for this attempt.
    var mayHaveBeenBilled: Bool {
        guard let outcome = details?.outcome else {
            // No outcome at all: a failure detected before the request could
            // reach the provider. `generate` checks its credential, storage
            // budget and voice catalog in that order, all before opening a
            // connection, so those carry no outcome and no charge.
            return false
        }
        return outcome != .failed
    }

    /// Whether a plain retry is safe. Anything that may have been billed needs
    /// `--regenerate` and therefore explicit consent.
    var isSafeToRetryWithoutRegenerate: Bool {
        switch code {
        case "SPEECH_RATE_LIMITED", "SPEECH_IN_PROGRESS":
            return true
        default:
            return !mayHaveBeenBilled
        }
    }

    var errorDescription: String? { message }

    var userFacingDescription: String {
        var description = "[\(code)] \(message)"
        if let remediation, !remediation.isEmpty {
            description += "\n\n\(remediation)"
        }
        return description
    }
}

struct SpeechErrorDetails: Decodable, Equatable {
    let provider: String?
    let reason: String?
    let outcome: SpeechError.Outcome?
    let traceID: String?
    let attemptID: String?
    let field: String?
    let value: String?
    let voiceReason: String?
    let rejectedCandidates: [RejectedExportCandidate]?

    enum CodingKeys: String, CodingKey {
        case provider
        case reason
        case outcome
        case traceID = "trace_id"
        case attemptID = "attempt_id"
        case field
        case value
        case voiceReason = "voice_reason"
        case rejectedCandidates = "rejected_candidates"
    }

    /// `details.value` is a string for some errors and a number for others
    /// (a rejected `--speed 3.5` versus a rejected `voice_id`). Decoding it as
    /// a plain string would drop the whole `details` object on a numeric
    /// error, and `details` is exactly where the retry decision lives.
    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        provider = try container.decodeIfPresent(String.self, forKey: .provider)
        reason = try container.decodeIfPresent(String.self, forKey: .reason)
        outcome = try container.decodeIfPresent(SpeechError.Outcome.self, forKey: .outcome)
        traceID = try container.decodeIfPresent(String.self, forKey: .traceID)
        attemptID = try container.decodeIfPresent(String.self, forKey: .attemptID)
        field = try container.decodeIfPresent(String.self, forKey: .field)
        voiceReason = try container.decodeIfPresent(String.self, forKey: .voiceReason)
        rejectedCandidates = try container.decodeIfPresent(
            [RejectedExportCandidate].self,
            forKey: .rejectedCandidates
        )

        if let text = try? container.decode(String.self, forKey: .value) {
            value = text
        } else if let number = try? container.decode(Double.self, forKey: .value) {
            // Formatted without a trailing `.0` so a rejected `--pitch 0`
            // does not read back as "0.0" in the error the user sees.
            value = number == number.rounded()
                ? String(Int(number))
                : String(number)
        } else if let flag = try? container.decode(Bool.self, forKey: .value) {
            value = String(flag)
        } else {
            value = nil
        }
    }
}

/// Which export roots `speech play` tried and why each was rejected.
struct RejectedExportCandidate: Decodable, Equatable {
    let code: String
    let reason: String
    let message: String
}

struct SpeechErrorResponse: Decodable {
    let schemaVersion: Int
    let error: SpeechErrorBody

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case error
    }
}

struct SpeechErrorBody: Decodable {
    let code: String
    let message: String
    let remediation: String?
    let details: SpeechErrorDetails?
}

// MARK: - Warnings

struct SpeechWarning: Decodable, Equatable {
    let code: String
    let reason: String?
    let message: String?
}

// MARK: - Voice catalog

struct VoiceCatalogResponse: Decodable {
    let schemaVersion: Int
    let receipt: VoiceCatalogReceipt

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case receipt
    }
}

struct VoiceCatalogReceipt: Decodable {
    let operation: String
    let provider: String
    let fetchedAt: String?
    let stale: Bool
    let voices: [VoiceCatalogVoice]
    let warnings: [SpeechWarning]?

    enum CodingKeys: String, CodingKey {
        case operation
        case provider
        case fetchedAt = "fetched_at"
        case stale
        case voices
        case warnings
    }
}

struct VoiceCatalogVoice: Decodable, Equatable, Identifiable {
    let provider: String
    let sourceType: String
    let voiceID: String
    let voiceName: String?
    let emotionLabel: String?
    let styleLabel: String?
    let description: [String]
    let createdTime: String?

    var id: String { voiceID }

    enum CodingKeys: String, CodingKey {
        case provider
        case sourceType = "source_type"
        case voiceID = "voice_id"
        case voiceName = "voice_name"
        case emotionLabel = "emotion_label"
        case styleLabel = "style_label"
        case description
        case createdTime = "created_time"
    }
}

// MARK: - Voice profile

struct SpeechProfileResponse: Decodable {
    let schemaVersion: Int
    let receipt: SpeechProfileReceipt

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case receipt
    }
}

struct SpeechProfileReceipt: Decodable {
    let operation: String
    let profile: SpeechProfile
    /// The *name* of the environment variable, never its value.
    let apiKeyEnv: String?
    let configPath: String?
    let warnings: [SpeechWarning]?

    enum CodingKeys: String, CodingKey {
        case operation
        case profile
        case apiKeyEnv = "api_key_env"
        case configPath = "config_path"
        case warnings
    }
}

struct SpeechProfile: Decodable, Equatable {
    let provider: String
    let model: String
    let voiceID: String?
    let emotionLabel: String?
    let styleLabel: String?
    let speed: Double
    let volume: Double
    let pitch: Int
    let verificationStatus: String
    let verifiedAt: String?
    let audio: SpeechAudioSpec?

    enum CodingKeys: String, CodingKey {
        case provider
        case model
        case voiceID = "voice_id"
        case emotionLabel = "emotion_label"
        case styleLabel = "style_label"
        case speed
        case volume
        case pitch
        case verificationStatus = "verification_status"
        case verifiedAt = "verified_at"
        case audio
    }
}

/// The audio format contract. `path`, `sha256`, `sizeBytes` and `durationMS`
/// are omitted rather than nulled in profile receipts, so every field beyond
/// the format itself is optional.
struct SpeechAudioSpec: Decodable, Equatable {
    let path: String?
    let sha256: String?
    let sizeBytes: Int?
    let durationMS: Int?
    let format: String
    let sampleRate: Int?
    let bitrate: Int?
    let channel: Int?

    enum CodingKeys: String, CodingKey {
        case path
        case sha256
        case sizeBytes = "size_bytes"
        case durationMS = "duration_ms"
        case format
        case sampleRate = "sample_rate"
        case bitrate
        case channel
    }
}

// MARK: - Generate

struct SpeechGenerateResponse: Decodable {
    let schemaVersion: Int
    let receipt: SpeechGenerateReceipt

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case receipt
    }
}

struct SpeechGenerateReceipt: Decodable {
    let operation: String
    let clipID: String
    let attemptID: String?
    let source: String
    let providerCalled: Bool
    let assetID: String
    let annotationID: String
    let contentKind: String
    let textSHA256: String?
    let unicodeCharacters: Int
    let estimatedBillingCharacters: Int
    let billingEstimatorVersion: String?
    let profile: SpeechProfile?
    let audio: SpeechAudioSpec?
    let provider: SpeechProviderUsage?
    let warnings: [SpeechWarning]?

    enum CodingKeys: String, CodingKey {
        case operation
        case clipID = "clip_id"
        case attemptID = "attempt_id"
        case source
        case providerCalled = "provider_called"
        case assetID = "asset_id"
        case annotationID = "annotation_id"
        case contentKind = "content_kind"
        case textSHA256 = "text_sha256"
        case unicodeCharacters = "unicode_characters"
        case estimatedBillingCharacters = "estimated_billing_characters"
        case billingEstimatorVersion = "billing_estimator_version"
        case profile
        case audio
        case provider
        case warnings
    }

    /// The authoritative "money was spent" signal. A cache hit and an export
    /// rehydration both succeed with `providerCalled == false`.
    var wasBilled: Bool { providerCalled }
}

struct SpeechProviderUsage: Decodable, Equatable {
    let traceID: String?
    let usageCharacters: Int?

    enum CodingKeys: String, CodingKey {
        case traceID = "trace_id"
        case usageCharacters = "usage_characters"
    }
}

// MARK: - Play

struct SpeechPlayResponse: Decodable {
    let schemaVersion: Int
    let receipt: SpeechPlayReceipt

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case receipt
    }
}

struct SpeechPlayReceipt: Decodable {
    let operation: String
    let clipID: String
    let source: String
    /// Always false under `--json`; the CLI injects a player that traps.
    /// Present so a client can assert the assumption rather than trust it.
    let played: Bool
    let providerCalled: Bool
    let assetID: String
    let annotationID: String
    let contentKind: String
    let path: String
    let audio: SpeechAudioSpec?
    let warnings: [SpeechWarning]?
    let exportOrigin: String?

    enum CodingKeys: String, CodingKey {
        case operation
        case clipID = "clip_id"
        case source
        case played
        case providerCalled = "provider_called"
        case assetID = "asset_id"
        case annotationID = "annotation_id"
        case contentKind = "content_kind"
        case path
        case audio
        case warnings
        case exportOrigin = "export_origin"
    }
}

/// `speech cache status --json` 的成功响应。
///
/// 只读的本地视图：零 provider 连接，零凭据。
struct SpeechCacheStatusResponse: Decodable {
    let schemaVersion: Int
    let receipt: SpeechCacheStatusReceipt

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case receipt
    }
}

/// 缓存状态收据。
///
/// 只解码面板真正用到的部分。收据里的预算与异常统计是给 `speech cache status`
/// 的**人读**输出用的，面板一个都不显示；把 `budget_bytes`、`reclaimable_versions`
/// 之类照抄进来只会让每次协议微调都要改这份 Swift，而面板一行都用不到。
struct SpeechCacheStatusReceipt: Decodable {
    let operation: String
    let entries: [SpeechCacheEntry]

    enum CodingKeys: String, CodingKey {
        case operation
        case entries
    }
}

/// 一条 clip 的缓存状态明细。
///
/// 归属字段（`asset_id` / `annotation_id` / `content_kind`）在状态不可信时是
/// `null`，这是**故意**的：Rust 侧不回退到 state，因为一条自相矛盾的归属比没有
/// 归属更危险——它会让面板把别人的音频列到这个标注名下。这里保留 `null` 而不是
/// 补一个默认值，过滤时它们自然落选，见
/// ``[SpeechCacheEntry]/playable(forAssetID:annotationID:)``。
struct SpeechCacheEntry: Decodable, Equatable {
    let clipID: String
    let assetID: String?
    let annotationID: String?
    let contentKind: String?
    /// 当前版本的音频时长（毫秒）；没有已校验音频时为 `null`。
    let durationMs: Int?
    /// `ready` / `absent` / `corrupt`。
    let status: String
    let accepted: Bool

    enum CodingKeys: String, CodingKey {
        case clipID = "clip_id"
        case assetID = "asset_id"
        case annotationID = "annotation_id"
        case contentKind = "content_kind"
        case durationMs = "duration_ms"
        case status
        case accepted
    }
}

/// 一条属于本标注、且**现在就能播放**的音频。
///
/// 面板显示和操作的最小单位。刻意不带音色与音调：缓存条目里没有这些字段
/// （`clip_id` 是它们的指纹，算法归 Rust 所有），所以面板不能声称"这条是按
/// 当前参数做的"。见 `updateCostNotice` 那里为什么只能给条件式说法。
struct SpeechClipSummary: Equatable {
    let clipID: String
    /// `highlight` 或 `note`。
    let contentKind: String
    let durationMs: Int?
}

extension Array where Element == SpeechCacheEntry {
    /// The clips that belong to one annotation and can actually be played.
    ///
    /// Three filters, each for its own reason:
    ///
    /// - **identity** -- the entry's own `asset_id` / `annotation_id` must match.
    ///   They are `null` when the entry is untrustworthy, and a `null` never
    ///   equals anything, so untrustworthy entries drop out here. Falling back
    ///   to the state file instead would be the bug: a `corrupt` row whose
    ///   identity came from somewhere else would be filed under the wrong
    ///   annotation, and the user would play -- and export -- the wrong audio.
    /// - **status** -- only `ready`. `absent` and `corrupt` have no verified
    ///   audio behind the clip ID, so the play and export controls have nothing
    ///   to act on; listing them would offer an action that can only fail.
    /// - **order** -- the contract already sorts by clip ID, and this keeps that
    ///   order so a reopened panel shows the same list in the same order.
    func playable(forAssetID assetID: String, annotationID: String) -> [SpeechClipSummary] {
        filter { entry in
            entry.status == "ready"
                && entry.assetID == assetID
                && entry.annotationID == annotationID
                && entry.contentKind != nil
        }
        .map {
            SpeechClipSummary(
                clipID: $0.clipID,
                contentKind: $0.contentKind ?? "highlight",
                durationMs: $0.durationMs
            )
        }
    }
}

/// `speech export` 的回执。ADR 0007 规定它只含稳定身份、相对路径、checksum、
/// 大小、格式和导出时间——不含 Speech Text、API Key 或供应商原始响应。
struct SpeechExportResponse: Decodable {
    let schemaVersion: Int
    let receipt: SpeechExportReceipt

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case receipt
    }
}

struct SpeechExportReceipt: Decodable {
    let operation: String
    let clipID: String
    let assetID: String
    let annotationID: String
    let contentKind: String
    /// 相对书籍导出根目录的路径。
    let relativePath: String
    /// 已校验音频的绝对路径。
    let path: String
    let sha256: String
    let sizeBytes: Int
    let format: String
    let exportedAt: String
    /// 目标文件字节一致、直接复用而没有重写。
    let reused: Bool
    /// 本次显式替换了内容不同的已有文件。
    let replaced: Bool
    /// 恒为 `false`：任何 export 路径都不联系供应商。
    let providerCalled: Bool
    let warnings: [SpeechWarning]?

    enum CodingKeys: String, CodingKey {
        case operation
        case clipID = "clip_id"
        case assetID = "asset_id"
        case annotationID = "annotation_id"
        case contentKind = "content_kind"
        case relativePath = "relative_path"
        case path
        case sha256
        case sizeBytes = "size_bytes"
        case format
        case exportedAt = "exported_at"
        case reused
        case replaced
        case providerCalled = "provider_called"
        case warnings
    }
}
