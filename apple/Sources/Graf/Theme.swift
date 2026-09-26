import AppKit
import SwiftUI

/// Shared colors and type. Views never hardcode a color or font; they read
/// it from here so light and dark mode stay consistent.
enum Theme {
    // MARK: Color

    /// Prose and primary text.
    static let ink = NSColor.labelColor
    /// Command names, braces, and other markup that steps back from prose.
    static let markup = NSColor.tertiaryLabelColor
    /// Comments sit furthest back.
    static let comment = NSColor.quaternaryLabelColor
    /// Paragraphs outside the one being written, in Focus mode.
    static let dimmed = NSColor.tertiaryLabelColor
    /// The only accent: links between source and output (references, the
    /// caret, the sync marker).
    static let link = NSColor(name: "graf.link") { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor(srgbRed: 0.55, green: 0.62, blue: 1.0, alpha: 1)
            : NSColor(srgbRed: 0.10, green: 0.05, blue: 0.67, alpha: 1)
    }
    /// Only for the broken token and its hint.
    static let error = NSColor.systemRed
    static let background = NSColor.textBackgroundColor
    /// Quiet surfaces: folded blocks, the preview well.
    static let surface = NSColor.underPageBackgroundColor

    // MARK: Type

    /// Prose size when no setting applies (the setting defaults to this too).
    static let defaultProseSize: CGFloat = 19
    static let columnWidth: CGFloat = 660

    static func prose(size: CGFloat, weight: NSFont.Weight = .regular, italic: Bool = false) -> NSFont {
        var descriptor = NSFont.systemFont(ofSize: size, weight: weight).fontDescriptor
        descriptor = descriptor.withDesign(.serif) ?? descriptor
        if italic {
            descriptor = descriptor.withSymbolicTraits(.italic)
        }
        return NSFont(descriptor: descriptor, size: size) ?? .systemFont(ofSize: size, weight: weight)
    }

    /// Markup is set smaller than prose so it steps back, scaled from the
    /// prose size (13pt beside 19pt prose).
    static func markupFont(prose: CGFloat, emphasis: Bool = false) -> NSFont {
        .monospacedSystemFont(ofSize: (prose * (emphasis ? 0.74 : 0.68)).rounded(), weight: .regular)
    }

    /// Heading sizes by level, relative to prose: title or chapter, section,
    /// subsection, then prose size.
    static func headingFont(level: Int, prose: CGFloat) -> NSFont {
        let scales: [CGFloat] = [1.58, 1.37, 1.16]
        let size = level < scales.count ? prose * scales[level] : prose
        return self.prose(size: size.rounded(), weight: .semibold)
    }

    // Built per use: NSParagraphStyle is not Sendable, so it cannot be a
    // shared static, and one allocation per restyled paragraph is cheap.
    static var proseParagraph: NSParagraphStyle {
        let style = NSMutableParagraphStyle()
        style.lineHeightMultiple = 1.3
        style.paragraphSpacing = 6
        return style
    }

    static var plainParagraph: NSParagraphStyle {
        let style = NSMutableParagraphStyle()
        style.lineHeightMultiple = 1.2
        return style
    }
}

extension Color {
    static let grafLink = Color(nsColor: Theme.link)
    static let grafError = Color(nsColor: Theme.error)
    static let grafSurface = Color(nsColor: Theme.surface)
}
