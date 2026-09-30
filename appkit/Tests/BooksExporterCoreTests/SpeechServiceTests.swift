import XCTest
@testable import BooksExporterCore

/// Exercises the speech protocol against recorded contract shapes.
///
/// Nothing here launches a process or contacts a provider: the runner is
/// replaced with a recorder, which is also what makes it possible to assert the
/// two properties that matter most and are easiest to lose -- that the
/// credential reaches the child environment and nowhere else, and that a
/// failure's `details.outcome` survives the trip out of stderr.
final class SpeechServiceTests: XCTestCase {

    // MARK: - Fixtures

    /// Shapes quoted from `src/speech/machine.rs`. `speed` is a JSON number and
    /// the audio block omits its file fields in a profile receipt -- both are
    /// easy to get wrong and neither is caught by a type that assumes symmetry.
    private static let profileJSON = """
    {
      "schema_version": 1,
      "receipt": {
        "operation": "profile_show",
        "profile": {
          "provider": "senseaudio",
          "model": "sensenova-tts-2.0",
          "voice_id": "male_0004_a",
          "emotion_label": null,
          "style_label": null,
          "speed": 1.0,
          "volume": 1.0,
          "pitch": 0,
          "verification_status": "unverified",
          "verified_at": null,
          "audio": { "format": "mp3", "sample_rate": 32000, "bitrate": 128000, "channel": 2 }
        },
        "api_key_env": "SENSEAUDIO_API_KEY",
        "config_path": "/Users/x/Library/Application Support/books-exporter/speech/config.json",
        "warnings": []
      }
    }
    """

    private static let voicesJSON = """
    {
      "schema_version": 1,
      "receipt": {
        "operation": "voices",
        "provider": "senseaudio",
        "fetched_at": "2026-09-30T07:38:38+00:00",
        "stale": false,
        "voices": [
          {
            "provider": "senseaudio",
            "source_type": "system",
            "voice_id": "female_0033_b",
            "voice_name": "女声 0033",
            "emotion_label": "开心",
            "style_label": null,
            "description": ["开心"],
            "created_time": null
          }
        ],
        "warnings": []
      }
    }
    """

    private static let generateProviderJSON = """
    {
      "schema_version": 1,
      "receipt": {
        "operation": "generate",
        "clip_id": "\(String(repeating: "a", count: 64))",
        "attempt_id": "attempt-1",
        "source": "provider",
        "provider_called": true,
        "asset_id": "book-1",
        "annotation_id": "annotation-41",
        "content_kind": "highlight",
        "text_sha256": "\(String(repeating: "b", count: 64))",
        "unicode_characters": 12,
        "estimated_billing_characters": 12,
        "billing_estimator_version": "senseaudio-docs-2026-09-10",
        "profile": {
          "provider": "senseaudio", "model": "sensenova-tts-2.0", "voice_id": "male_0004_a",
          "emotion_label": null, "style_label": null, "speed": 1.0, "volume": 1.0, "pitch": 0,
          "verification_status": "unverified", "verified_at": null,
          "audio": { "format": "mp3", "sample_rate": 32000, "bitrate": 128000, "channel": 2 }
        },
        "audio": {
          "path": "/Users/x/Library/Application Support/books-exporter/speech/clips/x/versions/1/audio.mp3",
          "sha256": "\(String(repeating: "c", count: 64))", "format": "mp3", "size_bytes": 4096,
          "duration_ms": 1234, "sample_rate": 32000, "bitrate": 128000, "channel": 2
        },
        "provider": { "trace_id": "trace-1", "usage_characters": 12 },
        "warnings": []
      }
    }
    """

    private static let generateCacheHitJSON = """
    {
      "schema_version": 1,
      "receipt": {
        "operation": "generate",
        "clip_id": "\(String(repeating: "c", count: 64))",
        "attempt_id": null,
        "source": "cache",
        "provider_called": false,
        "asset_id": "book-1", "annotation_id": "annotation-41", "content_kind": "note",
        "text_sha256": null, "unicode_characters": 5, "estimated_billing_characters": 5,
        "billing_estimator_version": "senseaudio-docs-2026-09-10",
        "profile": null, "audio": null, "provider": null, "warnings": []
      }
    }
    """

    private static func errorJSON(code: String, details: String?) -> String {
        // `details` is a member of the `error` object, not a sibling of it.
        // Quoting it as a sibling produces a plausible-looking fixture that
        // silently drops the field the retry decision depends on.
        let detailsField = details.map { ",\n    \"details\": \($0)" } ?? ""
        return """
        {
          "schema_version": 1,
          "error": {
            "code": "\(code)",
            "message": "boom",
            "remediation": "do the thing"\(detailsField)
          }
        }
        """
    }

