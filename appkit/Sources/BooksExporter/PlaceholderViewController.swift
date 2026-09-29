import AppKit

final class PlaceholderViewController: NSViewController {
    override func loadView() {
        let view = NSView()
        let label = NSTextField(labelWithString: "AppKit 实验骨架")
        label.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(label)

        NSLayoutConstraint.activate([
            label.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            label.centerYAnchor.constraint(equalTo: view.centerYAnchor)
        ])

        self.view = view
    }
}
