import Foundation

/// Source language of the document being edited.
public enum Syntax: Sendable {
    case latex
    case typst

    /// Chooses the syntax from a file name, defaulting to LaTeX.
    public init(fileName: String) {
        self = fileName.lowercased().hasSuffix(".typ") ? .typst : .latex
    }
}

/// A span of markup the editor styles differently from prose. Ranges are
/// UTF-16 offsets into the scanned string, so they apply directly to
/// `NSTextStorage`.
public enum MarkupToken: Equatable, Sendable {
    /// A command name such as `\section` or `#set`. Shown small and grey.
    case command(NSRange)
    /// Structural punctuation: braces, brackets, `$`, Typst `=` markers.
    case delimiter(NSRange)
    /// The argument of a cross-reference: `\cite{key}`, `\ref{fig}`, `@fig`.
    /// Shown in link blue, the only accent in the editor.
    case reference(NSRange)
    /// The body of inline math between `$` delimiters.
    case math(NSRange)
    /// A comment to the end of the line.
    case comment(NSRange)
    /// The title text of a section heading.
    case heading(NSRange, level: Int)
}

/// Scans LaTeX or Typst source for markup. It is a lexer, not a parser: it
/// only needs to be right about what to dim, and it must be fast enough to
/// run on every edited paragraph synchronously.
public enum MarkupScanner {
    public static func scan(_ text: NSString, in range: NSRange, syntax: Syntax) -> [MarkupToken] {
        var scanner = Cursor(text: text, range: range)
        switch syntax {
        case .latex: return scanner.scanLatex()
        case .typst: return scanner.scanTypst()
        }
    }

    /// Ranges that are markup rather than prose: commands, delimiters,
    /// references, math, and comments. Spell checking skips these.
    public static func markupRanges(in text: NSString, around range: NSRange? = nil, syntax: Syntax) -> [NSRange] {
        let scope = range ?? NSRange(location: 0, length: text.length)
        return scan(text, in: scope, syntax: syntax).compactMap { token in
            switch token {
            case let .command(range), let .delimiter(range), let .reference(range),
                 let .math(range), let .comment(range):
                range
            case .heading:
                nil
            }
        }
    }

    public static func scan(_ text: String, syntax: Syntax) -> [MarkupToken] {
        let string = text as NSString
        return scan(string, in: NSRange(location: 0, length: string.length), syntax: syntax)
    }
}

private let referenceCommands: Set<String> = [
    "cite", "citep", "citet", "citeauthor", "citeyear", "parencite", "textcite", "autocite",
    "ref", "eqref", "autoref", "cref", "Cref", "pageref", "label",
    "input", "include", "includegraphics", "bibliography",
]

/// Commands whose argument is structure, not prose: it steps back with the
/// rest of the markup.
private let structuralCommands: Set<String> = [
    "documentclass", "usepackage", "RequirePackage", "begin", "end",
    "newcommand", "renewcommand", "bibliographystyle", "setlength", "pagestyle",
]

private let headingLevels: [String: Int] = [
    "title": 0, "part": 0, "chapter": 0,
    "section": 1, "subsection": 2, "subsubsection": 3, "paragraph": 4,
]

private struct Cursor {
    let text: NSString
    let end: Int
    var position: Int
    var tokens: [MarkupToken] = []

    init(text: NSString, range: NSRange) {
        self.text = text
        self.position = range.location
        self.end = min(range.location + range.length, text.length)
    }

    func peek(_ offset: Int = 0) -> unichar? {
        let index = position + offset
        return index < end ? text.character(at: index) : nil
    }

    // MARK: LaTeX

    mutating func scanLatex() -> [MarkupToken] {
        while let character = peek() {
            switch character {
            case .backslash: scanLatexCommand()
            case .percent: scanComment(prefixLength: 1)
            case .dollar: scanMath()
            case .openBrace, .closeBrace, .openBracket, .closeBracket:
                tokens.append(.delimiter(NSRange(location: position, length: 1)))
                position += 1
            default: position += 1
            }
        }
        return tokens
    }

