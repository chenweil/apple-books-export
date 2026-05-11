import SwiftUI

struct ContentView: View {
    @State private var showPreview = false
    @State private var showExportDialog = false
    @State private var service: BookService
    @State private var bookListVM: BookListViewModel
    @State private var detailVM: DetailViewModel

    init() {
        let svc = BookService()
        _service = State(initialValue: svc)
        _bookListVM = State(initialValue: BookListViewModel(bookService: svc))
        _detailVM = State(initialValue: DetailViewModel(bookService: svc))
    }

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
                        showExportDialog = false
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
