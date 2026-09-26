import AppKit
import GrafCore
import GrafKit
import Observation
import SwiftUI
import UniformTypeIdentifiers

/// Per-window state: which project is open, or the launch screen.
@MainActor
@Observable
final class WindowModel {
    var workspace: Workspace?
    var showNewProject = false
    /// The navigator sidebar; hidden by default so the text comes first.
    var sidebar: NavigationSplitViewVisibility = .detailOnly
    var isOpening = false
    var openError: String?

    func open(_ url: URL) async {
        workspace?.saveBeforeQuit()
        isOpening = true
        defer { isOpening = false }
        do {
            workspace = try await Workspace.open(url)
            openError = nil
            WindowRegistry.shared.windowContentChanged()
        } catch {
            openError = error.localizedDescription
        }
    }

    /// Shows the system folder picker. Choosing a single .tex or .typ file
    /// opens its folder at that file.
    func chooseAndOpen() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = true
        panel.allowsMultipleSelection = false
        panel.allowedContentTypes = [.folder] + ["tex", "typ"].compactMap { UTType(filenameExtension: $0) }
        panel.prompt = "Open"
        panel.message = "Choose a project folder or a LaTeX or Typst file."
        guard panel.runModal() == .OK, let url = panel.url else { return }
        Task { await open(url) }
    }
}

/// The window model of the key window, for menu commands. SwiftUI focused
/// values don't reach the menu while an AppKit text view has focus, so
/// windows register here and the registry follows AppKit's key window. It is
/// an ObservableObject because that is what `Commands` re-render from.
@MainActor
final class WindowRegistry: ObservableObject {
    static let shared = WindowRegistry()
    @Published private(set) var keyModel: WindowModel?
    private var models: [ObjectIdentifier: WindowModel] = [:]

    /// Refreshes menu state after a window opens or closes a project.
    func windowContentChanged() {
        objectWillChange.send()
    }

    private init() {
        NotificationCenter.default.addObserver(
            forName: NSWindow.didBecomeKeyNotification, object: nil, queue: .main
        ) { notification in
            let window = notification.object as? NSWindow
            MainActor.assumeIsolated { self.windowBecameKey(window) }
        }
        NotificationCenter.default.addObserver(
            forName: NSWindow.willCloseNotification, object: nil, queue: .main
        ) { notification in
            let window = notification.object as? NSWindow
            MainActor.assumeIsolated { self.windowWillClose(window) }
        }
    }

    func register(_ model: WindowModel, for window: NSWindow) {
        models[ObjectIdentifier(window)] = model
        if window.isKeyWindow { keyModel = model }
    }

    private func windowBecameKey(_ window: NSWindow?) {
        guard let window, let model = models[ObjectIdentifier(window)] else { return }
        keyModel = model
    }

    private func windowWillClose(_ window: NSWindow?) {
        guard let window else { return }
        let model = models.removeValue(forKey: ObjectIdentifier(window))
        model?.workspace?.saveBeforeQuit()
        if keyModel === model { keyModel = nil }
    }
}

/// Reports the NSWindow hosting a SwiftUI view.
private struct WindowReader: NSViewRepresentable {
    let onWindow: (NSWindow) -> Void

    func makeNSView(context: Context) -> NSView { ReaderView(onWindow: onWindow) }
    func updateNSView(_ nsView: NSView, context: Context) {}

    private final class ReaderView: NSView {
        let onWindow: (NSWindow) -> Void
        init(onWindow: @escaping (NSWindow) -> Void) {
            self.onWindow = onWindow
            super.init(frame: .zero)
        }
        required init?(coder: NSCoder) { nil }
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if let window { onWindow(window) }
        }
    }
}

/// Folders and files the system asked Graf to open (Finder, `open`, or a
/// path on the command line), waiting for a window to take them.
@MainActor
@Observable
final class OpenRequests {
    static let shared = OpenRequests()
    private(set) var pending: [URL] = []

    func enqueue(_ urls: [URL]) {
        pending.append(contentsOf: urls.map(\.standardizedFileURL))
    }

    func take() -> URL? {
        pending.isEmpty ? nil : pending.removeFirst()
    }
}

struct RootView: View {
    @State private var model = WindowModel()
    @Environment(\.openWindow) private var openWindow
    private let requests = OpenRequests.shared

    var body: some View {
        Group {
            if let workspace = model.workspace {
                WorkspaceView(workspace: workspace, sidebar: $model.sidebar)
                    .id(ObjectIdentifier(workspace))
            } else {
                LaunchView(model: model)
            }
        }
        .frame(minWidth: 720, minHeight: 480)
        .sheet(isPresented: $model.showNewProject) {
            NewProjectSheet { url in
                Task { await model.open(url) }
            }
        }
        .background(WindowReader { window in
            WindowRegistry.shared.register(model, for: window)
        })
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.willTerminateNotification)) { _ in
            model.workspace?.saveBeforeQuit()
        }
        .onOpenURL { url in
            requests.enqueue([url])
        }
        // Prefer an empty window for new requests; any window may accept.
        .handlesExternalEvents(preferring: model.workspace == nil ? ["*"] : [], allowing: ["*"])
        .onChange(of: requests.pending, initial: true) {
            guard !requests.pending.isEmpty else { return }
            if model.workspace == nil, !model.isOpening, let url = requests.take() {
                Task { await model.open(url) }
            } else if model.workspace != nil {
                // This window is busy; a new one will take the request.
                openWindow(id: "project")
            }
        }
    }
}