    mutating func scanLatexCommand() {
        let start = position
        position += 1
        guard let next = peek() else {
            tokens.append(.command(NSRange(location: start, length: 1)))
            return
        }
        guard next.isLetter else {
            // Control symbol such as `\\`, `\%`, or `\$`.
            position += 1
            tokens.append(.command(NSRange(location: start, length: 2)))
            return
        }
        while let character = peek(), character.isLetter { position += 1 }
        if peek() == .star { position += 1 }
        let commandRange = NSRange(location: start, length: position - start)
        tokens.append(.command(commandRange))

        let name = text.substring(with: NSRange(location: start + 1, length: position - start - 1))
            .trimmingCharacters(in: CharacterSet(charactersIn: "*"))
        let isReference = referenceCommands.contains(name)
        let isStructural = structuralCommands.contains(name)
        let headingLevel = headingLevels[name]
        guard isReference || isStructural || headingLevel != nil else { return }

        skipOptionalArgument()
        guard let argument = scanBracedArgument() else { return }
        if isStructural {
            tokens.append(.command(argument))
        } else if isReference {
            tokens.append(.reference(argument))
        } else if let headingLevel {
            tokens.append(.heading(argument, level: headingLevel))
        }
    }

    /// Skips `[...]`, marking the brackets as delimiters.
    mutating func skipOptionalArgument() {
        guard peek() == .openBracket else { return }
        tokens.append(.delimiter(NSRange(location: position, length: 1)))
        position += 1
        while let character = peek(), character != .closeBracket { position += 1 }
        if peek() == .closeBracket {
            tokens.append(.delimiter(NSRange(location: position, length: 1)))
            position += 1
        }
    }

    /// Reads `{...}` with balanced nesting and returns the inner range. The
    /// braces become delimiter tokens. Returns nil if no brace follows.
    mutating func scanBracedArgument() -> NSRange? {
        guard peek() == .openBrace else { return nil }
        tokens.append(.delimiter(NSRange(location: position, length: 1)))
        position += 1
        let start = position
        var depth = 1
        while let character = peek() {
            if character == .backslash {
                position += 2
                continue
            }
            if character == .openBrace { depth += 1 }
            if character == .closeBrace {
                depth -= 1
                if depth == 0 { break }
            }
            position += 1
        }
        position = min(position, end)
        let inner = NSRange(location: start, length: position - start)
        if peek() == .closeBrace {
            tokens.append(.delimiter(NSRange(location: position, length: 1)))
            position += 1
        }
        return inner
    }

    // MARK: Typst

    mutating func scanTypst() -> [MarkupToken] {
        var atLineStart = position == 0 || text.character(at: position - 1) == .newline
        while let character = peek() {
            if atLineStart, character == .equals {
                scanTypstHeading()
                atLineStart = false
                continue
            }
            switch character {
            case .slash where peek(1) == .slash: scanComment(prefixLength: 2)
            case .hash: scanTypstCommand()
            case .at: scanTypstReference()
            case .lessThan: scanTypstLabel()
            case .dollar: scanMath()
            case .openBrace, .closeBrace, .openBracket, .closeBracket:
                tokens.append(.delimiter(NSRange(location: position, length: 1)))
                position += 1
            default: position += 1
            }
            atLineStart = character == .newline
        }
        return tokens
    }

    mutating func scanTypstHeading() {
        let start = position
        while peek() == .equals { position += 1 }
        let level = position - start
        guard peek() == .space else { return }
        tokens.append(.delimiter(NSRange(location: start, length: level)))
        position += 1
        let titleStart = position
        var labelStart: Int?
        while let character = peek(), character != .newline {
            if character == .lessThan { labelStart = position }
            position += 1
        }
        let lineEnd = position
        // A trailing `<label>` belongs to the heading but is not its title.
        var titleEnd = lineEnd
        if let labelStart, text.character(at: lineEnd - 1) == .greaterThan {
            titleEnd = labelStart
            while titleEnd > titleStart, text.character(at: titleEnd - 1) == .space { titleEnd -= 1 }
            position = labelStart
            scanTypstLabel()
            position = lineEnd
        }
        tokens.append(.heading(NSRange(location: titleStart, length: titleEnd - titleStart), level: level))
    }

