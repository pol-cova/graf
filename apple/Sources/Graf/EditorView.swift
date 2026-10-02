import AppKit
import GrafCore
import GrafKit
import SwiftUI

/// The writing surface: a TextKit 2 text view in a centered column.
struct EditorView: NSViewRepresentable {
    let workspace: Workspace

    func makeCoordinator() -> EditorCoordinator {
        EditorCoordinator(workspace: workspace)
    }

    func makeNSView(context: Context) -> NSScrollView {
        let textView = WritingTextView(usingTextLayoutManager: true)
        context.coordinator.attach(to: textView)

        let scrollView = NSScrollView()
        scrollView.documentView = textView
        scrollView.hasVerticalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.backgroundColor = Theme.background
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        context.coordinator.update(
            focusMode: workspace.focusMode,
            diagnostics: workspace.diagnosticsForCurrentFile,
            fileURL: workspace.fileURL,
            jump: workspace.jumpRequest,
            proseSize: AppSettings.shared.proseSize
        )
    }
}

/// A text view that keeps its text in a readable column centered in the
/// window, however wide the window gets, and finishes markup completions
/// with the text the core supplies.
final class WritingTextView: NSTextView {
    /// The core's completions for the current partial, by label.
    var pendingCompletions: [String: Completion] = [:]

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        let horizontal = max(32, (newSize.width - Theme.columnWidth) / 2)
        if abs(textContainerInset.width - horizontal) > 0.5 {
            textContainerInset = NSSize(width: horizontal, height: 72)
        }
    }

    /// The partial being completed: after `{` for keys, labels, and
    /// environments, or from `\` for commands. Colons count as part of a
    /// label (`fig:recall`), unlike the default word boundary.
    override var rangeForUserCompletion: NSRange {
        let text = string as NSString
        let caret = selectedRange().location
        var start = caret
        while start > 0 {
            let character = text.character(at: start - 1)
            if character == 0x7B || character == 0x20 || character == 0x0A || character == 0x7D { break }
            if character == 0x5C {
                start -= 1
                break
            }
            start -= 1
        }
        return NSRange(location: start, length: caret - start)
    }

    /// Browsing shows the label; the final choice inserts the core's text,
    /// which closes the brace or expands the environment.
    override func insertCompletion(_ word: String, forPartialWordRange charRange: NSRange, movement: Int, isFinal flag: Bool) {
        guard flag, let completion = pendingCompletions[word] else {
            super.insertCompletion(word, forPartialWordRange: charRange, movement: movement, isFinal: flag)
            return
        }
        let typed = (string as NSString).substring(with: charRange)
        super.insertCompletion(typed + completion.insertText, forPartialWordRange: charRange, movement: movement, isFinal: true)
        pendingCompletions = [:]
    }

    /// Opening a project leaves the caret ready to type, which also makes
    /// Find and completion work without a click first.
    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard let window else { return }
        DispatchQueue.main.async { window.makeFirstResponder(self) }
    }

    /// Tab inserts spaces, the width set in Settings.
    override func insertTab(_ sender: Any?) {
        let width = Int(AppSettings.shared.values.tabSize)
        insertText(String(repeating: " ", count: max(width, 1)), replacementRange: selectedRange())
    }
}

@MainActor
final class EditorCoordinator: NSObject, NSTextViewDelegate, NSTextStorageDelegate {
    private let workspace: Workspace
    private weak var textView: NSTextView?
    private var focusedParagraph: NSRange?
    private var focusMode = true
    private var shownFile: URL?
    /// Owns the error hints, so their subviews, underlines, and the frame
    /// observer are not this coordinator's problem.
    private var hints: DiagnosticHints?
    private var shownDiagnostics: [GrafCore.Diagnostic] = []
    /// Set while the coordinator itself replaces the text (opening a file),
    /// so that load is not mistaken for an edit.
    private var isLoading = false
    private var handledJump: UUID?
    private var proseSize = Theme.defaultProseSize

    init(workspace: Workspace) {
        self.workspace = workspace
    }

