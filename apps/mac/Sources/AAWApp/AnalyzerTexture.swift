import AppKit

/// A picture that scrolls with time: a bitmap of `width` columns and
/// `height` rows, a column written for each moment measured, the newest at
/// the right and the oldest falling off the left, drawn into a box as an
/// image in the slices `AnalyzerLayout.slices` gives. The image is made
/// over the pixels themselves, so a frame drawn copies nothing and costs
/// one image draw, however many columns the texture holds. Used from the
/// view that owns it, on the main thread.
final class ScrollingTexture {
    let width: Int
    let height: Int
    /// Row 0 at the top, each pixel `0xAARRGGBB`, premultiplied.
    private let pixels: UnsafeMutablePointer<UInt32>
    /// Columns written so far; the texture holds the last `width`.
    private(set) var written = 0

    init(width: Int, height: Int) {
        self.width = max(width, 1)
        self.height = max(height, 1)
        pixels = .allocate(capacity: self.width * self.height)
        pixels.initialize(repeating: 0, count: self.width * self.height)
    }

    deinit {
        pixels.deallocate()
    }

    /// Nothing written.
    func clear() {
        pixels.update(repeating: 0, count: width * height)
        written = 0
    }

    /// The next column, each row's pixel from `pixel`, row 0 at the top.
    func append(_ pixel: (Int) -> UInt32) {
        let column = written % width
        for row in 0..<height {
            pixels[row * width + column] = pixel(row)
        }
        written += 1
    }

    /// A column of nothing: time that passed unmeasured.
    func appendBlank() {
        append { _ in 0 }
    }

    /// A column with the rows from `top` to `bottom` in one color and the
    /// rest clear, as a waveform's column is.
    func append(from top: Int, to bottom: Int, color: UInt32) {
        append { row in row >= top && row <= bottom ? color : 0 }
    }

    /// The texture as an image over its pixels, with no copy.
    private func image() -> CGImage? {
        guard let provider = CGDataProvider(dataInfo: nil, data: pixels, size: width * height * 4, releaseData: { _, _, _ in }),
              let space = CGColorSpace(name: CGColorSpace.sRGB) else { return nil }
        let info = CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue)
        return CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4, space: space,
                       bitmapInfo: info, provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
    }

    /// Draws the texture across the box in a flipped context, the newest
    /// column at the box's right edge, the whole texture's width across it.
    func draw(in box: CGRect, context: CGContext) {
        guard written > 0, let image = image() else { return }
        context.saveGState()
        context.clip(to: box)
        context.interpolationQuality = .medium
        for slice in AnalyzerLayout.slices(written: written, width: width, in: box) {
            guard let part = image.cropping(to: CGRect(x: slice.columns.lowerBound, y: 0, width: slice.columns.count, height: height)) else { continue }
            // An image draws upright in an unflipped space.
            context.saveGState()
            context.translateBy(x: 0, y: slice.rect.minY + slice.rect.maxY)
            context.scaleBy(x: 1, y: -1)
            context.draw(part, in: slice.rect)
            context.restoreGState()
        }
        context.restoreGState()
    }
}

extension NSColor {
    /// The color as a texture's pixel, `0xAARRGGBB` premultiplied.
    var pixel: UInt32 {
        let c = usingColorSpace(.sRGB) ?? self
        let a = c.alphaComponent
        let channel = { (v: CGFloat) in UInt32((min(max(v * a, 0), 1) * 255).rounded()) }
        return UInt32((min(max(a, 0), 1) * 255).rounded()) << 24 | channel(c.redComponent) << 16 | channel(c.greenComponent) << 8 | channel(c.blueComponent)
    }
}
