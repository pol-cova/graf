import Foundation
import Testing
@testable import GrafKit

@Suite struct MarkupScannerTests {
    private func substrings(_ text: String, _ tokens: [MarkupToken], where match: (MarkupToken) -> NSRange?) -> [String] {
        let string = text as NSString
        return tokens.compactMap(match).map { string.substring(with: $0) }
    }

    @Test func latexCitationArgumentIsAReference() {
        let text = "as shown by \\cite{rayner2016}."
        let tokens = MarkupScanner.scan(text, syntax: .latex)
        let references = substrings(text, tokens) { if case let .reference(range) = $0 { range } else { nil } }
        let commands = substrings(text, tokens) { if case let .command(range) = $0 { range } else { nil } }
        #expect(references == ["rayner2016"])
        #expect(commands == ["\\cite"])
    }

    @Test func optionalArgumentsAreSkippedBeforeTheReference() {
        let text = "\\citep[p.~4]{knuth1984}"
        let tokens = MarkupScanner.scan(text, syntax: .latex)
        let references = substrings(text, tokens) { if case let .reference(range) = $0 { range } else { nil } }
        #expect(references == ["knuth1984"])
    }

    @Test func sectionTitleIsAHeadingWithItsLevel() {
        let text = "\\subsection*{Recall task}"
        let tokens = MarkupScanner.scan(text, syntax: .latex)
        let headings = tokens.compactMap { token -> (String, Int)? in
            if case let .heading(range, level) = token { ((text as NSString).substring(with: range), level) } else { nil }
        }
        #expect(headings.map(\.0) == ["Recall task"])
        #expect(headings.map(\.1) == [2])
    }

    @Test func nestedBracesStayInsideTheHeading() {
        let text = "\\section{The \\emph{real} method}"
        let tokens = MarkupScanner.scan(text, syntax: .latex)
        let headings = substrings(text, tokens) { if case let .heading(range, _) = $0 { range } else { nil } }
        #expect(headings == ["The \\emph{real} method"])
    }

    @Test func structuralArgumentsAreMarkupNotProse() {
        let text = "\\documentclass[11pt]{article}\n\\begin{document}"
        let tokens = MarkupScanner.scan(text, syntax: .latex)
        let commands = substrings(text, tokens) { if case let .command(range) = $0 { range } else { nil } }
        #expect(commands == ["\\documentclass", "article", "\\begin", "document"])
    }

    @Test func markupRangesCoverCommandsButNotProse() {
        let text = "Plain \\emph{words} and \\cite{key}."
        let markup = MarkupScanner.markupRanges(in: text as NSString, syntax: .latex)
            .filter { $0.length > 1 }
        let covered = markup.map { (text as NSString).substring(with: $0) }
        #expect(covered.contains("\\emph"))
        #expect(covered.contains("key"))
        #expect(!covered.contains { $0.contains("Plain") || $0.contains("words") })
    }

    @Test func inlineMathBodyIsSeparatedFromItsDelimiters() {
        let text = "gives $A = f / F$ for each"
        let tokens = MarkupScanner.scan(text, syntax: .latex)
        let math = substrings(text, tokens) { if case let .math(range) = $0 { range } else { nil } }
        #expect(math == ["A = f / F"])
    }

    @Test func unclosedMathOnlyMarksTheDollar() {
        let text = "typing $x_i and more prose"
        let tokens = MarkupScanner.scan(text, syntax: .latex)
        #expect(tokens == [.delimiter(NSRange(location: 7, length: 1))])
    }

    @Test func escapedPercentIsNotAComment() {
        let text = "50\\% of readers % a real comment"
        let tokens = MarkupScanner.scan(text, syntax: .latex)
        let comments = substrings(text, tokens) { if case let .comment(range) = $0 { range } else { nil } }
        #expect(comments == ["% a real comment"])
    }

    @Test func scanningASubrangeUsesAbsoluteOffsets() {
        let text = "Intro.\n\nSee \\ref{fig:a}." as NSString
        let paragraph = NSRange(location: 8, length: text.length - 8)
        let tokens = MarkupScanner.scan(text, in: paragraph, syntax: .latex)
        let references = tokens.compactMap { if case let .reference(range) = $0 { text.substring(with: range) } else { nil } }
        #expect(references == ["fig:a"])
    }

    @Test func typstHeadingsReferencesAndFunctions() {
        let text = "== Method\n#set page(width: 10cm)\nSee @fig:recall."
        let tokens = MarkupScanner.scan(text, syntax: .typst)
        let headings = tokens.compactMap { token -> Int? in
            if case let .heading(_, level) = token { level } else { nil }
        }
        let references = substrings(text, tokens) { if case let .reference(range) = $0 { range } else { nil } }
        let commands = substrings(text, tokens) { if case let .command(range) = $0 { range } else { nil } }
        #expect(headings == [2])
        #expect(references == ["@fig:recall"])
        #expect(commands == ["#set"])
    }

    @Test func typstLabelsAreReferencesButComparisonsAreNot() {
        let text = "== Recall <recall>\n#if x < 3 [small]"
        let tokens = MarkupScanner.scan(text, syntax: .typst)
        let references = substrings(text, tokens) { if case let .reference(range) = $0 { range } else { nil } }
        #expect(references == ["recall"])
    }

    @Test func emailAddressesAreNotTypstReferences() {
        let text = "Write to @ or email me."
        let tokens = MarkupScanner.scan(text, syntax: .typst)
        #expect(tokens.isEmpty)
    }

    }
