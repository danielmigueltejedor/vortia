// swift-tools-version: 6.0
// SPDX-License-Identifier: GPL-3.0-or-later

import PackageDescription

// Vortia Native: the SwiftUI client of vortia-engine.
//
//   VortiaKit     the wire format, the case setup and the engine process. Foundation only.
//   VortiaShared  the state and the views. SwiftUI and Charts, no AppKit.
//   VortiaMac     what only macOS has: the app, its menus and its file panels.
let package = Package(
    name: "Vortia",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "VortiaKit", targets: ["VortiaKit"]),
        .library(name: "VortiaShared", targets: ["VortiaShared"]),
        .executable(name: "VortiaMac", targets: ["VortiaMac"]),
    ],
    targets: [
        .target(name: "VortiaKit"),
        .target(name: "VortiaShared", dependencies: ["VortiaKit"]),
        .executableTarget(name: "VortiaMac", dependencies: ["VortiaShared", "VortiaKit"]),
        .testTarget(
            name: "VortiaKitTests", dependencies: ["VortiaKit"], resources: [.copy("Fixtures")]),
        .testTarget(name: "VortiaSharedTests", dependencies: ["VortiaShared", "VortiaKit"]),
    ]
)
