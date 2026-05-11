import SwiftUI

struct PreviewView: View {
    let book: Book
    let annotations: [Annotation]
    var onDismiss: () -> Void
    var onExport: () -> Void
    
    @State private var exportDone = false
    
    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
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
                        
                        ForEach(AnnotationType.allCases.sorted(by: { $0.sortOrder < $1.sortOrder })) { type in
                            guard let typeAnnotations = grouped[type] else { continue }
                            
                            Section {
                                ForEach(Array(typeAnnotations.enumerated()), id: \.element.id) { index, annotation in
                                    VStack(alignment: .leading, spacing: 8) {
                                        Text("### \(index + 1). \(annotation.chapterTitle.isEmpty ? "无章节" : annotation.chapterTitle)")
                                            .font(.headline)
                                        
                                        Text("*\(formatDate(annotation.createdAt))*")
                                            .font(.caption)
                                            .foregroundColor(.secondary)
                                        
                                        if let text = annotation.contentText {
                                            Text("> \(text)")
                                                .font(.body)
                                                .padding(.leading, 8)
                                                .foregroundColor(.secondary)
                                        }
                                        
                                        if let note = annotation.noteText, !note.isEmpty {
                                            Text("**笔记**: \(note)")
                                                .font(.body)
                                        }
                                        
                                        Divider()
                                    }
                                }
                            } header: {
                                Text("## \(type.displayName)")
                                    .font(.title2)
                                    .fontWeight(.bold)
                                    .padding(.top, 8)
                            }
                        }
                    }
                    .padding()
                }
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

struct PreviewLinkModifier: ViewModifier {
    @Binding var isPresented: Bool
    let book: Book
    let annotations: [Annotation]
    let onExport: () -> Void
    
    func body(content: Content) -> some View {
        content.sheet(isPresented: $isPresented) {
            PreviewView(
                book: book,
                annotations: annotations,
                onDismiss: { isPresented = false },
                onExport: {
                    onExport()
                    isPresented = false
                }
            )
        }
    }
}

extension View {
    func previewSheet(isPresented: Binding<Bool>, book: Book, annotations: [Annotation], onExport: @escaping () -> Void) -> some View {
        modifier(PreviewLinkModifier(isPresented: isPresented, book: book, annotations: annotations, onExport: onExport))
    }
}
