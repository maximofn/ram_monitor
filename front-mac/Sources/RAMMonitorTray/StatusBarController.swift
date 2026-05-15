import AppKit
import Foundation

private let repoURL = URL(string: "https://github.com/maximofn/ram_monitor")!
private let coffeeURL = URL(string: "https://www.buymeacoffee.com/maximofn")!
private let compactModeDefaultsKey = "RAMMonitorTray.compactMode"

enum TrayState: Sendable {
    case connecting
    case connected(Snapshot)
    case disconnected(String)
}

@MainActor
final class StatusBarController: NSObject {
    private let statusItem: NSStatusItem
    private let renderer: IconRenderer
    private let backendURL: String
    private var state: TrayState = .connecting
    private var lastAppearance: IconAppearance = .dark
    private var lastRenderedKey: String = ""
    private var compactMode: Bool

    init(renderer: IconRenderer, backendURL: String) {
        self.renderer = renderer
        self.backendURL = backendURL
        self.compactMode = UserDefaults.standard.bool(forKey: compactModeDefaultsKey)
        self.statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        super.init()
        if let button = statusItem.button {
            button.imagePosition = .imageLeft
            button.toolTip = "RAM Monitor — connecting to \(backendURL)"
        }
        // Subscribe to the system-wide light/dark toggle. Do NOT KVO
        // `effectiveAppearance` — AppKit re-evaluates it during normal repaints,
        // creating a refresh→paint→KVO feedback loop.
        DistributedNotificationCenter.default.addObserver(
            self,
            selector: #selector(appearanceChanged),
            name: Notification.Name("AppleInterfaceThemeChangedNotification"),
            object: nil
        )
        lastAppearance = currentAppearance
        applyState(.connecting)
    }

    deinit {
        DistributedNotificationCenter.default.removeObserver(self)
    }

    @objc private func appearanceChanged() {
        Task { @MainActor in
            self.lastAppearance = self.currentAppearance
            self.lastRenderedKey = ""
            self.refreshIcon()
        }
    }

    func applyState(_ new: TrayState) {
        state = new
        refreshIcon()
        refreshMenu()
        refreshTooltip()
    }

    private var currentAppearance: IconAppearance {
        let appearance = statusItem.button?.effectiveAppearance ?? NSApp.effectiveAppearance
        let match = appearance.bestMatch(from: [.darkAqua, .vibrantDark, .aqua, .vibrantLight])
        switch match {
        case .darkAqua, .vibrantDark: return .dark
        default: return .light
        }
    }

    private func refreshIcon() {
        let (snapshot, connected): (Snapshot?, Bool) = {
            switch state {
            case .connected(let snap): return (snap, true)
            default: return (nil, false)
            }
        }()
        // Dedupe identical renders — when visible fields haven't changed, skip
        // both CGContext work and the AppKit image swap. The backend ticks at
        // ~1 Hz; most ticks have identical visible state.
        let key = renderKey(snapshot: snapshot, connected: connected, appearance: lastAppearance)
        if key == lastRenderedKey { return }
        lastRenderedKey = key
        if let img = renderer.renderImage(snapshot: snapshot, connected: connected, appearance: lastAppearance, compact: compactMode) {
            statusItem.button?.image = img
        }
    }

    private func renderKey(snapshot: Snapshot?, connected: Bool, appearance: IconAppearance) -> String {
        var parts: [String] = ["\(connected)", "\(appearance)", "compact=\(compactMode)"]
        if let s = snapshot {
            // Visible inputs only: used GiB rounded, total GiB ceil, used percent rounded.
            let usedGi = Int(s.memory.usedGiB.rounded())
            let totalGi = Int(s.memory.totalGiB.rounded(.up))
            let pct = Int(s.memory.usedPercent.rounded())
            parts.append("\(usedGi)/\(totalGi):\(pct)")
        }
        return parts.joined(separator: "|")
    }

    private func refreshTooltip() {
        guard let button = statusItem.button else { return }
        switch state {
        case .connecting:
            button.toolTip = "RAM Monitor — connecting to \(backendURL)"
        case .connected(let snap):
            let used = formatBytes(snap.memory.usedBytes)
            let total = formatBytes(snap.memory.totalBytes)
            let avail = formatBytes(snap.memory.availableBytes)
            let pct = Int(snap.memory.usedPercent.rounded())
            var lines = [
                "\(snap.host) — kernel \(snap.kernel ?? "unknown")",
                "RAM: \(used) / \(total) (\(pct)% used)",
                "Available: \(avail)",
            ]
            if snap.swap.totalBytes > 0 {
                lines.append(
                    "Swap: \(formatBytes(snap.swap.usedBytes)) / \(formatBytes(snap.swap.totalBytes)) (\(Int(snap.swap.usedPercent.rounded()))%)"
                )
            }
            button.toolTip = lines.joined(separator: "\n")
        case .disconnected(let err):
            button.toolTip = "Backend offline: \(err)"
        }
    }

