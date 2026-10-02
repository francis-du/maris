// Offline visual-review utility: rasterize the actual Ratatui cell buffer, not a mock layout.
// No app windows, audio capture, network access or system-setting writes.
import AppKit
import Foundation

struct Cell: Decodable {
    let x: Int
    let y: Int
    let text: String
    let fg: [Int]
    let bg: [Int]
    let bold: Bool
    let display_width: Int?
}
struct Snapshot: Decodable {
    let width: Int
    let height: Int
    let label: String
    let cells: [Cell]
}
func color(_ components: [Int]) -> NSColor {
    guard components.count == 3 else { return .white }
    return NSColor(calibratedRed: CGFloat(components[0]) / 255,
                   green: CGFloat(components[1]) / 255,
                   blue: CGFloat(components[2]) / 255, alpha: 1)
}
func render(_ path: String) throws {
    let input = URL(fileURLWithPath: path)
    let data = try Data(contentsOf: input)
    guard data.count <= 8_000_000 else { throw NSError(domain: "UIReview", code: 1) }
    let snapshot = try JSONDecoder().decode(Snapshot.self, from: data)
    guard snapshot.width > 0 && snapshot.width <= 300 && snapshot.height > 0 && snapshot.height <= 100 else {
        throw NSError(domain: "UIReview", code: 2)
    }
    let normal = NSFont.monospacedSystemFont(ofSize: 14, weight: .regular)
    let bold = NSFont.monospacedSystemFont(ofSize: 14, weight: .semibold)
    let cellWidth = ceil(("M" as NSString).size(withAttributes: [.font: normal]).width)
    let cellHeight: CGFloat = 21
    let margin: CGFloat = 18
    let width = Int(CGFloat(snapshot.width) * cellWidth + margin * 2)
    let height = Int(CGFloat(snapshot.height) * cellHeight + margin * 2)
    guard let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0),
        let context = NSGraphicsContext(bitmapImageRep: bitmap) else {
        throw NSError(domain: "UIReview", code: 3)
    }
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = context
    color([7,9,13]).setFill()
    NSRect(x: 0, y: 0, width: width, height: height).fill()
    func point(_ cell: Cell) -> NSPoint {
        NSPoint(x: margin + CGFloat(cell.x) * cellWidth,
                y: CGFloat(height) - margin - CGFloat(cell.y + 1) * cellHeight)
    }
    // A terminal prints a wide grapheme once, using the leading cell's style across its
    // whole span. Reset-style continuation cells are not separately printed backgrounds.
    // Use Ratatui's exported cell width instead of guessing from the raster font.
    let ordered = snapshot.cells.filter {
        $0.x >= 0 && $0.x < snapshot.width && $0.y >= 0 && $0.y < snapshot.height
    }.sorted { ($0.y, $0.x) < ($1.y, $1.x) }
    var visible: [Cell] = []
    var currentRow = -1
    var nextColumn = 0
    for cell in ordered {
        if cell.y != currentRow { currentRow = cell.y; nextColumn = 0 }
        if cell.x < nextColumn { continue }
        let span = max(1, min(cell.display_width ?? 1, snapshot.width - cell.x))
        nextColumn = cell.x + span
        visible.append(cell)
        color(cell.bg).setFill()
        NSRect(origin: point(cell), size: NSSize(width: CGFloat(span) * cellWidth, height: cellHeight)).fill()
    }
    for cell in visible {
        if cell.text.trimmingCharacters(in: .whitespaces).isEmpty { continue }
        let position = point(cell)
        (cell.text as NSString).draw(at: NSPoint(x: position.x, y: position.y + 2),
            withAttributes: [.font: cell.bold ? bold : normal, .foregroundColor: color(cell.fg)])
    }
    NSGraphicsContext.restoreGraphicsState()
    guard let png = bitmap.representation(using: .png, properties: [:]) else {
        throw NSError(domain: "UIReview", code: 4)
    }
    let output = input.deletingPathExtension().appendingPathExtension("png")
    try png.write(to: output, options: .atomic)
    print("Rendered \(output.lastPathComponent) [\(width)x\(height)]")
}

do {
    guard CommandLine.arguments.count > 1 else {
        throw NSError(domain: "UIReviewUsage: render_console.swift snapshot.json ...", code: 64)
    }
    for path in CommandLine.arguments.dropFirst() { try render(path) }
} catch {
    fputs("UI render failed: \(error)\n", stderr)
    exit(1)
}
