import Foundation
import XCTest
@testable import BooksExporterCore

final class AppSettingsTests: XCTestCase {
    func testRefreshIntervalDefaultsAndPersistsAcrossStores() {
        let defaults = makeDefaults()
        let firstStore = AppSettingsStore(defaults: defaults)

        XCTAssertEqual(firstStore.refreshInterval, .fiveMinutes)

        firstStore.refreshInterval = .fifteenMinutes

        let secondStore = AppSettingsStore(defaults: defaults)
        XCTAssertEqual(secondStore.refreshInterval, .fifteenMinutes)
    }

    func testChangingRefreshIntervalPostsChangeNotification() {
        let defaults = makeDefaults()
        let notificationCenter = NotificationCenter()
        let store = AppSettingsStore(defaults: defaults, notificationCenter: notificationCenter)
        let expectation = expectation(description: "settings change notification")
        let token = notificationCenter.addObserver(
            forName: AppSettingsStore.didChangeNotification,
            object: store,
            queue: nil
        ) { _ in
            expectation.fulfill()
        }
        defer { notificationCenter.removeObserver(token) }

        store.refreshInterval = .thirtyMinutes

        wait(for: [expectation], timeout: 1)
    }

    private func makeDefaults() -> UserDefaults {
        let suiteName = "books-exporter-settings-" + UUID().uuidString
        let defaults = UserDefaults(suiteName: suiteName)!
        defaults.removePersistentDomain(forName: suiteName)
        return defaults
    }
}
