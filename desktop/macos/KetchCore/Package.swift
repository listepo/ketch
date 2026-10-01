// swift-tools-version: 6.0
// KetchCore: ketch's Rust core (crates/ketch-ffi) as a Swift package — the
// XCFramework and the UniFFI bindings `just xcframework` builds into this
// folder, both gitignored, plus the tests that drive them.
//
// The app depends on this package by path; nothing here is published.

import PackageDescription

let package = Package(
    name: "KetchCore",
    // The app's deployment target, which the static library is built for.
    platforms: [.macOS("26.0")],
    products: [
        .library(name: "KetchCore", targets: ["KetchCore"])
    ],
    targets: [
        // Named after the C module UniFFI generates, which the bindings import.
        .binaryTarget(name: "ketch_ffiFFI", path: "KetchFFI.xcframework"),
        .target(
            name: "KetchCore",
            dependencies: ["ketch_ffiFFI"],
            linkerSettings: [
                // The system frameworks the core's HTTP and TLS stack calls.
                .linkedFramework("Security"),
                .linkedFramework("SystemConfiguration"),
                .linkedFramework("CoreFoundation"),
            ]
        ),
        .testTarget(name: "KetchCoreTests", dependencies: ["KetchCore"]),
    ]
)
