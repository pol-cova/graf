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

    /// The paper a rendered page sits on. Pages are white in both
    /// appearances; a dark-mode surface here would frame them as dark
    /// rectangles, which is not what a PDF looks like.
    static let page = NSColor.white

    // MARK: Type

    /// Prose size when no setting applies (the setting defaults to this too).
    static let defaultProseSize: CGFloat = 19
    static let columnWidth: CGFloat = 660

    /// Type for interface chrome, named by role so a size is chosen once and
    /// reused rather than retyped per view: `.caption` is the small tertiary
    /// line, `.callout` the quiet hint under a control or beside a count, and
    /// so on up to `.wordmark` for the launch screen.
    ///
    /// These are `Font`, not `NSFont`, because every consumer is SwiftUI's
    /// `.font(_:)`. An earlier revision also declared an `NSFont` twin of
    /// each size; nothing used them once the call sites settled on SwiftUI, so
    /// they are gone rather than kept "for AppKit interop" nobody needs.
    ///
    /// Computed rather than stored: `Font` is a struct but the sizes are
    /// derived, and a stored static would need a `Sendable` guarantee for no
    /// benefit at these counts.
    enum Chrome {
        /// 11pt. Tertiary detail: "opens here", a file path in monospaced.
        static var captionUI: Font { .system(size: 11) }
        static var captionMonoUI: Font { .system(size: 11, design: .monospaced) }

        /// 12pt. The common quiet label: hints, secondary lines, page
        /// numbers, diagnostics, section captions.
        static var calloutUI: Font { .system(size: 12) }
        static var calloutMonoUI: Font { .system(size: 12, design: .monospaced) }

        /// 13pt. Row content: a template name, a status line, a detail value.
        static var rowUI: Font { .system(size: 13) }
        static var rowMediumUI: Font { .system(size: 13, weight: .medium) }
        static var rowMonoUI: Font { .system(size: 13, design: .monospaced) }
        static var rowMonoSemiboldUI: Font { .system(size: 13, weight: .semibold, design: .monospaced) }

        /// 14pt. A subtitle, or the title of a selected list row.
        static var bodyUI: Font { .system(size: 14) }
        static var bodyEmphasizedUI: Font { .system(size: 14, weight: .medium) }

        /// 17pt. A panel or sheet section heading; the selected Quick Open row.
        static var headingUI: Font { .system(size: 17) }
        static var headingSerifUI: Font { .system(size: 17, weight: .semibold, design: .serif) }

        /// 24pt. The title of a sheet.
        static var sheetTitleUI: Font { .system(size: 24, weight: .semibold, design: .serif) }

        /// 40pt. The launch screen wordmark.
        static var wordmarkUI: Font { .system(size: 40, weight: .semibold, design: .serif) }
    }

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
