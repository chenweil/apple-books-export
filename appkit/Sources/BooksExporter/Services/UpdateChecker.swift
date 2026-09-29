import Foundation

protocol UpdateManifestLoading {
    func load() async throws -> Data
}

struct UpdateManifest: Codable, Equatable {
    let schemaVersion: Int
    let channel: String
    let version: String
    let minimumMacOS: String
    let architectures: [String]
    let releaseURL: URL
    let notes: String

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case channel
        case version
        case minimumMacOS = "minimum_macos"
        case architectures
        case releaseURL = "release_url"
        case notes
    }
}

enum UpdateCheckTrigger: Equatable {
    case automatic
    case manual
}

enum UpdateCheckResult: Equatable {
    case available(UpdateManifest, shouldNotify: Bool)
    case upToDate
    case skipped
    case unavailable
}

struct URLSessionUpdateManifestLoader: UpdateManifestLoading {
    static let defaultEndpoint = URL(
        string: "https://github.com/chenweil/apple-books-export/releases/latest/download/latest.json"
    )!

    let endpoint: URL
    let session: URLSession
    let timeout: TimeInterval

    init(
        endpoint: URL = Self.defaultEndpoint,
        session: URLSession = .shared,
        timeout: TimeInterval = 5
    ) {
        self.endpoint = endpoint
        self.session = session
        self.timeout = timeout
    }

    func load() async throws -> Data {
        var request = URLRequest(url: endpoint)
        request.timeoutInterval = timeout
        request.setValue("application/json", forHTTPHeaderField: "Accept")

        let (data, response) = try await session.data(for: request)
        guard let httpResponse = response as? HTTPURLResponse,
              (200..<300).contains(httpResponse.statusCode) else {
            throw UpdateManifestLoadError.invalidHTTPResponse
        }
        return data
    }
}

enum UpdateManifestLoadError: Error {
    case invalidHTTPResponse
}

struct UpdateChecker {
    static let automaticCheckInterval: TimeInterval = 24 * 60 * 60

    private static let lastAutomaticCheckDateKey = "appkit.update.lastAutomaticCheckAt"
    private static let lastNotifiedVersionKey = "appkit.update.lastNotifiedVersion"
    private static let supportedSchemaVersion = 1
    private static let stableChannel = "stable"
    private static let trustedReleaseHost = "github.com"
    private static let trustedReleasePath = "/chenweil/apple-books-export/releases/"

    private let loader: UpdateManifestLoading
    private let currentVersion: String
    private let currentMacOSVersion: String
    private let currentArchitecture: String
    private let now: () -> Date
    private let defaults: UserDefaults
    private let decoder: JSONDecoder

    static func makeForCurrentApp(defaults: UserDefaults = .standard) -> UpdateChecker? {
        guard Bundle.main.bundleURL.pathExtension == "app",
              let currentVersion = Bundle.main.object(
                  forInfoDictionaryKey: "CFBundleShortVersionString"
              ) as? String,
              SemanticVersion(currentVersion) != nil else {
            return nil
        }

        let operatingSystemVersion = ProcessInfo.processInfo.operatingSystemVersion
        let currentMacOSVersion = [
            operatingSystemVersion.majorVersion,
            operatingSystemVersion.minorVersion,
            operatingSystemVersion.patchVersion
        ]
        .map(String.init)
        .joined(separator: ".")

        return UpdateChecker(
            loader: URLSessionUpdateManifestLoader(),
            currentVersion: currentVersion,
            currentMacOSVersion: currentMacOSVersion,
            currentArchitecture: currentArchitecture,
            defaults: defaults
        )
    }

    init(
        loader: UpdateManifestLoading,
        currentVersion: String,
        currentMacOSVersion: String,
        currentArchitecture: String,
        now: @escaping () -> Date = Date.init,
        defaults: UserDefaults = .standard,
        decoder: JSONDecoder = JSONDecoder()
    ) {
        self.loader = loader
        self.currentVersion = currentVersion
        self.currentMacOSVersion = currentMacOSVersion
        self.currentArchitecture = currentArchitecture
        self.now = now
        self.defaults = defaults
        self.decoder = decoder
    }

