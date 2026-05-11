import SwiftUI

struct DetailView: View {
    @Bindable var viewModel: DetailViewModel
    var onPreview: () -> Void
    var onExport: () -> Void
    
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let book = viewModel.selectedBook {
                VStack(alignment: .leading, spacing: 12) {
                    Text(book.title)
                        .font(.title2)
                        .fontWeight(.bold)
                    
                    Text("作者：\(book.author)")
                        .font(.headline)
                        .foregroundColor(.secondary)
                    
                    Divider()
                    
                    HStack(spacing: 16) {
                        StatBadge(label: "高亮", value: book.highlightsCount, color: .yellow)
                        StatBadge(label: "标注", value: book.annotationsCount, color: .blue)
                        StatBadge(label: "笔记", value: book.notesCount, color: .green)
                        StatBadge(label: "书签", value: book.bookmarksCount, color: .orange)
                    }
                    
                    Divider()
                    
                    if viewModel.isLoading {
                        HStack {
                            ProgressView()
                            Text("加载笔记中...")
                        }
                        .frame(maxWidth: .infinity, alignment: .center)
                        .padding()
                    } else if let error = viewModel.currentError {
                        Text(error.localizedDescription)
                            .font(.caption)
                            .foregroundColor(.red)
                            .padding(8)
                    } else {
                        ScrollView {
                            VStack(alignment: .leading, spacing: 8) {
                                Text("笔记预览 (前 5 条):")
                                    .font(.headline)
                                
                                ForEach(viewModel.annotations.prefix(5)) { annotation in
                                    AnnotationPreviewRow(annotation: annotation)
                                }
                                
                                if viewModel.annotations.count > 5 {
                                    Text("还有 \(viewModel.annotations.count - 5) 条笔记...")
                                        .font(.caption)
                                        .foregroundColor(.secondary)
                                }
                            }
                            .padding(8)
                        }
                    }
                    
                    Spacer()
                    
                    HStack(spacing: 16) {
                        Button(action: onPreview) {
                            Label("预览", systemImage: "eye")
                        }
                        .disabled(viewModel.annotations.isEmpty)
                        
                        Button(action: onExport) {
                            Label("导出", systemImage: "square.and.arrow.down")
                        }
                        .disabled(viewModel.annotations.isEmpty || viewModel.isExporting)
                        
                        Spacer()
                        
                        if viewModel.isExporting {
                            ProgressView()
                                .scaleEffect(0.8)
                            Text("导出中...")
                                .font(.caption)
                        }
                    }
                    .padding(.top, 8)
                }
                .padding()
            } else {
                VStack {
                    Spacer()
                    Image(systemName: "book")
                        .font(.system(size: 48))
                        .foregroundColor(.secondary)
                    Text("从左侧选择一本书")
                        .foregroundColor(.secondary)
                        .padding(.top, 16)
                    Spacer()
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .frame(minWidth: 300)
    }
}

struct StatBadge: View {
    let label: String
    let value: Int
    let color: Color
    
    var body: some View {
        VStack(spacing: 2) {
            Text("\(value)")
                .font(.title2)
                .fontWeight(.bold)
                .foregroundColor(color)
            Text(label)
                .font(.caption)
                .foregroundColor(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 8)
        .background(color.opacity(0.1))
        .cornerRadius(8)
    }
}

struct AnnotationPreviewRow: View {
    let annotation: Annotation
    
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(annotation.displayLocation)
                    .font(.caption)
                    .fontWeight(.medium)
                Spacer()
                Image(systemName: annotation.type.icon)
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
            
            if let text = annotation.contentText {
                Text(text)
                    .font(.caption)
                    .foregroundColor(.secondary)
                    .lineLimit(2)
            }
        }
        .padding(6)
        .background(Color.gray.opacity(0.05))
        .cornerRadius(4)
    }
}

extension AnnotationType {
    var icon: String {
        switch self {
        case .highlight: return "highlighter"
        case .note: return "note.text"
        case .bookmark: return "bookmark"
        }
    }
}
