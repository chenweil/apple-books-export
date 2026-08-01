// swift-tools-version: 5.9
import PackageDescription

// AppKit 实验项目(基于 swiftui 分支复用 Models/Services)
// 用 SPM 而非 Xcode project,见 docs/adr/0001-appkit-experiment-branch.md
//
// 编译运行:
//   swift build
//   swift run BooksExporter
let package = Package(
    name: "BooksExporter",
    platforms: [
        .macOS(.v14)
    ],
    targets: [
        .executableTarget(
            name: "BooksExporter",
            path: "Sources/BooksExporter",
            exclude: [
                "BooksExporter-Bridging-Header.h"
            ],
            linkerSettings: [
                .linkedFramework("AppKit"),
                .linkedFramework("Foundation")
            ]
        )
    ]
)