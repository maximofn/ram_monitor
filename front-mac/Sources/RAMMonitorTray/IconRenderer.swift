import AppKit
import CoreGraphics
import CoreText
import Foundation
import ImageIO
import UniformTypeIdentifiers

// Mirror of crates/ram-monitor-tray/src/icon/render.rs for the donut geometry,
// adapted to macOS conventions:
//
// 1. The label text is always white. The macOS menu bar is consistently dark
//    enough (translucent over wallpaper, never plain white) that white text
//    reads everywhere — and it matches the system convention used by the
//    clock, battery, wifi, etc. The linux tray colors the label by swap
//    pressure; the menu/tooltip surfaces that signal here instead.
// 2. The donut colors stay saturated and convey state on their own — that's
//    where the "memory critical" signal lives.
enum IconAppearance: Sendable {
    case dark
    case light
}

enum IconColors {
    // Donut "memory free" / "memory used" colors — same on both appearances.
    static let free   = CGColor(red: 0x66/255.0, green: 0xb3/255.0, blue: 0xff/255.0, alpha: 1.0)
    static let ok     = CGColor(red: 0x33/255.0, green: 0xb0/255.0, blue: 0x33/255.0, alpha: 1.0)
    static let warn1Donut = CGColor(red: 0xe6/255.0, green: 0xb8/255.0, blue: 0x00/255.0, alpha: 1.0)
    static let warn2Donut = CGColor(red: 0xff/255.0, green: 0xa0/255.0, blue: 0x40/255.0, alpha: 1.0)
    static let highDonut  = CGColor(red: 0xe0/255.0, green: 0x33/255.0, blue: 0x33/255.0, alpha: 1.0)

    static let dimFree = CGColor(red: 0x80/255.0, green: 0x80/255.0, blue: 0x80/255.0, alpha: 1.0)
    static let dimUsed = CGColor(red: 0x60/255.0, green: 0x60/255.0, blue: 0x60/255.0, alpha: 1.0)

    static func text(_ a: IconAppearance) -> CGColor {
        CGColor(red: 1, green: 1, blue: 1, alpha: 1)
    }
    static func dimText(_ a: IconAppearance) -> CGColor {
        CGColor(red: 0xbb/255.0, green: 0xbb/255.0, blue: 0xbb/255.0, alpha: 1.0)
    }
}

private let donutPadding: CGFloat = 2
private let innerLabelSize: CGFloat = 8

/// Picks the donut "used" color from memory utilization, matching `render.rs::used_color`.
private func usedColor(_ pct: Float) -> CGColor {
    if pct >= 90 { return IconColors.highDonut }
    if pct >= 80 { return IconColors.warn2Donut }
    if pct >= 70 { return IconColors.warn1Donut }
    return IconColors.ok
}

/// Text size matches the rust tray: floor(0.45 * height), clamped to [8, 16].
/// Fractional sizes produce blurry glyphs — the linux tray learned this the hard way.
private func textSize(forHeight h: CGFloat) -> CGFloat {
    let raw = (h * 0.45).rounded()
    return max(8, min(16, raw))
}

/// Format mirrors render.rs::label_text: " 8/32GB" or "12/32GB" — width-2 used
/// so the donut never shifts as occupancy crosses 10 GiB. Total uses ceil() to
/// match what users read on hardware labels (32 GB stick reports as 31.24 GiB
/// in /proc/meminfo). Used stays at round() so the figure tracks reality.
private func labelText(snapshot: Snapshot?) -> String {
    guard let snap = snapshot else { return "  --GB" }
    let used = Int(snap.memory.usedGiB.rounded())
    let total = Int(snap.memory.totalGiB.rounded(.up))
    return String(format: "%2d/%-2dGB", used, total)
}

struct IconRenderer {
    /// Logical height in points (status bar shows ~22 pt). The bitmap is rendered
    /// at 2× this for Retina; macOS scales the NSImage back down by its `size`.
    let height: CGFloat
    let baseIcon: CGImage?