    func attach(to textView: NSTextView) {
        self.textView = textView
        textView.delegate = self
        textView.isRichText = false
        textView.importsGraphics = false
        textView.allowsUndo = true
        textView.usesFindBar = true
        textView.isIncrementalSearchingEnabled = true
        textView.drawsBackground = true
        textView.backgroundColor = Theme.background
        textView.insertionPointColor = Theme.link
        textView.textColor = Theme.ink
        proseSize = AppSettings.shared.proseSize
        textView.font = Theme.prose(size: proseSize)
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        textView.textContainer?.widthTracksTextView = true
        // Markup must be typed exactly as written.
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false
        textView.isAutomaticTextReplacementEnabled = false
        textView.smartInsertDeleteEnabled = false
        // Prose still gets the system's writing help.
        textView.isContinuousSpellCheckingEnabled = true
        textView.isGrammarCheckingEnabled = true

        if let contentStorage = textView.textLayoutManager?.textContentManager as? NSTextContentStorage {
            contentStorage.textStorage = workspace.textStorage
        }
        workspace.textStorage.delegate = self
        let hints = DiagnosticHints(textView: textView)
        hints.observeReflow()
        self.hints = hints
        loadCurrentFile()
    }

    func update(
        focusMode: Bool,
        diagnostics: [GrafCore.Diagnostic],
        fileURL: URL,
        jump: (line: Int, id: UUID)?,
        proseSize: CGFloat
    ) {
        if shownFile != fileURL {
            loadCurrentFile()
        }
        if proseSize != self.proseSize {
            self.proseSize = proseSize
            isLoading = true
            restyle(NSRange(location: 0, length: workspace.textStorage.length))
            isLoading = false
            if hints?.isVisible == true { showDiagnostics() }
        }
        if let jump, jump.id != handledJump {
            handledJump = jump.id
            moveCaret(toLine: jump.line)
        }
        if self.focusMode != focusMode {
            self.focusMode = focusMode
            focusedParagraph = nil
            applyFocus()
        }
        if diagnostics != shownDiagnostics {
            shownDiagnostics = diagnostics
            showDiagnostics()
        }
    }

    /// Styles the whole document after a file is opened and restores the caret.
    private func loadCurrentFile() {
        guard let textView else { return }
        shownFile = workspace.fileURL
        isLoading = true
        restyle(NSRange(location: 0, length: workspace.textStorage.length))
        isLoading = false
        textView.undoManager?.removeAllActions()
        let caret = min(workspace.caret, workspace.textStorage.length)
        textView.setSelectedRange(NSRange(location: caret, length: 0))
        textView.scrollRangeToVisible(NSRange(location: caret, length: 0))
        focusedParagraph = nil
        applyFocus()
        clearHints()
    }

    private func moveCaret(toLine line: Int) {
        guard let textView,
              let range = (workspace.textStorage.string as NSString).rangeOfLine(line)
        else { return }
        let caret = NSRange(location: range.location, length: 0)
        textView.window?.makeFirstResponder(textView)
        textView.setSelectedRange(caret)
        textView.scrollRangeToVisible(range)
        textView.showFindIndicator(for: range)
    }

    // MARK: NSTextStorageDelegate

    nonisolated func textStorage(
        _ textStorage: NSTextStorage,
        didProcessEditing editedMask: NSTextStorageEditActions,
        range editedRange: NSRange,
        changeInLength delta: Int
    ) {
        guard editedMask.contains(.editedCharacters) else { return }
        // The storage is the workspace's own, and AppKit edits it on the
        // main thread; read it from there instead of sending it across.
        MainActor.assumeIsolated {
            let text = workspace.textStorage.string as NSString
            let paragraphs = Paragraphs.enclosingLines(in: text, of: editedRange)
            restyle(paragraphs)
            guard !isLoading, !workspace.isLoadingText else { return }
            // Errors wait their turn: hide hints while the writer types.
            clearHints()
            workspace.didEdit()
            if delta > 0 { offerCompletionIfTriggered() }
        }
    }

    // MARK: Completion

    /// Opening a citation, reference, or environment brings up the list
    /// right away, since a key is what the writer needs next.
    private static let completionTriggers = [
        "\\cite{", "\\ref{", "\\eqref{", "\\autoref{", "\\pageref{", "\\begin{",
    ]

    /// Checks once the edit has settled and the caret sits after the typed
    /// text; the range reported while the storage processes an edit is not
    /// reliably the insertion point.
    private func offerCompletionIfTriggered() {
        guard workspace.syntax == .latex else { return }
        DispatchQueue.main.async { [weak self] in
            guard let self, let textView = self.textView else { return }
            let text = textView.string as NSString
            let caret = textView.selectedRange().location
            guard textView.selectedRange().length == 0, caret <= text.length else { return }
            let start = text.lineRange(for: NSRange(location: caret, length: 0)).location
            let before = text.substring(with: NSRange(location: start, length: caret - start))
            guard Self.completionTriggers.contains(where: before.hasSuffix) else { return }
            textView.complete(nil)
        }
    }

