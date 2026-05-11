import SwiftUI

struct ExportProgressView: View {
    let book: Book
    var isExporting: Bool
    var exportDone: Bool
    var onDismiss: () -> Void
    
    var body: some View {
        VStack(spacing: 20) {
            if isExporting {
                ProgressView()
                    .scaleEffect(1.5)
                
                Text("正在导出《\(book.title)》...")
                    .font(.headline)
            } else if exportDone {
                Image(systemName: "checkmark.circle.fill")
                    .font(.system(size: 48))
                    .foregroundColor(.green)
                
                Text("导出成功!")
                    .font(.headline)
                
                Text("文件已保存到桌面")
                    .foregroundColor(.secondary)
            }
            
            HStack {
                if exportDone {
                    Button("完成", action: onDismiss)
                        .keyboardShortcut(.defaultAction)
                } else {
                    Button("取消", action: onDismiss)
                        .keyboardShortcut(.cancelAction)
                }
            }
        }
        .padding(30)
        .frame(width: 300)
    }
}