    fileprivate static func jsonError(code: String, details: String?) -> RustCLICommandResult {
        .failure(status: 1, stderr: Self.errorJSON(code: code, details: details))
    }

    // MARK: - Harness

    private final class Recorder: @unchecked Sendable {
        private let lock = NSLock()
        private var calls: [(arguments: [String], environment: [String: String]?)] = []
        private let result: @Sendable ([String], [String: String]?) -> RustCLICommandResult

        init(result: @escaping @Sendable ([String], [String: String]?) -> RustCLICommandResult) {
            self.result = result
        }

        var recorded: [(arguments: [String], environment: [String: String]?)] {
            lock.lock()
            defer { lock.unlock() }
            return calls
        }

        /// The single call that ran `subcommand`.
        ///
        /// Indexing by position does not work: resolving the credential issues
        /// a `profile show` of its own, so `.first` is frequently the probe
        /// rather than the call under test.
        func call(containing subcommand: String) throws -> (arguments: [String], environment: [String: String]?) {
            let matches = recorded.filter { $0.arguments.contains(subcommand) }
            return try XCTUnwrap(matches.last, "no call containing \(subcommand)")
        }

        func record(_ arguments: [String], _ environment: [String: String]?) -> RustCLICommandResult {
            lock.lock()
            calls.append((arguments, environment))
            lock.unlock()
            return result(arguments, environment)
        }
    }

    private func makeClient(
        _ recorder: Recorder
    ) -> RustCLIClient {
        RustCLIClient(
            executableURL: URL(fileURLWithPath: "/tmp/apple-books-exporter"),
            runner: { _, arguments, environment in recorder.record(arguments, environment) }
        )
    }

    /// Routes `profile show` to the profile fixture and everything else to
    /// `result`.
    ///
    /// A single canned response for every call does not work: the credential
    /// resolver asks the CLI for the profile on its way to injecting the key,
    /// so a recorder that answers `generate`'s JSON to that question makes the
    /// test fail in a way that looks like a decoding bug in the service.
    private func always(_ result: RustCLICommandResult) -> Recorder {
        Recorder { arguments, _ in
            arguments.contains("profile") ? .success(Self.profileJSON) : result
        }
    }

    private func makeKeyedService(
        _ recorder: Recorder,
        key: String? = "secret-value"
    ) -> (SpeechService, SpeechCredentialResolver) {
        let client = makeClient(recorder)
        let keychain = InMemoryKeychainStore()
        if let key {
            // Pre-seed under the name the CLI reports, so the tests do not have
            // to prime the resolver through a profile call first.
            try? keychain.setSecret(key, forKey: "SENSEAUDIO_API_KEY")
        }
        let resolver = SpeechCredentialResolver(client: client, keychain: keychain)
        return (SpeechService(client: client, credentialResolver: resolver), resolver)
    }

    // MARK: - Credential channel

    func testCredentialReachesTheChildEnvironmentAndNotTheArguments() async throws {
        let recorder = always(.success(Self.voicesJSON))
        let (service, _) = makeKeyedService(recorder)

        _ = try await service.voices()

        let call = try recorder.call(containing: "voices")
        XCTAssertEqual(call.environment?["SENSEAUDIO_API_KEY"], "secret-value")
        // The value must never reach argv: process arguments are world-readable
        // through `ps`.
        XCTAssertFalse(call.arguments.contains("secret-value"))
        XCTAssertFalse(call.arguments.contains { $0.contains("secret-value") })
    }

    func testCommandsThatDoNotNeedTheCredentialDoNotReceiveIt() async throws {
        let recorder = always(.success(Self.profileJSON))
        let (service, _) = makeKeyedService(recorder)

        _ = try await service.profile()
        XCTAssertNil(try recorder.call(containing: "profile").environment)
    }

    func testVoiceCatalogIsCredentialedEvenThoughItIsFree() async throws {
        // `speech voices` is the only free command that still needs a
        // credential, because it is a network call. Reading it as "free, so no
        // key" would send it unauthenticated and turn a clear
        // SPEECH_AUTH_FAILED into an empty picker.
        let recorder = always(.success(Self.voicesJSON))
        let (service, _) = makeKeyedService(recorder)

        _ = try await service.voices()

        XCTAssertEqual(
            try recorder.call(containing: "voices").environment?["SENSEAUDIO_API_KEY"],
            "secret-value"
        )
    }

