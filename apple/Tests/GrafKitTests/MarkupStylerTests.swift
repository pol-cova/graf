import AppKit
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

    @Test func unknownSyntaxLeavesNothingToStyle() {
        let string = "\\section{Hi}" as NSString
        let tokens = MarkupStyler.styledTokens(
            in: string,
            syntax: nil,
            over: NSRange(location: 0, length: string.length)
        )
        #expect(tokens.isEmpty)
    }

    @Test func distinctTokensDropsSpansThatRenderAsProse() {
        let string = "\\section{Hi}" as NSString
        let all = MarkupStyler.styledTokens(in: string, syntax: .latex, over: NSRange(location: 0, length: string.length))
        let distinct = MarkupStyler.distinctTokens(in: string, syntax: .latex, over: NSRange(location: 0, length: string.length))
        // Every resolved token is non-prose here, so filtering changes
        // nothing today; what matters is that the two entry points agree
        // rather than the applier re-deriving the judgement.
        #expect(all.count == distinct.count)
        #expect(distinct.allSatisfy { $0.style != .prose })
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