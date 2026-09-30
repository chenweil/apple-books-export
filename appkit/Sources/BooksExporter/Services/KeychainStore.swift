import Foundation

/// Storage for the one secret this app holds.
///
/// The Rust side never accepts a credential as an argument: there is no flag
/// that sets an API key, the config file records only the *name* of the
/// variable to read, and no receipt or error ever echoes a value. The
/// environment of the child process is the only channel, so the value has to
/// live somewhere on the Swift side and be handed to the launch explicitly.
///
/// Keychain rather than `UserDefaults` or a file in the app's support
/// directory, because ADR 0007 requires that the config stores a key
/// *reference* and never the key itself, and because a value readable by any
/// process running as this user is not what "not on disk" is meant to mean.
///
/// The protocol exists so tests can substitute an in-memory store. Nothing in
/// the app builds a `KeychainStore` directly except the composition root.
protocol KeychainStoring: AnyObject {
    /// The stored secret, or nil when nothing is stored.
    ///
    /// Returns nil rather than throwing for "absent" so a first run is not an
    /// error state; a real keychain failure throws.
    func secret(forKey name: String) throws -> String?
    func setSecret(_ value: String, forKey name: String) throws
    func removeSecret(forKey name: String) throws
}

enum KeychainStoreError: LocalizedError, Equatable {
    case unexpectedStatus(OSStatus)
    case malformedData

    var errorDescription: String? {
        switch self {
        case .unexpectedStatus(let status):
            // The status is safe to show; the payload that failed to decode is
            // the secret itself and is never put in an error message.
            return "钥匙串操作失败（OSStatus \(status)）"
        case .malformedData:
            return "钥匙串中的内容无法读取，请清除后重新填写。"
        }
    }
}

final class KeychainStore: KeychainStoring {
    private let service: String
    private let accessGroup: String?

    init(
        service: String = "com.chenweilong.books-exporter.speech",
        accessGroup: String? = nil
    ) {
        self.service = service
        self.accessGroup = accessGroup
    }

    private func baseQuery(forKey name: String) -> [String: Any] {
        var query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: name
        ]
        if let accessGroup {
            query[kSecAttrAccessGroup as String] = accessGroup
        }
        return query
    }

    func secret(forKey name: String) throws -> String? {
        var query = baseQuery(forKey: name)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne

        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)

        switch status {
        case errSecSuccess:
            guard let data = item as? Data else {
                throw KeychainStoreError.malformedData
            }
            return String(data: data, encoding: .utf8)
        case errSecItemNotFound:
            return nil
        default:
            throw KeychainStoreError.unexpectedStatus(status)
        }
    }

    func setSecret(_ value: String, forKey name: String) throws {
        let data = Data(value.utf8)
        let query = baseQuery(forKey: name)

        let updateStatus = SecItemUpdate(
            query as CFDictionary,
            [kSecValueData as String: data] as CFDictionary
        )

        switch updateStatus {
        case errSecSuccess:
            return
        case errSecItemNotFound:
            var insert = query
            insert[kSecValueData as String] = data
            // The value must be readable by this app on a later launch, so it
            // deliberately does not request kSecAttrAccessibleWhenUnlockedThis
            // DeviceOnly-style restrictions that would strand a value entered
            // while the screen is locked. A speech key is not a session token,
            // and the alternative is a setting that silently stops working.
            let addStatus = SecItemAdd(insert as CFDictionary, nil)
            guard addStatus == errSecSuccess else {
                throw KeychainStoreError.unexpectedStatus(addStatus)
            }
        default:
            throw KeychainStoreError.unexpectedStatus(updateStatus)
        }
    }

    func removeSecret(forKey name: String) throws {
        let status = SecItemDelete(baseQuery(forKey: name) as CFDictionary)
        switch status {
        case errSecSuccess, errSecItemNotFound:
            return
        default:
            throw KeychainStoreError.unexpectedStatus(status)
        }
    }
}

/// Test double. Not shipped in the app target's behaviour, only its absence of
/// side effects matters.
final class InMemoryKeychainStore: KeychainStoring, @unchecked Sendable {
    private let lock = NSLock()
    private var storage: [String: String] = [:]

    func secret(forKey name: String) throws -> String? {
        lock.lock()
        defer { lock.unlock() }
        return storage[name]
    }

    func setSecret(_ value: String, forKey name: String) throws {
        lock.lock()
        defer { lock.unlock() }
        storage[name] = value
    }

    func removeSecret(forKey name: String) throws {
        lock.lock()
        defer { lock.unlock() }
        storage.removeValue(forKey: name)
    }
}