    func testNoStoredKeyMeansNoEnvironmentRatherThanAnEmptyOne() async throws {
        let recorder = always(.success(Self.voicesJSON))
        let (service, _) = makeKeyedService(recorder, key: nil)

        _ = try await service.voices()

        // nil, not ["SENSEAUDIO_API_KEY": ""]. An empty value would reach the
        // provider as a real credential and turn a clear local "not configured"
        // into an opaque authentication failure.
        XCTAssertNil(try recorder.call(containing: "voices").environment)
    }

    func testEnvironmentNameIsLearnedFromTheProfileNotAssumed() async throws {
        let recorder = Recorder { arguments, _ in
            if arguments.contains("profile") {
                return .success(Self.profileJSON)
            }
            return .success(Self.voicesJSON)
        }
        let client = makeClient(recorder)
        let resolver = SpeechCredentialResolver(client: client, keychain: InMemoryKeychainStore())

        let name = try await resolver.environmentName()
        XCTAssertEqual(name, "SENSEAUDIO_API_KEY")
        XCTAssertEqual(
            try recorder.call(containing: "profile").arguments,
            ["speech", "profile", "show", "--json"]
        )
    }

    // MARK: - Billed-operation accounting

    func testProviderCallIsReportedAsBilled() async throws {
        let (service, _) = makeKeyedService(always(.success(Self.generateProviderJSON)))

        let receipt = try await service.generate(
            assetID: "book-1",
            annotationID: "annotation-41",
            contentKind: "highlight"
        ).receipt

        XCTAssertTrue(receipt.wasBilled)
        XCTAssertEqual(receipt.source, "provider")
        XCTAssertNotNil(receipt.attemptID)
        XCTAssertEqual(receipt.estimatedBillingCharacters, 12)
    }

    func testCacheHitIsNotReportedAsBilled() async throws {
        let (service, _) = makeKeyedService(always(.success(Self.generateCacheHitJSON)))

        let receipt = try await service.generate(
            assetID: "book-1",
            annotationID: "annotation-41",
            contentKind: "note"
        ).receipt

        XCTAssertFalse(receipt.wasBilled)
        XCTAssertEqual(receipt.source, "cache")
        XCTAssertNil(receipt.attemptID)
    }

    // MARK: - The retry decision

    func testUnknownOutcomeIsCarriedOutOfStderrAndTreatedAsPossiblyBilled() async throws {
        let recorder = always(Self.jsonError(
            code: "SPEECH_RESULT_UNKNOWN",
            details: """
            {"provider": "senseaudio", "reason": "result_unknown", "outcome": "unknown",
             "trace_id": "trace-9", "attempt_id": "attempt-9"}
            """
        ))
        let (service, _) = makeKeyedService(recorder)

        do {
            _ = try await service.generate(
                assetID: "book-1",
                annotationID: "annotation-41",
                contentKind: "highlight"
            )
            XCTFail("expected the failure to propagate")
        } catch let SpeechServiceError.commandFailed(error) {
            XCTAssertEqual(error.code, "SPEECH_RESULT_UNKNOWN")
            XCTAssertEqual(error.details?.outcome, .unknown)
            XCTAssertEqual(error.details?.traceID, "trace-9")
            // The whole point: this may already have cost money, so a plain
            // retry is not offered.
            XCTAssertTrue(error.mayHaveBeenBilled)
            XCTAssertFalse(error.isSafeToRetryWithoutRegenerate)
        }
    }

    func testProviderSucceededButArtifactMissingIsAlsoPossiblyBilled() async throws {
        let recorder = always(Self.jsonError(
            code: "SPEECH_ARTIFACT_COMMIT_FAILED",
            details: """
            {"provider": "senseaudio", "reason": "artifact_commit_failed",
             "outcome": "provider_succeeded_artifact_missing"}
            """
        ))
        let (service, _) = makeKeyedService(recorder)

        do {
            _ = try await service.generate(
                assetID: "book-1", annotationID: "a", contentKind: "note"
            )
            XCTFail("expected failure")
        } catch let SpeechServiceError.commandFailed(error) {
            XCTAssertTrue(error.mayHaveBeenBilled)
        }
    }

