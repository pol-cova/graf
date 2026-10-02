// swift-tools-version: 6.0
//
// Graf's native macOS front end. Run ../scripts/build_core_xcframework.sh
// first: it builds the Rust core into Frameworks/GrafCore.xcframework and
// generates Sources/GrafCore/graf_ffi.swift.

import PackageDescription

let package = Package(
    name: "Graf",
    platforms: [.macOS(.v15)],
    products: [
        .executable(name: "Graf", targets: ["Graf"]),
    ],
    targets: [
        // The Rust core as a static library. The module name must match the
        // modulemap the build script generates.
        .binaryTarget(
            name: "graf_ffiFFI",
            path: "Frameworks/GrafCore.xcframework"
        ),
        // Generated UniFFI bindings for the core.
        .target(
            name: "GrafCore",
            dependencies: ["graf_ffiFFI"]
        ),
        // Front-end logic with no AppKit or SwiftUI: markup scanning, paragraph
        // math, debouncing, recents. Kept separate so it is unit tested.
        .target(
            name: "GrafKit"
        ),
        .executableTarget(
            name: "Graf",
            dependencies: ["GrafCore", "GrafKit"]
        ),
        .testTarget(
            name: "GrafKitTests",
            dependencies: ["GrafKit"]
        ),
    ]
)
