import Foundation

enum RefreshInterval: Int, CaseIterable {
    case disabled = 0
    case oneMinute = 60
    case fiveMinutes = 300
    case fifteenMinutes = 900
    case thirtyMinutes = 1_800
    case oneHour = 3_600

    var displayName: String {
        switch self {
        case .disabled: return "关闭"
        case .oneMinute: return "每 1 分钟"
        case .fiveMinutes: return "每 5 分钟"
        case .fifteenMinutes: return "每 15 分钟"
        case .thirtyMinutes: return "每 30 分钟"
        case .oneHour: return "每小时"
        }
    }

    var timeInterval: TimeInterval? {
        guard self != .disabled else { return nil }
        return TimeInterval(rawValue)
    }
}

final class AppSettingsStore {
    static let shared = AppSettingsStore()
    static let refreshIntervalKey = "appkit.refreshInterval"
    static let didChangeNotification = Notification.Name("BooksExporter.AppSettingsDidChange")
    static let defaultRefreshInterval: RefreshInterval = .fiveMinutes

    private let defaults: UserDefaults
    private let notificationCenter: NotificationCenter

    init(
        defaults: UserDefaults = .standard,
        notificationCenter: NotificationCenter = .default
    ) {
        self.defaults = defaults
        self.notificationCenter = notificationCenter
    }

    var refreshInterval: RefreshInterval {
        get {
            guard let storedValue = defaults.object(forKey: Self.refreshIntervalKey) as? NSNumber,
                  let interval = RefreshInterval(rawValue: storedValue.intValue) else {
                return Self.defaultRefreshInterval
            }
            return interval
        }
        set {
            guard refreshInterval != newValue else { return }
            defaults.set(newValue.rawValue, forKey: Self.refreshIntervalKey)
            notificationCenter.post(name: Self.didChangeNotification, object: self)
        }
    }
}
