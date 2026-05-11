import SwiftUI

struct BookListView: View {
    @Bindable var viewModel: BookListViewModel
    
    @State private var currentPage = 1
    private let pageSize = 20
    
    var paginatedBooks: [Book] {
        let start = (currentPage - 1) * pageSize
        let end = min(start + pageSize, viewModel.books.count)
        guard start < viewModel.books.count else { return [] }
        return Array(viewModel.books[start..<end])
    }
    
    var totalPages: Int {
        max(1, (viewModel.books.count + pageSize - 1) / pageSize)
    }
    
    var body: some View {
        VStack(spacing: 0) {
            Text("书籍列表")
                .font(.headline)
                .padding(8)
            
            Divider()
            
            if viewModel.isLoading {
                ProgressView()
                    .padding()
            } else if viewModel.books.isEmpty {
                VStack {
                    Spacer()
                    Text("未找到笔记")
                        .foregroundColor(.secondary)
                    Text("请确保 Apple Books 中有做笔记的书籍")
                        .font(.caption)
                        .foregroundColor(.secondary)
                    Spacer()
                }
                .frame(maxWidth: .infinity)
            } else {
                ScrollView {
                    LazyVStack(spacing: 2) {
                        ForEach(paginatedBooks) { book in
                            BookRowView(book: book, isSelected: viewModel.selectedBook == book)
                                .onTapGesture {
                                    viewModel.selectBook(book)
                                }
                        }
                    }
                    .padding(8)
                }
            }
            
            Divider()
            
            HStack {
                Button("◄ 上一页") {
                    if currentPage > 1 {
                        currentPage -= 1
                    }
                }
                .disabled(currentPage <= 1)
                
                Spacer()
                
                Text("第 \(currentPage) / \(totalPages) 页")
                    .font(.caption)
                    .frame(width: 100)
                
                Spacer()
                
                Button("下一页 ►") {
                    if currentPage < totalPages {
                        currentPage += 1
                    }
                }
                .disabled(currentPage >= totalPages)
            }
            .padding(8)
        }
        .frame(minWidth: 300)
    }
}
