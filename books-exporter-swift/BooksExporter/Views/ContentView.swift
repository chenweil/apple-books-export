import SwiftUI

struct ContentView: View {
    @State private var bookListVM: BookListViewModel!
    @State private var detailVM: DetailViewModel!
    @State private var showPreview = false
    @State private var showExportDialog = false
    
    var body: some View {
        HSplitView {
            BookListView(viewModel: bookListVM)
                .frame(minWidth: 300, idealWidth: 400)
            
            DetailView(
                viewModel: detailVM,
                onPreview: {
                    showPreview = true
                },
                onExport: {
                    showExportDialog = true
                }
            )
            .frame(minWidth: 300, idealWidth: 400)
        }
        .frame(minWidth: 700, minHeight: 500)
        .onAppear {
            let service = BookService()
            bookListVM = BookListViewModel(bookService: service)
            detailVM = DetailViewModel(bookService: service)
            
            Task {
                await bookListVM.loadBooks()
            }
        }
        .onChange(of: bookListVM.selectedBook) { _, newBook in
            guard let book = newBook else { return }
            Task {
                await detailVM.onBookSelected(book)
            }
        }
        .sheet(isPresented: $showPreview) {
            if let book = bookListVM.selectedBook {
                PreviewView(
                    book: book,
                    annotations: detailVM.annotations,
                    onDismiss: { showPreview = false },
                    onExport: {
                        showExportDialog = true
                        showPreview = false
                    }
                )
            }
        }
        .sheet(isPresented: $showExportDialog) {
            if let book = bookListVM.selectedBook {
                ExportProgressView(
                    book: book,
                    isExporting: detailVM.isExporting,
                    exportDone: detailVM.exportDone,
                    onDismiss: {
                        if detailVM.exportDone {
                            showExportDialog = false
                        } else {
                            showExportDialog = false
                        }
                    }
                )
                .task(id: detailVM.isExporting) {
                    if detailVM.isExporting {
                        let documentsURL = FileManager.default.urls(for: .desktopDirectory, in: .userDomainMask)[0]
                        await detailVM.exportSelectedBook(to: documentsURL)
                    }
                }
            }
        }
    }
}

#Preview {
    ContentView()
}