    private func refreshMenu() {
        let menu = NSMenu()
        menu.autoenablesItems = false

        switch state {
        case .connecting:
            menu.addItem(disabledItem("Connecting to \(backendURL)…"))
            menu.addItem(.separator())
        case .disconnected(let err):
            menu.addItem(disabledItem("Backend offline: \(err)"))
            menu.addItem(disabledItem("Backend: \(backendURL)"))
            menu.addItem(.separator())
        case .connected(let snap):
            let pct = Int(snap.memory.usedPercent.rounded())
            menu.addItem(disabledItem(
                "RAM: \(formatBytes(snap.memory.usedBytes)) / \(formatBytes(snap.memory.totalBytes)) (\(pct)%)"
            ))
            menu.addItem(disabledItem("Available: \(formatBytes(snap.memory.availableBytes))"))
            menu.addItem(disabledItem("Free: \(formatBytes(snap.memory.freeBytes))"))
            menu.addItem(disabledItem("Buffers: \(formatBytes(snap.memory.buffersBytes))"))
            menu.addItem(disabledItem("Cached: \(formatBytes(snap.memory.cachedBytes))"))
            menu.addItem(.separator())
            if snap.swap.totalBytes > 0 {
                let spct = Int(snap.swap.usedPercent.rounded())
                menu.addItem(disabledItem(
                    "Swap: \(formatBytes(snap.swap.usedBytes)) / \(formatBytes(snap.swap.totalBytes)) (\(spct)%)"
                ))
            } else {
                menu.addItem(disabledItem("Swap: disabled"))
            }
            menu.addItem(.separator())
            if !snap.processes.isEmpty {
                menu.addItem(disabledItem("Top processes (\(snap.processes.count))"))
                for proc in snap.processes {
                    let line = String(
                        format: "  %6d %@ — %@ (%.1f%%)",
                        proc.pid,
                        proc.name as NSString,
                        formatBytes(proc.rssBytes) as NSString,
                        proc.memoryPercent
                    )
                    menu.addItem(disabledItem(line))
                }
                menu.addItem(.separator())
            }
            var backendLine = "Backend: \(backendURL)"
            if let kernel = snap.kernel {
                backendLine += " — kernel \(kernel)"
            }
            menu.addItem(disabledItem(backendLine))
            menu.addItem(disabledItem("Updated: \(shortTime(snap.timestamp))"))
            menu.addItem(.separator())
        }

        let toggleTitle = compactMode ? "Cambiar a extendido" : "Cambiar a compacto"
        let toggle = NSMenuItem(title: toggleTitle, action: #selector(toggleCompactMode), keyEquivalent: "")
        toggle.target = self
        menu.addItem(toggle)
        menu.addItem(.separator())

        let repo = NSMenuItem(title: "Repository", action: #selector(openRepo), keyEquivalent: "")
        repo.target = self
        menu.addItem(repo)
        let coffee = NSMenuItem(title: "Buy me a coffee", action: #selector(openCoffee), keyEquivalent: "")
        coffee.target = self
        menu.addItem(coffee)
        menu.addItem(.separator())
        let quit = NSMenuItem(title: "Quit", action: #selector(quit), keyEquivalent: "q")
        quit.target = self
        menu.addItem(quit)

        statusItem.menu = menu
    }

    @objc private func openRepo() { NSWorkspace.shared.open(repoURL) }
    @objc private func openCoffee() { NSWorkspace.shared.open(coffeeURL) }
    @objc private func quit() { NSApp.terminate(nil) }

    @objc private func toggleCompactMode() {
        compactMode.toggle()
        UserDefaults.standard.set(compactMode, forKey: compactModeDefaultsKey)
        lastRenderedKey = ""
        refreshIcon()
        refreshMenu()
    }
}

// MARK: - Helpers

private func disabledItem(_ title: String) -> NSMenuItem {
    let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
    item.isEnabled = false
    return item
}

private func formatBytes(_ bytes: UInt64) -> String {
    let gib: Double = 1024 * 1024 * 1024
    let mib: Double = 1024 * 1024
    let kib: Double = 1024
    let b = Double(bytes)
    if b >= gib { return String(format: "%.2f GiB", b / gib) }
    if b >= mib { return String(format: "%.0f MiB", b / mib) }
    if b >= kib { return String(format: "%.0f KiB", b / kib) }
    return "\(bytes) B"
}

/// "2026-05-06T10:11:12.345Z" → "10:11:12". Mirrors the rust short_time helper.
private func shortTime(_ rfc3339: String) -> String {
    guard let tIdx = rfc3339.firstIndex(of: "T") else { return rfc3339 }
    let after = rfc3339[rfc3339.index(after: tIdx)...]
    if let dot = after.firstIndex(of: ".") {
        return String(after[..<dot])
    }
    if let plus = after.firstIndex(where: { $0 == "+" || $0 == "Z" || $0 == "-" }) {
        return String(after[..<plus])
    }
    return String(after)
}
