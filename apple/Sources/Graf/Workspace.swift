import AppKit
import GrafCore
import GrafKit
import Observation
import PDFKit

/// What the status dot in the toolbar says.
enum BuildStatus: Equatable {
    case idle
    case edited
    case compiling
    case built(milliseconds: UInt64)
    case failed(errors: Int, message: String)

    var label: String {
        switch self {
        case .idle: "Ready"
        case .edited: "Edited"
        case .compiling: "Compiling…"
        case let .built(milliseconds): "Compiled · \(Self.format(milliseconds))"
        case let .failed(errors, message):
            errors == 0 ? message : errors == 1 ? "1 error" : "\(errors) errors"
        }
    }

    private static func format(_ milliseconds: UInt64) -> String {
        milliseconds < 1000
            ? "\(milliseconds)ms"
            : String(format: "%.1fs", Double(milliseconds) / 1000)
    }
}

/// One open project: the file being written, its build, and its preview.
///
/// Swift owns the live text in `textStorage`. The Rust core only sees a
/// snapshot when the writer pauses: then the file is saved atomically and
/// the project is compiled, in that order, off the main actor.
@MainActor
@Observable
final class Workspace {
    private(set) var project: ProjectInfo
    private(set) var fileURL: URL
    private(set) var syntax: Syntax?
    private(set) var isDirty = false
    private(set) var status: BuildStatus = .idle
    /// The last PDF that built successfully. A failed build never clears it.
    private(set) var pdf: PDFDocument?
    /// Diagnostics from the latest finished build, for every file.
    private(set) var diagnostics: [GrafCore.Diagnostic] = []
    private(set) var stats: Stats?
    private(set) var outline: [OutlineItem] = []
    /// A file error the writer needs to know about (a failed save, say).
    private(set) var fileError: String?

    var previewOpen = false
    var focusMode = true
    /// Zero-based page shown in the preview and the page peek.
    var previewPage = 0
    /// UTF-16 caret offset, kept for recents and restored on reopen.
    var caret = 0
    /// A request for the editor to move the caret to a one-based line. The
    /// id makes a repeated jump to the same line still count.
    private(set) var jumpRequest: (line: Int, id: UUID)?

    @ObservationIgnored let textStorage = NSTextStorage()
    /// True while the workspace replaces the whole text (opening a file), so
    /// the editor does not treat the load as an edit.
    @ObservationIgnored private(set) var isLoadingText = false
    /// Bumped on every edit. The core rejects builds of older revisions.
    @ObservationIgnored private var revision: UInt64 = 0
    @ObservationIgnored private let compiler = Compiler()
    @ObservationIgnored private let idle = Debouncer(delay: .milliseconds(800))
    @ObservationIgnored private let recents = RecentProjects()

    private init(project: ProjectInfo, fileURL: URL, text: String, caret: Int) {
        self.project = project
        self.fileURL = fileURL
        self.syntax = Self.syntax(for: fileURL)
        self.caret = caret
        textStorage.setAttributedString(NSAttributedString(string: text))
        let compiler = compiler
        Task.detached(priority: .utility) { compiler.warmUp() }
    }

    // MARK: Opening

    /// Opens a project folder, or the folder around a single file. Restores
    /// the file and caret from the last session when there is one.
    static func open(_ url: URL) async throws -> Workspace {
        let isDirectory = (try? url.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) ?? false
        let folder = isDirectory ? url : url.deletingLastPathComponent()
        let project = await Task.detached { openProject(directory: folder.path) }.value

        let remembered = RecentProjects().all.first { $0.path == project.root }
        let file: URL
        var caret = 0
        if !isDirectory {
            file = url
        } else if let last = remembered?.lastFile,
                  FileManager.default.fileExists(atPath: URL(fileURLWithPath: project.root).appending(path: last).path) {
            file = URL(fileURLWithPath: project.root).appending(path: last)
            caret = remembered?.caret ?? 0
        } else if let root = project.rootDocument {
            file = URL(fileURLWithPath: root)
        } else if let first = project.files.first(where: { $0.kind == .latex || $0.kind == .typst }) {
            file = URL(fileURLWithPath: first.path)
        } else {
            throw WorkspaceError.noDocument(folder: folder.lastPathComponent)
        }

        let text = try await Task.detached { try readText(path: file.path) }.value
        let workspace = Workspace(project: project, fileURL: file, text: text, caret: caret)
        workspace.remember()
        await workspace.commit()
        return workspace
    }

    /// Switches to another file in the project, saving the current one first.
    func openFile(_ url: URL) async {
        guard url != fileURL else { return }
        await idle.flush { await self.commit() }
        do {
            let text = try await Task.detached { try readText(path: url.path) }.value
            fileURL = url
            syntax = Self.syntax(for: url)
            caret = 0
            revision += 1
            compiler.sourceEdited(revision: revision)
            isLoadingText = true
            textStorage.setAttributedString(NSAttributedString(string: text))
            isLoadingText = false
            isDirty = false
            fileError = nil
            remember()
            await refreshAnalysis(text: text)
        } catch {
            fileError = error.localizedDescription
        }
    }

    // MARK: Editing

    /// Called by the editor after every change to `textStorage`.
    func didEdit() {
        revision += 1
        isDirty = true
        status = .edited
        compiler.sourceEdited(revision: revision)
        idle.schedule { await self.commit() }
    }

