import Foundation

/// Paragraph boundaries as a writer sees them: text between blank lines.
/// Focus mode dims everything outside the paragraph that holds the caret.
public enum Paragraphs {
    /// The blank-line-delimited paragraph containing `location`, without the
    /// surrounding blank lines. A caret on a blank line yields an empty range
    /// at that location.
    public static func range(in text: NSString, containing location: Int) -> NSRange {
        let length = text.length
        guard length > 0 else { return NSRange(location: 0, length: 0) }
        let location = min(max(location, 0), length)

        // A caret at the very end sits on the last line, unless the text ends
        // with a newline, which puts it on a new empty line.
        if location == length && isNewline(text.character(at: length - 1)) {
            return NSRange(location: location, length: 0)
        }
        let probe = min(location, length - 1)
        let line = text.lineRange(for: NSRange(location: probe, length: 0))
        if isBlank(text, line) {
            return NSRange(location: location, length: 0)
        }

        var start = line.location
        while start > 0 {
            let previous = text.lineRange(for: NSRange(location: start - 1, length: 0))
            if isBlank(text, previous) { break }
            start = previous.location
        }

        var end = NSMaxRange(line)
        while end < length {
            let next = text.lineRange(for: NSRange(location: end, length: 0))
            if isBlank(text, next) { break }
            end = NSMaxRange(next)
        }
        // Leave the final newline outside the paragraph.
        while end > start, isNewline(text.character(at: end - 1)) { end -= 1 }
        return NSRange(location: start, length: end - start)
    }

    /// Expands `range` to whole paragraphs, for restyling after an edit.
    /// Uses line boundaries on both sides so styling never splits a token
    /// that spans a line.
    public static func enclosingLines(in text: NSString, of range: NSRange) -> NSRange {
        guard text.length > 0 else { return NSRange(location: 0, length: 0) }
        let clamped = NSIntersectionRange(range, NSRange(location: 0, length: text.length))
        let probe = clamped.length == 0 && clamped.location == text.length
            ? NSRange(location: max(text.length - 1, 0), length: 0)
            : clamped
        return text.paragraphRange(for: probe)
    }

    private static func isBlank(_ text: NSString, _ line: NSRange) -> Bool {
        text.substring(with: line).allSatisfy(\.isWhitespace)
    }

    private static func isNewline(_ character: unichar) -> Bool {
        character == 0x0A || character == 0x0D
    }
}