    func check(trigger: UpdateCheckTrigger) async -> UpdateCheckResult {
        let checkDate = now()
        if trigger == .automatic {
            if let lastCheckDate = defaults.object(forKey: Self.lastAutomaticCheckDateKey) as? Date,
               checkDate.timeIntervalSince(lastCheckDate) < Self.automaticCheckInterval {
                return .skipped
            }
            defaults.set(checkDate, forKey: Self.lastAutomaticCheckDateKey)
        }

        guard let data = try? await loader.load(),
              let manifest = try? decoder.decode(UpdateManifest.self, from: data),
              let currentVersion = SemanticVersion(currentVersion),
              let remoteVersion = SemanticVersion(manifest.version),
              let currentMacOSVersion = NumericVersion(self.currentMacOSVersion),
              let minimumMacOS = NumericVersion(manifest.minimumMacOS),
              isValid(manifest: manifest),
              !remoteVersion.isPrerelease,
              !currentVersion.isPrerelease,
              minimumMacOS <= currentMacOSVersion,
              manifest.architectures.contains(where: isCompatibleArchitecture)
        else {
            return .unavailable
        }

        guard remoteVersion > currentVersion else {
            return .upToDate
        }

        let shouldNotify: Bool
        if trigger == .manual {
            shouldNotify = true
        } else if defaults.string(forKey: Self.lastNotifiedVersionKey) == manifest.version {
            shouldNotify = false
        } else {
            defaults.set(manifest.version, forKey: Self.lastNotifiedVersionKey)
            shouldNotify = true
        }

        return .available(manifest, shouldNotify: shouldNotify)
    }

    private func isValid(manifest: UpdateManifest) -> Bool {
        guard manifest.schemaVersion == Self.supportedSchemaVersion,
              manifest.channel == Self.stableChannel,
              !manifest.architectures.isEmpty,
              !manifest.notes.contains("\0"),
              manifest.releaseURL.scheme?.lowercased() == "https",
              manifest.releaseURL.host?.lowercased() == Self.trustedReleaseHost,
              manifest.releaseURL.path.hasPrefix(Self.trustedReleasePath) else {
            return false
        }
        return true
    }

    private func isCompatibleArchitecture(_ architecture: String) -> Bool {
        architecture.lowercased() == "universal"
            || architecture.lowercased() == currentArchitecture.lowercased()
    }

    private static var currentArchitecture: String {
        #if arch(arm64)
        return "arm64"
        #elseif arch(x86_64)
        return "x86_64"
        #else
        return "unknown"
        #endif
    }
}

private struct SemanticVersion: Comparable, Equatable {
    let major: Int
    let minor: Int
    let patch: Int
    let prereleaseIdentifiers: [PrereleaseIdentifier]

    var isPrerelease: Bool {
        !prereleaseIdentifiers.isEmpty
    }

