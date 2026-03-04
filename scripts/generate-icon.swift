#!/usr/bin/env swift
import AppKit

// NSApplication must be initialized before any AppKit/SF Symbol rendering can occur.
// .prohibited prevents the app from appearing in the Dock or taking focus.
let app = NSApplication.shared
app.setActivationPolicy(.prohibited)

// The SF Symbol used as the app icon
let symbolName = "doc.on.clipboard"

// All required sizes for a macOS .iconset directory.
// @2x entries are the HiDPI (Retina) variants of the same logical size.
let sizes: [(String, Int)] = [
    ("icon_16x16.png", 16),
    ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32),
    ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128),
    ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256),
    ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512),
    ("icon_512x512@2x.png", 1024),
]

// Output directory is passed as the first CLI argument (e.g. the project root)
let outputDir = CommandLine.arguments[1]
let iconsetPath = "\(outputDir)/Cliphop.iconset"

// Create the .iconset directory; iconutil will later convert it to an .icns file
try! FileManager.default.createDirectory(
    atPath: iconsetPath, withIntermediateDirectories: true
)

for (filename, size) in sizes {
    let cgSize = CGFloat(size)

    guard let baseImage = NSImage(
        systemSymbolName: symbolName, accessibilityDescription: nil
    ) else {
        fatalError("SF Symbol '\(symbolName)' not found")
    }

    // Scale the symbol to 60% of the canvas so it has comfortable padding
    let config = NSImage.SymbolConfiguration(
        pointSize: cgSize * 0.6, weight: .regular
    )
    let image = baseImage.withSymbolConfiguration(config)!

    // Create an RGBA bitmap at the exact pixel dimensions required
    let bitmapRep = NSBitmapImageRep(
        bitmapDataPlanes: nil,
        pixelsWide: size, pixelsHigh: size,
        bitsPerSample: 8, samplesPerPixel: 4,
        hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB,
        bytesPerRow: 0, bitsPerPixel: 0
    )!
    // Set the point size equal to pixel size so drawing coordinates map 1:1
    bitmapRep.size = NSSize(width: cgSize, height: cgSize)

    // Redirect drawing into the bitmap context
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmapRep)

    // Center the symbol on the canvas
    let imageSize = image.size
    let x = (cgSize - imageSize.width) / 2
    let y = (cgSize - imageSize.height) / 2
    image.draw(in: NSRect(x: x, y: y, width: imageSize.width, height: imageSize.height))

    NSGraphicsContext.restoreGraphicsState()

    // Encode to PNG and write to the iconset directory
    let pngData = bitmapRep.representation(using: .png, properties: [:])!
    try! pngData.write(to: URL(fileURLWithPath: "\(iconsetPath)/\(filename)"))
}

print("Iconset created at \(iconsetPath)")
exit(0)
