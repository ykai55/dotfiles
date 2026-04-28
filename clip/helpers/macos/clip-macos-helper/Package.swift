// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "clip-macos-helper",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "clip-macos-helper", targets: ["clip-macos-helper"]),
    ],
    targets: [
        .executableTarget(name: "clip-macos-helper"),
    ]
)
