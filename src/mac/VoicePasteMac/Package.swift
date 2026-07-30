// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "VoicePasteMac",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .executable(name: "VoicePasteMac", targets: ["VoicePasteMac"])
    ],
    targets: [
        .executableTarget(
            name: "VoicePasteMac",
            path: "Sources/VoicePasteMac"
        )
    ]
)