    mutating func scanTypstCommand() {
        let start = position
        position += 1
        while let character = peek(), character.isIdentifier { position += 1 }
        tokens.append(.command(NSRange(location: start, length: position - start)))
    }

    mutating func scanTypstReference() {
        let start = position
        position += 1
        while let character = peek(), character.isIdentifier || character == .colon { position += 1 }
        // A sentence-ending period is punctuation, not part of the label.
        while position - start > 1, text.character(at: position - 1) == .period { position -= 1 }
        guard position - start > 1 else { return }
        tokens.append(.reference(NSRange(location: start, length: position - start)))
    }

    /// A label definition such as `<fig:recall>`. Anything else starting with
    /// `<` (a comparison in code, say) is left alone.
    mutating func scanTypstLabel() {
        let start = position
        var cursor = position + 1
        while cursor < end, text.character(at: cursor).isIdentifier || text.character(at: cursor) == .colon {
            cursor += 1
        }
        guard cursor < end, cursor > start + 1, text.character(at: cursor) == .greaterThan else {
            position += 1
            return
        }
        tokens.append(.delimiter(NSRange(location: start, length: 1)))
        tokens.append(.reference(NSRange(location: start + 1, length: cursor - start - 1)))
        tokens.append(.delimiter(NSRange(location: cursor, length: 1)))
        position = cursor + 1
    }

    // MARK: Shared

    mutating func scanComment(prefixLength: Int) {
        let start = position
        position += prefixLength
        while let character = peek(), character != .newline { position += 1 }
        tokens.append(.comment(NSRange(location: start, length: position - start)))
    }

    /// Inline math between single `$` delimiters. An unclosed `$` only marks
    /// itself, so half-typed math never swallows the rest of the paragraph.
    mutating func scanMath() {
        let open = position
        position += 1
        let bodyStart = position
        while let character = peek(), character != .dollar {
            if character == .backslash { position += 1 }
            position += 1
        }
        guard peek() == .dollar else {
            position = bodyStart
            tokens.append(.delimiter(NSRange(location: open, length: 1)))
            return
        }
        tokens.append(.delimiter(NSRange(location: open, length: 1)))
        tokens.append(.math(NSRange(location: bodyStart, length: position - bodyStart)))
        tokens.append(.delimiter(NSRange(location: position, length: 1)))
        position += 1
    }
}

private extension unichar {
    static let backslash = unichar(UInt8(ascii: "\\"))
    static let percent = unichar(UInt8(ascii: "%"))
    static let dollar = unichar(UInt8(ascii: "$"))
    static let openBrace = unichar(UInt8(ascii: "{"))
    static let closeBrace = unichar(UInt8(ascii: "}"))
    static let openBracket = unichar(UInt8(ascii: "["))
    static let closeBracket = unichar(UInt8(ascii: "]"))
    static let star = unichar(UInt8(ascii: "*"))
    static let newline = unichar(UInt8(ascii: "\n"))
    static let equals = unichar(UInt8(ascii: "="))
    static let space = unichar(UInt8(ascii: " "))
    static let slash = unichar(UInt8(ascii: "/"))
    static let hash = unichar(UInt8(ascii: "#"))
    static let at = unichar(UInt8(ascii: "@"))
    static let colon = unichar(UInt8(ascii: ":"))
    static let period = unichar(UInt8(ascii: "."))
    static let lessThan = unichar(UInt8(ascii: "<"))
    static let greaterThan = unichar(UInt8(ascii: ">"))

    var isLetter: Bool {
        (0x41...0x5A).contains(self) || (0x61...0x7A).contains(self)
    }

    var isIdentifier: Bool {
        isLetter || (0x30...0x39).contains(self) || self == 0x2D || self == 0x5F || self == 0x2E
    }
}
