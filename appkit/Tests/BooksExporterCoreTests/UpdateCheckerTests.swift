import Foundation
import XCTest
@testable import BooksExporterCore

final class UpdateCheckerTests: XCTestCase {
    func testManualCheckReportsCompatibleStableRelease() async {
        let checker = makeChecker(
            manifest: """
            {
              "schema_version": 1,
              "channel": "stable",
              "version": "0.1.9",
              "minimum_macos": "14.0",
              "architectures": ["arm64"],
              "release_url": "https://github.com/chenweil/apple-books-export/releases/tag/v0.1.9",
              "notes": "修复更新检查。"
            }
            """
        )

        let result = await checker.check(trigger: .manual)

        guard case let .available(manifest, shouldNotify) = result else {
            return XCTFail("expected a compatible stable release")
        }
        XCTAssertEqual(manifest.version, "0.1.9")
        XCTAssertEqual(manifest.notes, "修复更新检查。")
        XCTAssertTrue(shouldNotify)
    }

    func testManualCheckReportsUpToDateWhenVersionsMatch() async {
        let checker = makeChecker(
            manifest: """
            {
              "schema_version": 1,
              "channel": "stable",
              "version": "0.1.8",
              "minimum_macos": "14.0",
              "architectures": ["arm64"],
              "release_url": "https://github.com/chenweil/apple-books-export/releases/tag/v0.1.8",
              "notes": ""
            }
            """
        )

        let result = await checker.check(trigger: .manual)

        XCTAssertEqual(result, .upToDate)
    }

    func testManualCheckUsesSemanticVersionOrdering() async {
        let checker = makeChecker(
            manifest: manifest(version: "0.1.10", minimumMacOS: "14.0", architectures: ["arm64"]),
            currentVersion: "0.1.9"
        )

        let result = await checker.check(trigger: .manual)

        guard case let .available(manifest, _) = result else {
            return XCTFail("expected 0.1.10 to be newer than 0.1.9")
        }
        XCTAssertEqual(manifest.version, "0.1.10")
    }

    func testManualCheckRejectsUnsupportedSchemaAndNonStableChannel() async {
        let unsupportedSchemaChecker = makeChecker(
            manifest: manifest(version: "0.1.9", schemaVersion: 2)
        )
        let betaChecker = makeChecker(
            manifest: manifest(version: "0.1.9", channel: "beta")
        )

        let unsupportedSchemaResult = await unsupportedSchemaChecker.check(trigger: .manual)
        let betaResult = await betaChecker.check(trigger: .manual)

        XCTAssertEqual(unsupportedSchemaResult, .unavailable)
        XCTAssertEqual(betaResult, .unavailable)
    }

    func testManualCheckRejectsIncompatibleRelease() async {
        let checker = makeChecker(
            manifest: manifest(version: "0.1.9", minimumMacOS: "15.0", architectures: ["arm64"])
        )

        let result = await checker.check(trigger: .manual)

        XCTAssertEqual(result, .unavailable)
    }

    func testManualCheckRejectsWrongArchitecture() async {
        let checker = makeChecker(
            manifest: manifest(version: "0.1.9", minimumMacOS: "14.0", architectures: ["x86_64"])
        )

        let result = await checker.check(trigger: .manual)

        XCTAssertEqual(result, .unavailable)
    }

    func testManualCheckRejectsPrereleaseAndUntrustedReleaseURL() async {
        let prereleaseChecker = makeChecker(
            manifest: manifest(
                version: "0.1.9-beta.1",
                minimumMacOS: "14.0",
                architectures: ["arm64"]
            )
        )
        let untrustedURLChecker = makeChecker(
            manifest: manifest(
                version: "0.1.9",
                minimumMacOS: "14.0",
                architectures: ["arm64"],
                releaseURL: "https://example.com/releases/v0.1.9"
            )
        )

        let prereleaseResult = await prereleaseChecker.check(trigger: .manual)
        let untrustedURLResult = await untrustedURLChecker.check(trigger: .manual)

        XCTAssertEqual(prereleaseResult, .unavailable)
        XCTAssertEqual(untrustedURLResult, .unavailable)
    }

    func testAutomaticCheckIsThrottledAndNotifiesEachVersionOnce() async {
        var now = Date(timeIntervalSince1970: 1_700_000_000)
        let defaults = makeDefaults()
        let checker = makeChecker(
            manifest: manifest(version: "0.1.9", minimumMacOS: "14.0", architectures: ["arm64"]),
            defaults: defaults,
            now: { now }
        )

        let firstResult = await checker.check(trigger: .automatic)
        let throttledResult = await checker.check(trigger: .automatic)
        now.addTimeInterval(UpdateChecker.automaticCheckInterval + 1)
        let alreadyNotifiedResult = await checker.check(trigger: .automatic)

        guard case let .available(_, shouldNotify) = firstResult else {
            return XCTFail("expected the first automatic check to find an update")
        }
        XCTAssertTrue(shouldNotify)
        XCTAssertEqual(throttledResult, .skipped)
        guard case let .available(_, shouldNotifyAgain) = alreadyNotifiedResult else {
            return XCTFail("expected a later automatic check to find the same update")
        }
        XCTAssertFalse(shouldNotifyAgain)
    }

    private func makeChecker(
        manifest: String,
        currentVersion: String = "0.1.8",
        currentMacOSVersion: String = "14.0",
        currentArchitecture: String = "arm64",
        defaults: UserDefaults? = nil,
        now: @escaping () -> Date = { Date(timeIntervalSince1970: 1_700_000_000) }
    ) -> UpdateChecker {
        let defaults = defaults ?? makeDefaults()
        let loader = StubManifestLoader(data: Data(manifest.utf8))

        return UpdateChecker(
            loader: loader,
            currentVersion: currentVersion,
            currentMacOSVersion: currentMacOSVersion,
            currentArchitecture: currentArchitecture,
            now: now,
            defaults: defaults
        )
    }

    private func manifest(
        version: String,
        minimumMacOS: String = "14.0",
        architectures: [String] = ["arm64"],
        releaseURL: String = "https://github.com/chenweil/apple-books-export/releases/tag/v0.1.9",
        schemaVersion: Int = 1,
        channel: String = "stable"
    ) -> String {
        let architecturesJSON = architectures.map { "\"\($0)\"" }.joined(separator: ", ")
        return """
        {
          "schema_version": \(schemaVersion),
          "channel": "\(channel)",
          "version": "\(version)",
          "minimum_macos": "\(minimumMacOS)",
          "architectures": [\(architecturesJSON)],
          "release_url": "\(releaseURL)",
          "notes": ""
        }
        """
    }

    private func makeDefaults() -> UserDefaults {
        let suiteName = "books-exporter-update-checker-" + UUID().uuidString
        let defaults = UserDefaults(suiteName: suiteName)!
        defaults.removePersistentDomain(forName: suiteName)
        return defaults
    }
}

private struct StubManifestLoader: UpdateManifestLoading {
    let data: Data

    func load() async throws -> Data {
        data
    }
}
