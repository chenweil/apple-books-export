import Foundation
import SwiftUI

func setupDatabasePermissions() {
    let homeDir = FileManager.default.homeDirectoryForCurrentUser
    let dataPath = homeDir.appendingPathComponent("Library/Containers/com.apple.iBooksX/Data/Documents")
    
    if !FileManager.default.fileExists(atPath: dataPath.path) {
        return
    }
    
    let alert = NSAlert()
    alert.messageText = "需要完全磁盘访问权限"
    alert.informativeText = "导出 Apple Books 笔记需要「完全磁盘访问权限」。\n\n请在「系统设置 → 隐私与安全性 → 完全磁盘访问权限」中添加终端或此应用。"
    alert.alertStyle = .warning
    alert.addButton(withTitle: "打开系统设置")
    alert.addButton(withTitle: "取消")
    
    if alert.runModal() == .alertFirstButtonReturn {
        NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")!)
    }
}