    init(height: CGFloat) {
        self.height = height
        self.baseIcon = Self.loadBaseIcon(targetHeight: height)
    }

    @MainActor
    func renderImage(snapshot: Snapshot?, connected: Bool, appearance: IconAppearance, compact: Bool = false) -> NSImage? {
        guard let result = renderCGImage(snapshot: snapshot, connected: connected, appearance: appearance, compact: compact) else {
            return nil
        }
        let img = NSImage(cgImage: result.cgImage, size: result.logicalSize)
        img.isTemplate = false
        return img
    }

    func renderPNG(
        snapshot: Snapshot?,
        connected: Bool,
        to path: String,
        appearance: IconAppearance = .dark,
        compact: Bool = false
    ) throws {
        guard let result = renderCGImage(snapshot: snapshot, connected: connected, appearance: appearance, compact: compact) else {
            throw NSError(domain: "IconRenderer", code: 1,
                          userInfo: [NSLocalizedDescriptionKey: "render failed"])
        }
        let url = URL(fileURLWithPath: path)
        guard let dest = CGImageDestinationCreateWithURL(
            url as CFURL, UTType.png.identifier as CFString, 1, nil
        ) else {
            throw NSError(domain: "IconRenderer", code: 2,
                          userInfo: [NSLocalizedDescriptionKey: "could not create PNG destination"])
        }
        CGImageDestinationAddImage(dest, result.cgImage, nil)
        guard CGImageDestinationFinalize(dest) else {
            throw NSError(domain: "IconRenderer", code: 3,
                          userInfo: [NSLocalizedDescriptionKey: "PNG encode failed"])
        }
    }

    private struct RenderResult {
        let cgImage: CGImage
        let logicalSize: CGSize
    }

