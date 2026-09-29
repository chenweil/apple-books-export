import AppKit
import Foundation

enum PermissionStatus {
    case ok
    case needsFullDiskAccess
    case notAvailable
}

func checkDatabasePermissionStatus() -> PermissionStatus {
    let homeDir = FileManager.default.homeDirectoryForCurrentUser
    let dataPath = homeDir.appendingPathComponent("Library/Containers/com.apple.iBooksX/Data/Documents")

    guard FileManager.default.fileExists(atPath: dataPath.path) else {
        return .notAvailable
    }

    // Data path exists. If the app can't open the databases, it's almost
    // certainly a Full Disk Access issue.
    return .needsFullDiskAccess
}

func presentFullDiskAccessAlertIfNeeded(for window: NSWindow?) -> Bool {
    guard checkDatabasePermissionStatus() == .needsFullDiskAccess else {
        return false
    }

    let alert = NSAlert()
    alert.messageText = "需要完全磁盘访问权限"
    alert.informativeText = "导出 Apple Books 笔记需要「完全磁盘访问权限」。\n\n请在「系统设置 → 隐私与安全性 → 完全磁盘访问权限」中添加此应用或终端，然后点击「重试」。"
    alert.alertStyle = .warning
    alert.addButton(withTitle: "打开系统设置")
    alert.addButton(withTitle: "重试")
    alert.addButton(withTitle: "取消")

    let response = alert.runModal()
    if response == .alertFirstButtonReturn {
        NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")!)
    }
    return response == .alertSecondButtonReturn
}