    func textView(
        _ textView: NSTextView,
        completions words: [String],
        forPartialWordRange charRange: NSRange,
        indexOfSelectedItem index: UnsafeMutablePointer<Int>?
    ) -> [String] {
        guard workspace.syntax == .latex else { return [] }
        let text = textView.string as NSString
        let caret = textView.selectedRange().location
        let lineStart = text.lineRange(for: NSRange(location: caret, length: 0)).location
        let before = text.substring(with: NSRange(location: lineStart, length: caret - lineStart))
        let completions = workspace.completer.complete(lineBeforeCaret: before)
        (textView as? WritingTextView)?.pendingCompletions = Dictionary(
            completions.map { ($0.label, $0) }, uniquingKeysWith: { first, _ in first }
        )
        index?.pointee = completions.isEmpty ? -1 : 0
        return completions.map(\.label)
    }

    // MARK: NSTextViewDelegate

    /// Spell checking is for prose. Words inside commands, references,
    /// math, and comments are never marked misspelled.
    func textView(_ textView: NSTextView, shouldSetSpellingState value: Int, range affectedCharRange: NSRange) -> Int {
        guard value != 0, let syntax = workspace.syntax else { return value }
        let text = workspace.textStorage.string as NSString
        let paragraph = Paragraphs.enclosingLines(in: text, of: affectedCharRange)
        let markup = MarkupScanner.markupRanges(in: text, around: paragraph, syntax: syntax)
        let isMarkup = markup.contains { NSIntersectionRange($0, affectedCharRange).length > 0 }
        return isMarkup ? 0 : value
    }

    func textViewDidChangeSelection(_ notification: Notification) {
        guard let textView else { return }
        workspace.caret = textView.selectedRange().location
        applyFocus()
    }

    // MARK: Styling

    /// Applies prose and markup attributes to `range`. Runs synchronously on
    /// the edited paragraphs only, so it keeps up with typing.
    ///
    /// Deciding *what* each span looks like is `MarkupStyler`'s job, in
    /// GrafKit, where it is unit tested. This method only translates those
    /// styles into `Theme` fonts and colors.
    private func restyle(_ range: NSRange) {
        let storage = workspace.textStorage
        let range = NSIntersectionRange(range, NSRange(location: 0, length: storage.length))
        guard range.length > 0 else { return }

        storage.beginEditing()
        defer { storage.endEditing() }

        guard let syntax = workspace.syntax else {
            storage.setAttributes([
                .font: Theme.markupFont(prose: proseSize, emphasis: true),
                .foregroundColor: Theme.ink,
                .paragraphStyle: Theme.plainParagraph,
            ], range: range)
            return
        }

        storage.setAttributes([
            .font: Theme.prose(size: proseSize),
            .foregroundColor: Theme.ink,
            .paragraphStyle: Theme.proseParagraph,
        ], range: range)

        let markup = Theme.markupFont(prose: proseSize)
        let text = storage.string as NSString
        for token in MarkupStyler.styledTokens(in: text, syntax: syntax, over: range) {
            let attributes: [NSAttributedString.Key: Any]
            switch token.style {
            case .markup:
                attributes = [.font: markup, .foregroundColor: Theme.markup]
            case .comment:
                attributes = [.font: markup, .foregroundColor: Theme.comment]
            case .reference:
                attributes = [
                    .font: Theme.markupFont(prose: proseSize, emphasis: true),
                    .foregroundColor: Theme.link,
                ]
            case .math:
                attributes = [.font: Theme.prose(size: proseSize, italic: true)]
            case let .heading(level):
                attributes = [.font: Theme.headingFont(level: level, prose: proseSize)]
            }
            storage.addAttributes(attributes, range: token.range)
        }
    }

    // MARK: Focus mode

