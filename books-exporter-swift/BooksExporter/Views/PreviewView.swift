import SwiftUI

struct PreviewView: View {
    let book: Book
    let annotations: [Annotation]
    var onDismiss: () -> Void
    var onExport: () -> Void
    
    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text("# \(book.title)")
                        .font(.title)
                        .fontWeight(.bold)
                    
                    Text("**作者**: \(book.author)")
                        .font(.headline)
                    
                    Text("**笔记数量**: \(book.totalAnnotations)")
                    
                    Divider()
                    
                    let grouped = Dictionary(grouping: annotations) { $0.type }
                    
                    ForEach(AnnotationType.allCases.sorted(by: { $0.sortOrder < $1.sortOrder }), id: \.self) { type in
                        if let typeAnnotations = grouped[type], !typeAnnotations.isEmpty {
                            Text("## \(type.displayName)")
                                .font(.title2)
                                .fontWeight(.bold)
                                .padding(.top, 8)

                            ForEach(typeAnnotations.enumerated().map { (index: $0, annotation: $1) }, id: \.annotation.id) { item in
                                VStack(alignment: .leading, spacing: 8) {
                                    Text("### \(item.index + 1). \(item.annotation.displayLocation)")
                                        .font(.headline)

                                    Text("*\(formatDate(item.annotation.createdAt))*")
                                        .font(.caption)
                                        .foregroundColor(.secondary)

                                    if let text = item.annotation.contentText {
                                        Text("> \(text)")
                                            .font(.body)
                                            .padding(.leading, 8)
                                            .foregroundColor(.secondary)
                                    }

                                    if let note = item.annotation.noteText, !note.isEmpty {
                                        Text("**笔记**: \(note)")
                                            .font(.body)
                                    }

                                    Divider()
                                }
                            }
                        }
                    }
                }
                .padding()
            }
            .navigationTitle("预览")
            .toolbar {
                ToolbarItem(placement: .primaryAction) {
                    Button("导出") {
                        onExport()
                    }
                    .disabled(annotations.isEmpty)
                }
            }
        }
        .frame(minWidth: 600, minHeight: 500)
    }
    
    private func formatDate(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd HH:mm"
        return formatter.string(from: date)
    }
}
