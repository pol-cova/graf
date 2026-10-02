import AppKit
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
    /// What one styled span looks like. Prose is the default, so it is not
    /// spelled out here.
    public enum Style: Equatable, Sendable {
        case prose
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
    /// This is the mapping that used to be the body of `restyle`. Note that
    /// a token with no visual difference from prose — `delimiter` inside an
    /// otherwise-prose run — still resolves, so the caller can apply it
    /// unconditionally; collapsing no-op spans here would mean the applier
    /// had to re-derive the same judgement.
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
    ///
    /// `syntax` being `nil` — a file Graf does not compile — leaves the
    /// whole span as prose, which is what the caller applies first.
    public static func styledTokens(
        in text: NSString,
        syntax: Syntax?,
        over range: NSRange
    ) -> [StyledToken] {
        guard let syntax else { return [] }
        return MarkupScanner.scan(text, in: range, syntax: syntax).map(style(for:))
    }

    /// Spans whose style differs from prose. Prose itself is applied to the
    /// whole range before these, so a token that would render identically
    /// to prose is dropped here rather than needlessly re-styled.
    public static func distinctTokens(
        in text: NSString,
        syntax: Syntax?,
        over range: NSRange
    ) -> [StyledToken] {
        styledTokens(in: text, syntax: syntax, over: range).filter { $0.style != .prose }
    }
}