// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "RAMMonitorTray",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(
            name: "RAMMonitorTray",
            resources: [.process("Resources")]
        )
    ]
)
