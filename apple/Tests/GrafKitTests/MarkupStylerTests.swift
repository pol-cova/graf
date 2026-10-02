import Foundation
import Testing
@testable import GrafKit

/// The styling decisions, asserted over plain source. Before this split the
/// same mapping was a `switch` inside a method that could only run against
/// a live `NSTextStorage` inside a window.
@Suite struct MarkupStylerTests {
    private func styles(_ text: String, _ syntax: Syntax = .latex) -> [(String, MarkupStyler.Style)] {
        let string = text as NSString
        return MarkupStyler.styledTokens(in: string, syntax: syntax, over: NSRange(location: 0, length: string.length))
            .map { (string.substring(with: $0.range), $0.style) }
    }

    @Test func commandAndDelimiterBothStepBack() {
        let found = styles("\\section{Hi}")
        #expect(found.contains { $0.0 == "\\section" && $0.1 == .markup })
        #expect(found.contains { $0.0 == "{" && $0.1 == .markup })
    }

    @Test func referenceIsTheOnlyAccent() {
        let found = styles("see \\ref{fig:a}.")
        #expect(found.contains { $0.0 == "fig:a" && $0.1 == .reference })
        // The command itself is not an accent, only its argument.
        #expect(found.contains { $0.0 == "\\ref" && $0.1 == .markup })
    }

    @Test func mathIsItalicsProseNotMarkup() {
        let found = styles("value $x^2$ here")
        #expect(found.contains { $0.0 == "x^2" && $0.1 == .math })
        // Delimiters still dim; the body does not.
        #expect(found.contains { $0.0 == "$" && $0.1 == .markup })
    }

    @Test func commentCarriesItsOwnStyle() {
        let found = styles("visible % hidden")
        #expect(found.contains { $0.0 == "% hidden" && $0.1 == .comment })
    }

    @Test func headingCarriesItsLevel() {
        let found = styles("\\subsection*{Recall}")
        #expect(found.contains { $0.0 == "Recall" && $0.1 == .heading(level: 2) })
    }

    @Test func typstTokensResolveToo() {
        let found = styles("#set page()", .typst)
        #expect(found.contains { $0.0 == "#set" && $0.1 == .markup })
    }

    @Test func everyTokenIsAStyleTheApplierCanApply() {
    // Prose is what a span has *before* any token is applied, so every style
    // the styler returns is a departure from it. The applier's switch is
    // exhaustive over `Style` with no `.prose` branch, so this is the list of
    // what it must handle: if a case is ever added here, the applier stops
    // compiling until it deals with it.
    let string = "\\section{Hi}\n\\cite{key} $x$\n% note" as NSString
    let tokens = MarkupStyler.styledTokens(
        in: string,
        syntax: .latex,
        over: NSRange(location: 0, length: string.length)
    )
    let styles: [MarkupStyler.Style] = tokens.map(\.style)
    #expect(!styles.isEmpty)
    // Each kind that appears is one the applier handles; markup covers both
    // command and delimiter, and heading carries its level.
    #expect(styles.contains(.reference))
    #expect(styles.contains(.math))
    #expect(styles.contains(.comment))
    #expect(styles.contains(.markup))
    #expect(styles.contains(.heading(level: 1)))
}

    @Test func aCommentDoesNotExtendItsHeadingToEndOfLine() {
        // Pins real scanner behaviour rather than the behaviour I assumed:
        // inside a heading, the comment is absorbed and the heading token
        // spans to end-of-line. The styler must pass that through unchanged
        // rather than second-guess the scanner.
        let string = "\\section{Hi % note}" as NSString
        let tokens = MarkupStyler.styledTokens(in: string, syntax: .latex, over: NSRange(location: 0, length: string.length))
        let found = tokens.map { (string.substring(with: $0.range), $0.style) }

        #expect(found.contains { $0.0 == "\\section" && $0.1 == .markup })
        #expect(found.contains { $0.1 == .heading(level: 1) })
        #expect(found.contains { $0.0 == "}" && $0.1 == .markup })
        #expect(tokens.count == 4)
    }

    @Test func commentsOutsideAHeadingAreTheirOwnStyle() {
        let found = styles("text % hidden")
        #expect(found.contains { $0.0 == "% hidden" && $0.1 == .comment })
    }
}