    /// Dims everything outside the caret's paragraph. Uses TextKit 2
    /// rendering attributes, which change how text draws without touching
    /// the document's attributes or the undo stack.
    private func applyFocus() {
        guard let textView,
              let layoutManager = textView.textLayoutManager,
              let content = layoutManager.textContentManager as? NSTextContentStorage
        else { return }
        let text = workspace.textStorage.string as NSString
        let paragraph = focusMode
            ? Paragraphs.range(in: text, containing: textView.selectedRange().location)
            : NSRange(location: NSNotFound, length: 0)
        guard paragraph != focusedParagraph else { return }
        focusedParagraph = paragraph

        let whole = content.documentRange
        layoutManager.removeRenderingAttribute(.foregroundColor, for: whole)
        guard focusMode else { return }

        let before = NSRange(location: 0, length: paragraph.location)
        let after = NSRange(location: NSMaxRange(paragraph), length: text.length - NSMaxRange(paragraph))
        for range in [before, after] where range.length > 0 {
            if let textRange = content.textRange(for: range) {
                layoutManager.addRenderingAttribute(.foregroundColor, value: Theme.dimmed, for: textRange)
            }
        }
    }

    // MARK: Diagnostics

    /// Puts a quiet hint under each line that has an error, like frame 05 of
    /// the concept. Hints clear as soon as the writer types again.
    private func showDiagnostics() {
        hints?.show(shownDiagnostics, in: workspace.textStorage.string as NSString)
    }

    private func clearHints() {
        hints?.clear()
    }
}

/// Owns the error hints under broken lines.
///
/// This type exists so the coordinator does not have to. It holds the hint
/// subviews, the underline rendering attributes, and the frame-change
/// observer that repositions them; tearing them down is its own
/// responsibility rather than a `clearHints()` call the caller must
/// remember to make on every path that invalidates them.
@MainActor
final class DiagnosticHints {
    private weak var textView: NSTextView?
    private var views: [NSView] = []
    /// Ranges currently underlined, so clearing retracts them precisely
    /// rather than stripping attributes from the whole document.
    private var underlined: [NSTextRange] = []
    /// The diagnostics last drawn, kept so a reflow can redraw the same set
    /// without the coordinator re-supplying them.
    private var lastDiagnostics: [GrafCore.Diagnostic] = []
    /// Retained so `deinit` can unregister; see `observeReflow`.
    /// `nonisolated(unsafe)` because `deinit` on a `@MainActor` type is not
    /// isolated, and the token is only ever created on the main actor and
    /// read once during deallocation.
    nonisolated(unsafe) private var reflowObserver: (any NSObjectProtocol)?

    /// Whether any hint is showing. The frame observer skips work when
    /// nothing is visible, which is the common case.
    var isVisible: Bool { !views.isEmpty }

    init(textView: NSTextView) {
        self.textView = textView
    }

