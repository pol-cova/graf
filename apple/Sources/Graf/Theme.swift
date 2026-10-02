import AppKit
import SwiftUI

/// Shared colors and type. Views never hardcode a color or font; they read
/// it from here so light and dark mode stay consistent.
enum Theme {
    // MARK: Color

    /// Prose and primary text.
    ///
    /// Dark mode is an explicit value rather than `labelColor`. Apple's dark
    /// `labelColor` is pure white, which measures 16.7:1 against the dark
    /// background: past the point where extra contrast helps, and the letters
    /// visibly halo. A writer staring at this for an hour reads better with a
    /// warm off-white at 9.4:1, which is still AAA.
    static let ink = dynamic(light: (30, 29, 27), dark: (208, 207, 202))

    /// Command names, braces, and other markup that steps back from prose.
    ///
    /// Was `tertiaryLabelColor`, which is 3.5:1 on the dark background — below
    /// AA for the 13pt size this is set at. Now 4.8:1.
    static let markup = dynamic(light: (110, 108, 104), dark: (156, 155, 149))

    /// Comments sit furthest back.
    static let comment = dynamic(light: (128, 126, 122), dark: (128, 127, 122))

    /// Paragraphs outside the one being written, in Focus mode.
    ///
    /// This used to be `tertiaryLabelColor` — the same colour as `markup`. A
    /// command inside the paragraph being written was therefore identical to a
    /// whole unfocused paragraph, which defeats the point of Focus mode. It is
    /// now a distinct step: clearly dimmer than the focused line (2.6:1), and
    /// clearly brighter than nothing else claims.
    ///
    /// It does not reach AA against the background, and cannot without
    /// becoming hard to tell from the focused paragraph. That is the trade
    /// Focus mode is for: the surrounding argument stays readable for
    /// orientation, at lower contrast than the line being written.
    static let dimmed = dynamic(light: (150, 148, 144), dark: (116, 115, 110))

    /// The only accent: links between source and output — reference arguments,
    /// and the caret.
    static let link = dynamic(light: (26, 13, 171), dark: (122, 150, 255))

    /// Only for the broken token and its hint.
    ///
    /// `systemRed` is too saturated to sit in body text all day, and on dark
    /// it vibrates. This is the same hue pulled toward the ink and lifted in
    /// lightness.
    static let error = dynamic(light: (196, 32, 32), dark: (255, 105, 105))

    static let background = dynamic(light: (255, 255, 255), dark: (40, 40, 44))

    /// Quiet surfaces: the preview well and other recessed areas.
    static let surface = dynamic(light: (242, 241, 238), dark: (32, 32, 36))

    /// The paper a rendered page sits on. Pages are white in both
    /// appearances; a dark-mode surface here would frame them as dark
    /// rectangles, which is not what a PDF looks like.
    static let page = NSColor.white

    /// An `NSColor` that resolves per appearance from 8-bit components.
    ///
    /// Built once and cached by AppKit. A `static let` would not do: an
    /// `NSColor` is a class and is not `Sendable`, so a stored one would be
    /// shared mutable global state under Swift 6 strict concurrency.
    private static func dynamic(light: (Double, Double, Double), dark: (Double, Double, Double)) -> NSColor {
        NSColor(name: nil) { appearance in
            nsColor(appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? dark : light)
        }
    }

    private static func nsColor(_ components: (Double, Double, Double)) -> NSColor {
        NSColor(srgbRed: components.0 / 255, green: components.1 / 255, blue: components.2 / 255, alpha: 1)
    }

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