    func testFailedOutcomeIsSafeToRetry() async throws {
        let recorder = always(Self.jsonError(
            code: "SPEECH_RATE_LIMITED",
            details: """
            {"provider": "senseaudio", "reason": "rate_limited", "outcome": "failed"}
            """
        ))
        let (service, _) = makeKeyedService(recorder)

        do {
            _ = try await service.generate(
                assetID: "book-1", annotationID: "a", contentKind: "note"
            )
            XCTFail("expected failure")
        } catch let SpeechServiceError.commandFailed(error) {
            XCTAssertFalse(error.mayHaveBeenBilled)
            XCTAssertTrue(error.isSafeToRetryWithoutRegenerate)
        }
    }

    func testPreFlightFailureWithoutAnOutcomeIsNotTreatedAsBilled() async throws {
        // `generate` checks credential, storage budget and voice catalog in
        // that order, all before opening a connection, and those errors carry
        // no `outcome`.
        let recorder = always(Self.jsonError(
            code: "SPEECH_AUTH_FAILED",
            details: #"{"provider": "senseaudio", "reason": "missing_api_key"}"#
        ))
        let (service, _) = makeKeyedService(recorder)

        do {
            _ = try await service.generate(
                assetID: "book-1", annotationID: "a", contentKind: "note"
            )
            XCTFail("expected failure")
        } catch let SpeechServiceError.commandFailed(error) {
            XCTAssertFalse(error.mayHaveBeenBilled)
            XCTAssertEqual(error.details?.reason, "missing_api_key")
            XCTAssertEqual(error.remediation, "do the thing")
        }
    }

    func testNumericDetailValueIsNotLost() async throws {
        // A rejected `--speed 3.5` sends a number. Decoding `details.value` as a
        // string would fail the whole `details` object, and `details` is where
        // the reason and the outcome live.
        let recorder = always(Self.jsonError(
            code: "SPEECH_PROFILE_INVALID",
            details: #"{"reason": "out_of_range", "field": "speed", "value": 3.5}"#
        ))
        let (service, _) = makeKeyedService(recorder)

        do {
            _ = try await service.generate(
                assetID: "book-1", annotationID: "a", contentKind: "note"
            )
            XCTFail("expected failure")
        } catch let SpeechServiceError.commandFailed(error) {
            XCTAssertEqual(error.details?.field, "speed")
            XCTAssertEqual(error.details?.value, "3.5")
        }
    }

    func testWholeNumberDetailValueIsNotRenderedWithATrailingZero() async throws {
        let recorder = Recorder { arguments, _ in
            if arguments.contains("profile") { return .success(Self.profileJSON) }
            return Self.jsonError(
                code: "SPEECH_PROFILE_INVALID",
                details: #"{"reason": "out_of_range", "field": "pitch", "value": 0}"#
            )
        }
        let (service, _) = makeKeyedService(recorder)

        do {
            _ = try await service.generate(
                assetID: "book-1", annotationID: "a", contentKind: "note"
            )
            XCTFail("expected failure")
        } catch let SpeechServiceError.commandFailed(error) {
            XCTAssertEqual(error.details?.value, "0")
        }
    }

    // MARK: - Argument construction

    func testOverridesAreSentAsCLIExpectsThem() async throws {
        let recorder = always(.success(Self.generateProviderJSON))
        let (service, _) = makeKeyedService(recorder)

        _ = try await service.generate(
            assetID: "book-1",
            annotationID: "annotation-41",
            contentKind: "highlight",
            voiceID: "female_0033_b",
            speed: 1.25,
            volume: 1,
            pitch: -12
        )

        let arguments = try recorder.call(containing: "generate").arguments
        XCTAssertEqual(arguments, [
            "speech", "generate", "--json",
            "--asset-id", "book-1",
            "--annotation-id", "annotation-41",
            "--content", "highlight",
            "--voice-id", "female_0033_b",
            "--speed", "1.25",
            "--volume", "1",
            "--pitch", "-12",
        ])
    }

    func testToneNumbersAreNotFormattedWithALocaleSeparator() async throws {
        let recorder = always(.success(Self.generateProviderJSON))
        let (service, _) = makeKeyedService(recorder)

        _ = try await service.generate(
            assetID: "book-1", annotationID: "a", contentKind: "note",
            speed: 1.5, volume: 0.5
        )

        let arguments = try recorder.call(containing: "generate").arguments
        // "1,5" is rejected by the CLI as a profile error, which is a far worse
        // report than never sending it.
        XCTAssertFalse(arguments.contains { $0.contains(",") })
        XCTAssertTrue(arguments.contains("1.5"))
        XCTAssertTrue(arguments.contains("0.5"))
    }