    /// Starts watching for reflow, so hints follow their line when the
    /// column width changes.
    ///
    /// The observer token is kept and removed in `deinit`. A block observer
    /// holding `[weak self]` cannot leak this object, but without the
    /// removal it stays registered for the process lifetime — one per
    /// coordinator ever created — and runs a main-actor hop on every frame
    /// change of a text view nobody is looking at.
    func observeReflow() {
        reflowObserver = NotificationCenter.default.addObserver(
            forName: NSView.frameDidChangeNotification,
            object: textView,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.repositionIfVisible() }
        }
    }

    deinit {
        if let reflowObserver {
            NotificationCenter.default.removeObserver(reflowObserver)
        }
    }

    /// Removes every hint and underline. Safe to call when nothing is shown.
    func clear() {
        views.forEach { $0.removeFromSuperview() }
        views.removeAll()
        guard let layoutManager = textView?.textLayoutManager
        else { underlined = []; return }
        for range in underlined {
            layoutManager.removeRenderingAttribute(.underlineStyle, for: range)
            layoutManager.removeRenderingAttribute(.underlineColor, for: range)
        }
        underlined = []
    }

    /// Draws one hint per error diagnostic, at the line that caused it.
    func show(_ diagnostics: [GrafCore.Diagnostic], in text: NSString) {
        lastDiagnostics = diagnostics
        clear()
        guard let textView,
              let layoutManager = textView.textLayoutManager,
              let content = layoutManager.textContentManager as? NSTextContentStorage
        else { return }

        for diagnostic in diagnostics where diagnostic.severity == .error {
            guard let line = diagnostic.line, line > 0,
                  let lineRange = text.rangeOfLine(Int(line)),
                  let textRange = content.textRange(for: lineRange)
            else { continue }
            // TextKit 2 lays out lazily; measure real geometry, not an estimate.
            layoutManager.ensureLayout(for: textRange)
            guard let fragment = layoutManager.textLayoutFragment(for: textRange.location) else { continue }

            let underlineRange = text.trimmedRange(lineRange)
            if underlineRange.length > 0, let underline = content.textRange(for: underlineRange) {
                layoutManager.addRenderingAttribute(
                    .underlineStyle,
                    value: NSUnderlineStyle.single.union(.patternDot).rawValue,
                    for: underline
                )
                layoutManager.addRenderingAttribute(.underlineColor, value: Theme.error, for: underline)
                underlined.append(underline)
            }

            let hint = NSHostingView(rootView: DiagnosticHint(message: diagnostic.message))
            hint.setFrameSize(hint.fittingSize)
            hint.setFrameOrigin(origin(size: hint.frame.size, fragment: fragment, in: textView))
            textView.addSubview(hint)
            views.append(hint)
        }
    }

    /// Redraws in place after a reflow. Skips the work entirely when nothing is
    /// showing, which is the common case.
    func repositionIfVisible() {
        guard isVisible else { return }
        show(lastDiagnostics, in: (textView?.string ?? "") as NSString)
    }

    /// Places a hint where it never covers text: after the end of the
    /// broken line, else in the right margin, else under the line.
    private func origin(size: NSSize, fragment: NSTextLayoutFragment, in textView: NSTextView) -> NSPoint {
        // Layout fragment frames are in text-container coordinates.
        let origin = textView.textContainerOrigin
        let frame = fragment.layoutFragmentFrame
        let lastLine = fragment.textLineFragments.last
        let lineTop = origin.y + frame.minY + (lastLine?.typographicBounds.minY ?? 0)
        let lineHeight = lastLine?.typographicBounds.height ?? frame.height
        let centeredY = lineTop + (lineHeight - size.height) / 2
        let textEnd = origin.x + frame.minX + (lastLine?.typographicBounds.maxX ?? frame.width)
        let columnEnd = origin.x + (textView.textContainer?.size.width ?? frame.width)
            - (textView.textContainer?.lineFragmentPadding ?? 0)

        if textEnd + 16 + size.width <= columnEnd {
            return NSPoint(x: textEnd + 16, y: centeredY)
        }
        if columnEnd + 12 + size.width <= textView.bounds.width - 8 {
            return NSPoint(x: columnEnd + 12, y: centeredY)
        }
        return NSPoint(x: origin.x + frame.minX, y: origin.y + frame.maxY + 2)
    }
}

/// One-line hint shown under a broken line.
struct DiagnosticHint: View {
    let message: String

    var body: some View {
        HStack(spacing: 8) {
            Circle().fill(Color.grafError).frame(width: 6, height: 6)
            Text(message)
                .font(Theme.Chrome.calloutUI)
                .foregroundStyle(.primary)
                .lineLimit(1)
        }
        .padding(.vertical, 3)
        .padding(.horizontal, 8)
        .background(.background, in: RoundedRectangle(cornerRadius: 5))
        .overlay(RoundedRectangle(cornerRadius: 5).strokeBorder(Color.grafError.opacity(0.25)))
        .accessibilityLabel("Error: \(message)")
    }
}

extension NSTextContentStorage {
    /// Converts a UTF-16 range in the backing string to a TextKit 2 range.
    func textRange(for range: NSRange) -> NSTextRange? {
        guard let start = location(documentRange.location, offsetBy: range.location),
              let end = location(start, offsetBy: range.length)
        else { return nil }
        return NSTextRange(location: start, end: end)
    }
}

extension NSString {
    /// The range of the one-based `line`, without its newline.
    func rangeOfLine(_ line: Int) -> NSRange? {
        var current = 1
        var location = 0
        while current < line {
            let found = range(of: "\n", options: [], range: NSRange(location: location, length: length - location))
            guard found.location != NSNotFound else { return nil }
            location = found.location + 1
            current += 1
        }
        guard location <= length else { return nil }
        var contentsEnd = 0
        getLineStart(nil, end: nil, contentsEnd: &contentsEnd, for: NSRange(location: location, length: 0))
        return NSRange(location: location, length: contentsEnd - location)
    }

    /// `range` without leading and trailing whitespace.
    func trimmedRange(_ range: NSRange) -> NSRange {
        var start = range.location
        var end = NSMaxRange(range)
        let whitespace = CharacterSet.whitespaces
        while start < end, let scalar = UnicodeScalar(character(at: start)), whitespace.contains(scalar) { start += 1 }
        while end > start, let scalar = UnicodeScalar(character(at: end - 1)), whitespace.contains(scalar) { end -= 1 }
        return NSRange(location: start, length: end - start)
    }
}
