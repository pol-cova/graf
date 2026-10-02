import Foundation

/// The visual decisions of the editor's styling pass, kept apart from the
/// AppKit code that applies them.
///
/// `restyle` used to live in `EditorCoordinator` as a switch over scanner
/// tokens that picked a font and a color per token kind. That switch is a
/// design decision — it says what a command should look like, that a
/// reference is the one accent, that math is italic prose — and it was
/// untestable, because it could only run inside an AppKit text view holding
/// a live document.
///
/// Splitting the decision from the application makes it assertable over a
/// plain string, which is what the tests in `MarkupStylerTests` do. What
/// follows is the cost: a plain value per styled token, instead of an
/// `NSTextStorage` mutated in place.
///
/// These are `Sendable`. `NSFont` is a class and is *not* `Sendable`, so no
/// font appears in these values; the coordinator resolves one from `Theme`
/// when it applies them.
public enum MarkupStyler {
    /// What one styled span looks like.
    ///
    /// There is no `prose` case: prose is what a span has *before* any token
    /// is applied, and `EditorCoordinator.restyle` lays it down across the
    /// whole range first. A token either differs from prose and appears here,
    /// or it does not and the scanner should not have emitted it. An earlier
    /// revision carried a `prose` case plus a `distinctTokens` filter to drop
    /// it; `style(for:)` never produced one, so both were unreachable.
    public enum Style: Equatable, Sendable {
        /// A command name, braces, and other markup that steps back.
        case markup
        /// The argument of a cross-reference: link blue, the only accent.
        case reference
        /// The body of inline math: prose, italic.
        case math
        case comment
        case heading(level: Int)
    }

    /// One token and the style it resolves to. `MarkupScanner` already
    /// decided *what* is markup; this decides how it looks.
    public struct StyledToken: Equatable, Sendable {
        public let range: NSRange
        public let style: Style

        public init(range: NSRange, style: Style) {
            self.range = range
            self.style = style
        }
    }

    /// Resolves the style for one scanner token.
    ///
    /// This is the mapping that used to be the body of `restyle`.
    public static func style(for token: MarkupToken) -> StyledToken {
        switch token {
        case let .command(range), let .delimiter(range):
            StyledToken(range: range, style: .markup)
        case let .reference(range):
            StyledToken(range: range, style: .reference)
        case let .math(range):
            StyledToken(range: range, style: .math)
        case let .comment(range):
            StyledToken(range: range, style: .comment)
        case let .heading(range, level):
            StyledToken(range: range, style: .heading(level: level))
        }
    }

    /// Scans `text` over `range` and resolves every token, in document order.
    public static func styledTokens(
        in text: NSString,
        syntax: Syntax,
        over range: NSRange
    ) -> [StyledToken] {
        MarkupScanner.scan(text, in: range, syntax: syntax).map(style(for:))
    }
}