    private func renderCGImage(snapshot: Snapshot?, connected: Bool, appearance: IconAppearance, compact: Bool) -> RenderResult? {
        let scale: CGFloat = 2
        let layout = self.layout(snapshot: snapshot, scale: scale, connected: connected, appearance: appearance, compact: compact)
        let pxW = max(1, Int(layout.totalLogicalWidth * scale))
        let pxH = max(1, Int(height * scale))

        guard let ctx = CGContext(
            data: nil,
            width: pxW,
            height: pxH,
            bitsPerComponent: 8,
            bytesPerRow: 0,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else { return nil }

        // Core Graphics origin is bottom-left. Flip vertically and scale so all
        // subsequent draw calls can use logical points with top-left origin
        // (matching the rust renderer's coordinate system).
        ctx.translateBy(x: 0, y: CGFloat(pxH))
        ctx.scaleBy(x: scale, y: -scale)

        draw(layout: layout, ctx: ctx, connected: connected)

        guard let cg = ctx.makeImage() else { return nil }
        return RenderResult(
            cgImage: cg,
            logicalSize: CGSize(width: layout.totalLogicalWidth, height: height)
        )
    }

    // MARK: - Layout

    private struct Layout {
        let totalLogicalWidth: CGFloat
        let donutSize: CGFloat
        let iconWidth: CGFloat
        let textWidth: CGFloat
        let textPx: CGFloat
        let snapshot: Snapshot?
        let connected: Bool
        let appearance: IconAppearance
        let compact: Bool
    }

    private func layout(snapshot: Snapshot?, scale: CGFloat, connected: Bool, appearance: IconAppearance, compact: Bool) -> Layout {
        let textPx = textSize(forHeight: height)
        // Probe with the widest plausible label so the donut never shifts when
        // used/total cross digit boundaries (e.g. " 9/32GB" → "10/32GB").
        let probeWidth = compact ? 0 : measureText("00/00GB", size: textPx)
        let donutSize = max(8, height - donutPadding * 2)
        let iconW: CGFloat = baseIcon.map { CGFloat($0.width) / scale } ?? 0
        let total: CGFloat
        if snapshot == nil {
            // Disconnected / connecting: icon + dash, no donut. The dash signals
            // "no data" without the visual weight of a (always grey) ring that
            // could be misread as "0% used".
            let dashW = measureText("-", size: textPx)
            total = iconW + 4 + dashW + 2
        } else if compact {
            total = iconW + 2 + donutSize
        } else {
            total = iconW + 2 + probeWidth + 2 + donutSize
        }
        return Layout(
            totalLogicalWidth: max(height, total),
            donutSize: donutSize,
            iconWidth: iconW,
            textWidth: probeWidth,
            textPx: textPx,
            snapshot: snapshot,
            connected: connected,
            appearance: appearance,
            compact: compact
        )
    }

    // MARK: - Drawing

    private func draw(layout: Layout, ctx: CGContext, connected: Bool) {
        guard let snap = layout.snapshot else {
            // Connecting / disconnected: dimmed base icon and a dash.
            var x: CGFloat = 0
            if let icon = baseIcon {
                let iconHpt = CGFloat(icon.height) / 2.0
                let iconWpt = CGFloat(icon.width) / 2.0
                let iconY = (height - iconHpt) / 2.0
                ctx.saveGState()
                ctx.interpolationQuality = .high
                ctx.setAlpha(0.4)
                ctx.translateBy(x: x, y: iconY + iconHpt)
                ctx.scaleBy(x: 1, y: -1)
                ctx.draw(icon, in: CGRect(x: 0, y: 0, width: iconWpt, height: iconHpt))
                ctx.restoreGState()
                x += iconWpt + 4
            }
            drawText(
                "-",
                ctx: ctx,
                x: x,
                size: layout.textPx,
                color: IconColors.dimText(layout.appearance),
                blockHeight: height
            )
            return
        }

        if let icon = baseIcon {
            let iconHpt = CGFloat(icon.height) / 2.0
            let iconWpt = CGFloat(icon.width) / 2.0
            let iconY = (height - iconHpt) / 2.0
            ctx.saveGState()
            ctx.interpolationQuality = .high
            ctx.translateBy(x: 0, y: iconY + iconHpt)
            ctx.scaleBy(x: 1, y: -1)
            ctx.draw(icon, in: CGRect(x: 0, y: 0, width: iconWpt, height: iconHpt))
            ctx.restoreGState()
        }

        if !layout.compact {
            let label = labelText(snapshot: snap)
            let labelColor = layout.connected
                ? IconColors.text(layout.appearance)
                : IconColors.dimText(layout.appearance)
            let textX = layout.iconWidth + 2
            drawText(
                label,
                ctx: ctx,
                x: textX,
                size: layout.textPx,
                color: labelColor,
                blockHeight: height
            )
        }

        let donutX = layout.compact
            ? (layout.iconWidth + 2)
            : (layout.iconWidth + 2 + layout.textWidth + 2)
        let usedPct = snap.memory.usedPercent
        drawDonut(
            ctx: ctx,
            x: donutX,
            y: donutPadding,
            size: layout.donutSize,
            usedPercent: usedPct,
            connected: layout.connected
        )

        // Memory percent centered inside the donut hole. 8 pt fits "100" in the
        // inner diameter without touching the ring.
        let pctText = "\(Int(usedPct.rounded()))"
        let pctW = measureText(pctText, size: innerLabelSize)
        let pctX = donutX + layout.donutSize / 2 - pctW / 2
        let pctColor = layout.connected
            ? IconColors.text(layout.appearance)
            : IconColors.dimText(layout.appearance)
        drawText(
            pctText,
            ctx: ctx,
            x: pctX,
            size: innerLabelSize,
            color: pctColor,
            blockHeight: height
        )
    }

    private func drawDonut(
        ctx: CGContext,
        x: CGFloat,
        y: CGFloat,
        size: CGFloat,
        usedPercent: Float,
        connected: Bool
    ) {
        let cx = x + size / 2
        let cy = y + size / 2
        let rOuter = size / 2
        let rInner = rOuter * 0.78

        let freeColor = connected ? IconColors.free : IconColors.dimFree
        ctx.saveGState()
        ctx.setFillColor(freeColor)
        ctx.addArc(center: CGPoint(x: cx, y: cy), radius: rOuter, startAngle: 0, endAngle: .pi * 2, clockwise: false)
        ctx.fillPath()
        ctx.restoreGState()

        if usedPercent > 0.5 {
            let color = connected ? usedColor(usedPercent) : IconColors.dimUsed
            let sweep = CGFloat(min(100, max(0, usedPercent))) / 100.0 * (.pi * 2)
            // Match the rust renderer: start at -90° (12 o'clock) sweeping clockwise.
            let start = -CGFloat.pi / 2
            let end = start + sweep
            ctx.saveGState()
            ctx.setFillColor(color)
            ctx.move(to: CGPoint(x: cx, y: cy))
            ctx.addArc(
                center: CGPoint(x: cx, y: cy),
                radius: rOuter,
                startAngle: start,
                endAngle: end,
                clockwise: false
            )
            ctx.closePath()
            ctx.fillPath()
            ctx.restoreGState()
        }

        // Punch the hole.
        ctx.saveGState()
        ctx.setBlendMode(.clear)
        ctx.addArc(center: CGPoint(x: cx, y: cy), radius: rInner, startAngle: 0, endAngle: .pi * 2, clockwise: false)
        ctx.fillPath()
        ctx.restoreGState()
    }

    // MARK: - Text (CoreText, system monospaced-digit font)

    private static func font(size: CGFloat) -> CTFont {
        // monospacedDigit, not monospaced: SF Pro with mono digits, matching
        // the system clock/battery so width doesn't jiggle as digits change.
        let nsf = NSFont.monospacedDigitSystemFont(ofSize: size, weight: .regular)
        return nsf as CTFont
    }

    private func measureText(_ text: String, size: CGFloat) -> CGFloat {
        let line = makeLine(text: text, size: size, color: IconColors.text(.dark))
        let width = CTLineGetTypographicBounds(line, nil, nil, nil)
        return CGFloat(width)
    }

    private func makeLine(text: String, size: CGFloat, color: CGColor) -> CTLine {
        let attrs: [NSAttributedString.Key: Any] = [
            .font: Self.font(size: size),
            .foregroundColor: color,
        ]
        let attributed = NSAttributedString(string: text, attributes: attrs)
        return CTLineCreateWithAttributedString(attributed)
    }

    private func drawText(
        _ text: String,
        ctx: CGContext,
        x: CGFloat,
        size: CGFloat,
        color: CGColor,
        blockHeight: CGFloat
    ) {
        let line = makeLine(text: text, size: size, color: color)
        var ascent: CGFloat = 0
        var descent: CGFloat = 0
        var leading: CGFloat = 0
        _ = CTLineGetTypographicBounds(line, &ascent, &descent, &leading)

        let baselineFromTop = ((blockHeight - size) / 2.0).rounded() + ascent

        ctx.saveGState()
        ctx.translateBy(x: x, y: baselineFromTop)
        ctx.scaleBy(x: 1, y: -1)
        ctx.textPosition = .zero
        CTLineDraw(line, ctx)
        ctx.restoreGState()
    }

    // MARK: - Base icon loading

    private static func loadBaseIcon(targetHeight: CGFloat) -> CGImage? {
        guard let url = Bundle.module.url(forResource: "ram", withExtension: "png"),
              let src = CGImageSourceCreateWithURL(url as CFURL, nil),
              let cg = CGImageSourceCreateImageAtIndex(src, 0, nil) else {
            return nil
        }

        // Resize to 2× target height so it composites cleanly at Retina.
        let pxH = Int(targetHeight * 2)
        let aspect = CGFloat(cg.width) / CGFloat(cg.height)
        let pxW = max(1, Int((targetHeight * 2 * aspect).rounded()))

        guard let ctx = CGContext(
            data: nil,
            width: pxW,
            height: pxH,
            bitsPerComponent: 8,
            bytesPerRow: 0,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else { return nil }

        ctx.interpolationQuality = .high
        ctx.draw(cg, in: CGRect(x: 0, y: 0, width: pxW, height: pxH))
        return ctx.makeImage()
    }
}