    func testRegenerateIsOnlySentWhenAskedFor() async throws {
        let plain = always(.success(Self.generateCacheHitJSON))
        let (plainService, _) = makeKeyedService(plain)
        _ = try await plainService.generate(assetID: "b", annotationID: "a", contentKind: "note")
        XCTAssertFalse(try plain.call(containing: "generate").arguments.contains("--regenerate"))

        let forced = always(.success(Self.generateProviderJSON))
        let (forcedService, _) = makeKeyedService(forced)
        _ = try await forcedService.generate(
            assetID: "b", annotationID: "a", contentKind: "note", regenerate: true
        )
        XCTAssertTrue(try forced.call(containing: "generate").arguments.contains("--regenerate"))
    }

    func testVoiceRefreshIsOptIn() async throws {
        let cached = always(.success(Self.voicesJSON))
        let (cachedService, _) = makeKeyedService(cached)
        _ = try await cachedService.voices()
        XCTAssertFalse(try cached.call(containing: "voices").arguments.contains("--refresh"))

        let refreshed = always(.success(Self.voicesJSON))
        let (refreshedService, _) = makeKeyedService(refreshed)
        _ = try await refreshedService.voices(refresh: true)
        XCTAssertTrue(try refreshed.call(containing: "voices").arguments.contains("--refresh"))
    }

    // MARK: - Playback contract

    func testPlayReturnsThePathAndClaimsNoPlayback() async throws {
        let playJSON = """
        {
          "schema_version": 1,
          "receipt": {
            "operation": "play",
            "clip_id": "\(String(repeating: "a", count: 64))",
            "source": "cache",
            "path": "/Users/x/…/audio.mp3",
            "played": false,
            "provider_called": false,
            "asset_id": "book-1",
            "annotation_id": "annotation-41",
            "content_kind": "highlight",
            "audio": { "format": "mp3", "sample_rate": 32000 },
            "warnings": [],
            "export_origin": null
          }
        }
        """
        let recorder = Recorder { arguments, _ in
            if arguments.contains("profile") { return .success(Self.profileJSON) }
            return .success(playJSON)
        }
        let (service, _) = makeKeyedService(recorder)

        let receipt = try await service.play(clipID: String(repeating: "a", count: 64))

        XCTAssertFalse(receipt.played)
        XCTAssertEqual(receipt.path, "/Users/x/…/audio.mp3")
        XCTAssertFalse(receipt.providerCalled)
        // Playing is local and offline, so it must not carry the key into the
        // child process.
        XCTAssertNil(try recorder.call(containing: "play").environment)
    }

    func testAMachinePlayReceiptClaimingPlaybackIsRefused() async throws {
        // If the contract ever changes so that `--json` plays, the app must
        // stop and say so rather than quietly gaining a second playback path.
        let dishonestJSON = """
        {
          "schema_version": 1,
          "receipt": {
            "operation": "play", "clip_id": "x", "source": "cache", "path": "/tmp/a.mp3",
            "played": true, "provider_called": false,
            "asset_id": "b", "annotation_id": "a", "content_kind": "note",
            "audio": null, "warnings": [], "export_origin": null
          }
        }
        """
        let recorder = Recorder { arguments, _ in
            if arguments.contains("profile") { return .success(Self.profileJSON) }
            return .success(dishonestJSON)
        }
        let (service, _) = makeKeyedService(recorder)

        do {
            _ = try await service.play(clipID: "x")
            XCTFail("expected the contract violation to be refused")
        } catch let SpeechServiceError.malformedResponse(details) {
            XCTAssertTrue(details.contains("不应发声"))
        }
    }

    // MARK: - Envelope discipline

    func testEmptyStdoutIsNotTreatedAsASuccessfulEmptyResult() async throws {
        let recorder = Recorder { arguments, _ in
            if arguments.contains("profile") { return .success(Self.profileJSON) }
            return RustCLICommandResult(terminationStatus: 0)
        }
        let (service, _) = makeKeyedService(recorder)

        do {
            _ = try await service.voices()
            XCTFail("expected a malformed-response error")
        } catch let SpeechServiceError.emptyResponse {
            XCTAssertTrue(true)
        }
    }

    func testNonJSONStderrStillProducesAUsableError() async throws {
        let recorder = Recorder { arguments, _ in
            if arguments.contains("profile") { return .success(Self.profileJSON) }
            return .failure(status: 2, stderr: "dyld: symbol not found")
        }
        let (service, _) = makeKeyedService(recorder)

        do {
            _ = try await service.voices()
            XCTFail("expected failure")
        } catch let SpeechServiceError.commandFailed(error) {
            XCTAssertEqual(error.code, "BACKEND_UNAVAILABLE")
            XCTAssertTrue(error.message.contains("dyld"))
        }
    }