    /// Saves and compiles now instead of waiting for a pause (⌘S).
    func commitNow() async {
        idle.cancel()
        await commit()
    }

    /// Saves synchronously. Used when the app quits, where there is no time
    /// for a background task.
    func saveBeforeQuit() {
        idle.cancel()
        guard isDirty else { return }
        do {
            try saveText(path: fileURL.path, text: textStorage.string)
            isDirty = false
        } catch {
            fileError = error.localizedDescription
        }
        remember()
    }

    /// Save, then analyze and compile the snapshot taken at this revision.
    private func commit() async {
        let text = textStorage.string
        let snapshot = revision
        let path = fileURL.path

        if isDirty {
            do {
                try await Task.detached { try saveText(path: path, text: text) }.value
                if snapshot == revision { isDirty = false }
                fileError = nil
            } catch {
                fileError = "Couldn't save \(fileURL.lastPathComponent): \(error.localizedDescription)"
                return
            }
        }

        await refreshAnalysis(text: text)
        await compile(text: text, revision: snapshot)
    }

    private func refreshAnalysis(text: String) async {
        guard let engine = currentEngine else {
            stats = nil
            outline = []
            return
        }
        let (stats, outline) = await Task.detached {
            (GrafCore.stats(text: text, engine: engine), GrafCore.outline(text: text, engine: engine))
        }.value
        self.stats = stats
        self.outline = outline
    }

    // MARK: Compiling

    private func compile(text: String, revision snapshot: UInt64) async {
        guard let engine = buildEngine else {
            status = .idle
            return
        }
        // The project's root document drives the build, from disk (just
        // saved). A lone file compiles from the snapshot.
        let input = CompileInput(
            engine: engine,
            text: text,
            revision: snapshot,
            projectRoot: project.root,
            rootDocument: project.rootDocument
        )
        status = .compiling
        let compiler = compiler
        let outcome = await Task.detached { () -> Result<CompileSuccess, CompileFailure> in
            do { return .success(try compiler.compile(input: input)) }
            catch let failure as CompileFailure { return .failure(failure) }
            catch { return .failure(.Failed(revision: snapshot, message: error.localizedDescription, diagnostics: [], durationMs: 0)) }
        }.value

        switch outcome {
        case let .success(result):
            guard let document = PDFDocument(data: result.pdf) else {
                status = .failed(errors: 0, message: "The PDF could not be read")
                return
            }
            pdf = document
            previewPage = min(previewPage, max(document.pageCount - 1, 0))
            diagnostics = result.diagnostics
            status = snapshot == revision ? .built(milliseconds: result.durationMs) : .edited
        case let .failure(.Failed(_, message, diagnostics, _)):
            self.diagnostics = diagnostics
            let errors = diagnostics.filter { $0.severity == .error }.count
            status = .failed(errors: errors, message: Self.shortMessage(message))
        case .failure(.Stale):
            // A newer edit is already queued; its build will report.
            break
        }
    }

    /// Diagnostics that point into the file being edited.
    var diagnosticsForCurrentFile: [GrafCore.Diagnostic] {
        diagnostics.filter { diagnostic in
            guard let file = diagnostic.file else { return true }
            return URL(fileURLWithPath: file, relativeTo: URL(fileURLWithPath: project.root)).standardizedFileURL.path
                == fileURL.standardizedFileURL.path
                || URL(fileURLWithPath: file).lastPathComponent == fileURL.lastPathComponent
        }
    }

    // MARK: Helpers

    func jump(toLine line: Int) {
        jumpRequest = (line, UUID())
    }

    var fileName: String { fileURL.lastPathComponent }

    /// Path relative to the project root, for recents and display.
    var relativePath: String {
        let root = project.root.hasSuffix("/") ? project.root : project.root + "/"
        return fileURL.path.hasPrefix(root) ? String(fileURL.path.dropFirst(root.count)) : fileURL.lastPathComponent
    }

    private var currentEngine: Engine? {
        switch syntax {
        case .latex: .latex
        case .typst: .typst
        case nil: nil
        }
    }

    /// The engine for the whole project: the root document's, else the
    /// current file's.
    private var buildEngine: Engine? {
        if let root = project.rootDocument {
            return Self.syntax(for: URL(fileURLWithPath: root)) == .typst ? .typst : .latex
        }
        return currentEngine
    }

    func remember() {
        recents.record(RecentProject(
            path: project.root,
            name: project.name,
            lastFile: relativePath,
            caret: caret
        ))
    }

    private static func syntax(for url: URL) -> Syntax? {
        switch url.pathExtension.lowercased() {
        case "tex", "sty", "cls": .latex
        case "typ": .typst
        default: nil
        }
    }

    /// Keeps a backend's first line, which is the part worth showing.
    private static func shortMessage(_ message: String) -> String {
        let line = message.split(separator: "\n").first.map(String.init) ?? message
        return line.count > 80 ? String(line.prefix(79)) + "…" : line
    }
}

enum WorkspaceError: LocalizedError {
    case noDocument(folder: String)

    var errorDescription: String? {
        switch self {
        case let .noDocument(folder): "\(folder) has no LaTeX or Typst file to open."
        }
    }
}
