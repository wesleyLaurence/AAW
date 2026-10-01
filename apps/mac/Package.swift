// swift-tools-version: 6.0
import Foundation
import PackageDescription

// libsndfile, which the Rust engine decodes samples with. build.sh sets this
// from Homebrew as the engine's build does.
let sndfile = ProcessInfo.processInfo.environment["SNDFILE_LIB_DIR"] ?? "/opt/homebrew/lib"

let package = Package(
    name: "AAW",
    platforms: [.macOS(.v14)],
    targets: [
        // The Rust core as a static library with its C header, made by build.sh.
        .binaryTarget(name: "aaw_ffiFFI", path: "Generated/aaw_ffiFFI.xcframework"),
        // The Swift bindings UniFFI generates, copied here by build.sh.
        .target(
            name: "AAWCore",
            dependencies: ["aaw_ffiFFI"],
            swiftSettings: [.swiftLanguageMode(.v5)],
            linkerSettings: [
                .unsafeFlags(["-L\(sndfile)"]),
                .linkedLibrary("sndfile"),
                .linkedFramework("CoreAudio"),
                .linkedFramework("AudioToolbox"),
                .linkedFramework("AudioUnit"),
            ]
        ),
        .target(name: "AAWApp", dependencies: ["AAWCore"]),
        .executableTarget(name: "AAW", dependencies: ["AAWApp"]),
        .testTarget(name: "AAWAppTests", dependencies: ["AAWApp"]),
    ]
)
