import SwiftUI

struct ContentView: View {
    @State private var bookService = BookService()
    @State private var bookListVM: BookListViewModel!
    @State private var detailVM: DetailViewModel!
    @State private var showPreview = false
    @State private var showExportDialog = false
    @State private var exportDone = false
    
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
            bookListVM = BookListViewModel(bookService: bookService)
            detailVM = DetailViewModel(bookService: bookService)
            
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
                    isExporting: bookService.isBusy,
                    exportDone: exportDone,
                    onDismiss: {
                        if exportDone {
                            showExportDialog = false
                            exportDone = false
                            detailVM.resetExportState()
                        } else {
                            showExportDialog = false
                        }
                    }
                )
                
                if bookService.isBusy && !exportDone {
                    Task {
                        let documentsURL = FileManager.default.urls(for: .desktopDirectory, in: .userDomainMask)[0]
                        try? await detailVM.exportSelectedBook(to: documentsURL)
                        exportDone = detailVM.exportDone
                    }
                }
            }
        }
    }
}

#Preview {
    ContentView()
}