    init?(_ rawValue: String) {
        guard !rawValue.isEmpty else { return nil }

        let withoutBuildMetadata = rawValue.split(
            separator: "+",
            maxSplits: 1,
            omittingEmptySubsequences: false
        )
        guard withoutBuildMetadata.count == 1 || withoutBuildMetadata[1].isEmpty == false else {
            return nil
        }

        let versionAndPrerelease = withoutBuildMetadata[0].split(
            separator: "-",
            maxSplits: 1,
            omittingEmptySubsequences: false
        )
        guard versionAndPrerelease.count <= 2,
              !versionAndPrerelease[0].isEmpty else {
            return nil
        }

        let core = versionAndPrerelease[0].split(separator: ".", omittingEmptySubsequences: false)
        guard core.count == 3,
              let major = Self.parseCoreIdentifier(core[0]),
              let minor = Self.parseCoreIdentifier(core[1]),
              let patch = Self.parseCoreIdentifier(core[2]) else {
            return nil
        }

        let prereleaseIdentifiers: [PrereleaseIdentifier]
        if versionAndPrerelease.count == 2 {
            let identifiers = versionAndPrerelease[1].split(separator: ".", omittingEmptySubsequences: false)
            guard !identifiers.isEmpty,
                  !identifiers.contains(where: { $0.isEmpty }) else {
                return nil
            }
            var parsed: [PrereleaseIdentifier] = []
            for identifier in identifiers {
                guard let parsedIdentifier = PrereleaseIdentifier(identifier) else {
                    return nil
                }
                parsed.append(parsedIdentifier)
            }
            prereleaseIdentifiers = parsed
        } else {
            prereleaseIdentifiers = []
        }

        self.major = major
        self.minor = minor
        self.patch = patch
        self.prereleaseIdentifiers = prereleaseIdentifiers
    }

    static func < (lhs: SemanticVersion, rhs: SemanticVersion) -> Bool {
        if lhs.major != rhs.major { return lhs.major < rhs.major }
        if lhs.minor != rhs.minor { return lhs.minor < rhs.minor }
        if lhs.patch != rhs.patch { return lhs.patch < rhs.patch }

        switch (lhs.isPrerelease, rhs.isPrerelease) {
        case (false, true): return false
        case (true, false): return true
        case (false, false): return false
        case (true, true): break
        }

        for (left, right) in zip(lhs.prereleaseIdentifiers, rhs.prereleaseIdentifiers) {
            if left != right { return left < right }
        }
        return lhs.prereleaseIdentifiers.count < rhs.prereleaseIdentifiers.count
    }

    private static func parseCoreIdentifier(_ value: Substring) -> Int? {
        guard !value.isEmpty,
              value.allSatisfy(\.isNumber),
              value.count == 1 || value.first != "0" else {
            return nil
        }
        return Int(value)
    }
}

private enum PrereleaseIdentifier: Comparable, Equatable {
    case numeric(Int)
    case text(String)

    init?(_ value: Substring) {
        guard !value.isEmpty else { return nil }
        if value.allSatisfy(\.isNumber) {
            guard value.count == 1 || value.first != "0",
                  let number = Int(value) else {
                return nil
            }
            self = .numeric(number)
        } else if value.allSatisfy({ $0.isLetter || $0.isNumber || $0 == "-" }) {
            self = .text(String(value))
        } else {
            return nil
        }
    }

    static func < (lhs: PrereleaseIdentifier, rhs: PrereleaseIdentifier) -> Bool {
        switch (lhs, rhs) {
        case let (.numeric(left), .numeric(right)):
            return left < right
        case (.numeric, .text):
            return true
        case (.text, .numeric):
            return false
        case let (.text(left), .text(right)):
            return left < right
        }
    }
}

private struct NumericVersion: Comparable, Equatable {
    let major: Int
    let minor: Int
    let patch: Int

    init?(_ rawValue: String) {
        let components = rawValue.split(separator: ".", omittingEmptySubsequences: false)
        guard (1...3).contains(components.count),
              components.allSatisfy({ !$0.isEmpty && $0.allSatisfy(\.isNumber) }),
              components.allSatisfy({ $0.count == 1 || $0.first != "0" }),
              let major = Int(components[0]),
              let minor = components.count > 1 ? Int(components[1]) : 0,
              let patch = components.count > 2 ? Int(components[2]) : 0 else {
            return nil
        }
        self.major = major
        self.minor = minor
        self.patch = patch
    }

    static func < (lhs: NumericVersion, rhs: NumericVersion) -> Bool {
        if lhs.major != rhs.major { return lhs.major < rhs.major }
        if lhs.minor != rhs.minor { return lhs.minor < rhs.minor }
        return lhs.patch < rhs.patch
    }
}
