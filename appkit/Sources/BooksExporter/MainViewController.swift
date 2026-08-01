import AppKit

final class MainViewController: NSSplitViewController {
    private let bookListViewController = BookListViewController()
    private let bookDetailViewController = BookDetailViewController()

    override func viewDidLoad() {
        super.viewDidLoad()
        splitView.isVertical = true
        splitView.dividerStyle = .thin

        let listItem = NSSplitViewItem(viewController: bookListViewController)
        listItem.minimumThickness = 260
        listItem.maximumThickness = 400
        listItem.preferredThicknessFraction = 0.38

        let detailItem = NSSplitViewItem(viewController: bookDetailViewController)
        detailItem.minimumThickness = 360

        addSplitViewItem(listItem)
        addSplitViewItem(detailItem)

        bookListViewController.onBookSelected = { [weak self] book in
            self?.bookDetailViewController.show(book: book)
        }
    }
}
