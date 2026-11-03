// Renders the mantra syllables into assets/tibetan_atlas.png using the
// macOS system Tibetan font (Kailasa). Cell order must match GLYPHS in
// src/main.rs. Run: swift scripts/gen-atlas.swift
import AppKit
import CoreText
import Foundation

let syllables: [String] = [
    "ཨོཾ",      //  0 om
    "མ",        //  1 ma
    "ཎི",       //  2 ṇi
    "པདྨེ",      //  3 padme
    "ཧཱུྃ",      //  4 hūṃ
    "བཛྲ",      //  5 vajra
    "སཏྭ",      //  6 sattva
    "ཨཱཿ",      //  7 āḥ
    "ན",        //  8 na
    "མོ",       //  9 mo
    "ཨ",        // 10 a
    "མི",       // 11 mi
    "ཏཱ",       // 12 tā
    "བྷ",       // 13 bha
    "བུདྡྷཱ",     // 14 buddhā
    "ཡ",        // 15 ya
    "དྷརྨཱ",     // 16 dharmā
    "སངྒྷཱ",     // 17 saṅghā
]

let cols = 6, rows = 3, cell = 256
let fill: CGFloat = 0.84 // fraction of the cell the ink may occupy (leaves spacing)
let boldness: CGFloat = -2.0 // CoreText stroke width in % of font size; negative = fill + stroke
let fontName = "Kailasa"
let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "assets/tibetan_atlas.png"

let cs = CGColorSpaceCreateDeviceRGB()
guard let ctx = CGContext(data: nil, width: cols * cell, height: rows * cell, bitsPerComponent: 8,
                          bytesPerRow: 0, space: cs,
                          bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { exit(1) }
// `swift scripts/gen-atlas.swift preview.png --opaque` paints a black
// background so the white glyphs can be eyeballed in an image viewer.
if CommandLine.arguments.contains("--opaque") {
    ctx.setFillColor(CGColor(red: 0, green: 0, blue: 0, alpha: 1))
    ctx.fill(CGRect(x: 0, y: 0, width: cols * cell, height: rows * cell))
}
ctx.setAllowsAntialiasing(true)
ctx.setShouldSmoothFonts(true)

func line(_ s: String, size: CGFloat) -> CTLine {
    let font = CTFontCreateWithName(fontName as CFString, size, nil)
    let attrs: [NSAttributedString.Key: Any] = [
        .font: font, .foregroundColor: NSColor.white,
        .strokeColor: NSColor.white, .strokeWidth: boldness,
    ]
    return CTLineCreateWithAttributedString(NSAttributedString(string: s, attributes: attrs))
}

for (i, s) in syllables.enumerated() {
    // Measure at a probe size, then scale the font so the ink fits `fill` of the cell.
    // CTLineGetImageBounds is relative to the context's current text position.
    let probe: CGFloat = 100
    ctx.textPosition = .zero
    let b0 = CTLineGetImageBounds(line(s, size: probe), ctx)
    let scale = fill * CGFloat(cell) / max(b0.width, b0.height)
    let l = line(s, size: probe * scale)
    ctx.textPosition = .zero
    let b = CTLineGetImageBounds(l, ctx)
    let col = i % cols, row = i / cols
    // CoreGraphics origin is bottom-left; row 0 goes at the top.
    let ox = CGFloat(col * cell) + (CGFloat(cell) - b.width) / 2 - b.minX
    let oy = CGFloat((rows - 1 - row) * cell) + (CGFloat(cell) - b.height) / 2 - b.minY
    ctx.textPosition = CGPoint(x: ox, y: oy)
    CTLineDraw(l, ctx)
}

guard let img = ctx.makeImage(),
      let dest = CGImageDestinationCreateWithURL(URL(fileURLWithPath: out) as CFURL, "public.png" as CFString, 1, nil)
else { exit(2) }
CGImageDestinationAddImage(dest, img, nil)
guard CGImageDestinationFinalize(dest) else { exit(3) }
print("wrote \(out) \(cols * cell)x\(rows * cell), \(syllables.count) cells")