    func testErrorEnvelopeWithExitCodeZeroIsRefused() async throws {
        // Success is discriminated by the presence of `receipt`, not by the
        // exit code, so the two can disagree. Rendering an error as a result
        // would be worse than refusing it.
        let recorder = Recorder { arguments, _ in
            if arguments.contains("profile") { return .success(Self.profileJSON) }
            return RustCLICommandResult(
                stdout: Data(Self.errorJSON(code: "SPEECH_AUTH_FAILED", details: nil).utf8),
                terminationStatus: 0
            )
        }
        let (service, _) = makeKeyedService(recorder)

        do {
            _ = try await service.voices()
            XCTFail("expected refusal")
        } catch let SpeechServiceError.malformedResponse(details) {
            XCTAssertTrue(details.contains("错误信封"))
        }
    }

    func testUnsupportedSchemaVersionIsNamed() async throws {
        let bumped = Self.profileJSON.replacingOccurrences(
            of: "\"schema_version\": 1",
            with: "\"schema_version\": 2"
        )
        let recorder = Recorder { arguments, _ in
            if arguments.contains("profile") { return .success(Self.profileJSON) }
            return .success(bumped)
        }
        let (service, _) = makeKeyedService(recorder)

        do {
            _ = try await service.voices()
            XCTFail("expected refusal")
        } catch let SpeechServiceError.unsupportedSchemaVersion(version) {
            XCTAssertEqual(version, 2)
        }
    }

    // MARK: - Catalog shape

    func testCatalogDecodesWithoutInventingFieldsThatDoNotExist() async throws {
        let (service, _) = makeKeyedService(always(.success(Self.voicesJSON)))

        let receipt = try await service.voices().receipt

        XCTAssertEqual(receipt.provider, "senseaudio")
        XCTAssertFalse(receipt.stale)
        let voice = try XCTUnwrap(receipt.voices.first)
        XCTAssertEqual(voice.voiceID, "female_0033_b")
        XCTAssertEqual(voice.emotionLabel, "开心")
        // `description` is an array, empty rather than null, when absent.
        XCTAssertEqual(voice.description, ["开心"])
        XCTAssertNil(voice.styleLabel)
        // There is no language or gender field, and no sample URL. A picker
        // built as though they existed would ship empty columns.
        XCTAssertNil(voice.createdTime)
    }

    func testProfileAudioSpecOmitsFileFieldsInAProfileReceipt() async throws {
        let (service, _) = makeKeyedService(always(.success(Self.profileJSON)))

        let profile = try await service.profile().receipt.profile

        // A strict decoder that expected `path` here would fail the whole
        // profile response, taking the credential name with it.
        let audio = try XCTUnwrap(profile.audio)
        XCTAssertNil(audio.path)
        XCTAssertNil(audio.sha256)
        XCTAssertEqual(audio.format, "mp3")
        XCTAssertEqual(profile.voiceID, "male_0004_a")
    }

    // MARK: - The cost of changing a parameter

    func testToneComparisonSpotsADifferentClip() {
        let profile = SpeechProfile(
            provider: "senseaudio",
            model: "sensenova-tts-2.0",
            voiceID: "male_0004_a",
            emotionLabel: nil,
            styleLabel: nil,
            speed: 1.0,
            volume: 1.0,
            pitch: 0,
            verificationStatus: "unverified",
            verifiedAt: nil,
            audio: nil
        )

        // Identical parameters reuse the cached clip: no charge.
        XCTAssertFalse(profile.toneDiffers(voiceID: "male_0004_a", speed: 1.0, volume: 1.0, pitch: 0))
        // Any of these produces a different `clip_id`, so a different charge.
        XCTAssertTrue(profile.toneDiffers(voiceID: "female_0033_b", speed: 1.0, volume: 1.0, pitch: 0))
        XCTAssertTrue(profile.toneDiffers(voiceID: "male_0004_a", speed: 1.1, volume: 1.0, pitch: 0))
        XCTAssertTrue(profile.toneDiffers(voiceID: "male_0004_a", speed: 1.0, volume: 1.2, pitch: 0))
        XCTAssertTrue(profile.toneDiffers(voiceID: "male_0004_a", speed: 1.0, volume: 1.0, pitch: -1))
    }
}
