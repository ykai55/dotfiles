// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "clip-macos-helper",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "clip-macos-helper", targets: ["clip-macos-helper"]),
        .executable(name: "clip-history-menu", targets: ["clip-history-menu"]),
    ],
    targets: [
        .executableTarget(name: "clip-macos-helper"),
        .executableTarget(name: "clip-history-menu"),
    ]
)
