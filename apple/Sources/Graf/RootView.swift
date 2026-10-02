import AppKit
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
    /// Bumped by Go to… (⌘K); the workspace view shows quick open.
    var quickOpenRequest = 0

    func open(_ url: URL) async {
        workspace?.saveBeforeQuit()
        isOpening = true
        defer { isOpening = false }
        do {
            let root = WindowRegistry.shared.projectRoot(containing: url)
            workspace = try await Workspace.open(url, projectRoot: root)
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
    private var windows: [ObjectIdentifier: NSWindow] = [:]

    /// The window already editing `url`, so a file is never open twice.
    /// Two editors on one file would each save over the other. Only
    /// `focusWindow(showing:)` looks this up.
    private func window(showing url: URL) -> NSWindow? {
        let target = url.standardizedFileURL
        return models.first { _, model in
            model.workspace?.fileURL.standardizedFileURL == target
        }.flatMap { windows[$0.key] }
    }

    /// Brings the tab or window editing `url` forward, if there is one.
    @discardableResult
    func focusWindow(showing url: URL) -> Bool {
        guard let window = window(showing: url) else { return false }
        window.makeKeyAndOrderFront(nil)
        return true
    }

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
        windows[ObjectIdentifier(window)] = window
        // Project windows share one native tab bar. The tabbing mode only
        // applies to windows shown later, so a window opened for a new tab
        // joins its host explicitly.
        window.tabbingIdentifier = "graf.project"
        window.tabbingMode = .preferred
        if let host = OpenRequests.shared.takeTabHost(for: window), window.tabbedWindows == nil {
            host.addTabbedWindow(window, ordered: .above)
            window.makeKeyAndOrderFront(nil)
        }
        if window.isKeyWindow { keyModel = model }
    }

    /// The root of an open project that contains `url`, so a file from that
    /// project opens in it instead of becoming a project of its own folder.
    func projectRoot(containing url: URL) -> URL? {
        let path = url.standardizedFileURL.path
        return models.values
            .compactMap { $0.workspace?.project.root }
            .filter { path.hasPrefix($0.hasSuffix("/") ? $0 : $0 + "/") }
            .max(by: { $0.count < $1.count })
            .map { URL(fileURLWithPath: $0, isDirectory: true) }
    }

    private func windowBecameKey(_ window: NSWindow?) {
        guard let window, let model = models[ObjectIdentifier(window)] else { return }
        keyModel = model
    }

    private func windowWillClose(_ window: NSWindow?) {
        guard let window else { return }
        let model = models.removeValue(forKey: ObjectIdentifier(window))
        windows.removeValue(forKey: ObjectIdentifier(window))
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
    /// The window the next new window should join as a tab.
    @ObservationIgnored private weak var tabHost: NSWindow?

    func enqueue(_ urls: [URL]) {
        pending.append(contentsOf: urls.map(\.standardizedFileURL))
    }

    func take() -> URL? {
        pending.isEmpty ? nil : pending.removeFirst()
    }

    func joinNextWindow(to host: NSWindow?) {
        tabHost = host
    }

    /// Hands out the tab host once, to the window created for the request.
    func takeTabHost(for window: NSWindow) -> NSWindow? {
        guard let host = tabHost, host !== window else { return nil }
        tabHost = nil
        return host
    }
}

/// Opens `url` in its own tab, or brings forward the tab already showing it.
@MainActor
func openInNewTab(_ url: URL, using openWindow: OpenWindowAction) {
    guard !WindowRegistry.shared.focusWindow(showing: url) else { return }
    OpenRequests.shared.joinNextWindow(to: NSApp.keyWindow ?? NSApp.mainWindow)
    OpenRequests.shared.enqueue([url])
    openWindow(id: "project")
}

struct RootView: View {
    @State private var model = WindowModel()
    @Environment(\.openWindow) private var openWindow
    private let requests = OpenRequests.shared

    var body: some View {
        Group {
            if let workspace = model.workspace {
                WorkspaceView(workspace: workspace, sidebar: $model.sidebar, quickOpenRequest: model.quickOpenRequest)
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
            if model.workspace == nil {
                requests.enqueue([url])
            } else {
                // This window is busy: the file gets its own tab.
                openInNewTab(url, using: openWindow)
            }
        }
        // Prefer an empty window for new requests; any window may accept.
        .handlesExternalEvents(preferring: model.workspace == nil ? ["*"] : [], allowing: ["*"])
        .onChange(of: requests.pending, initial: true) {
            // Only an empty window takes a waiting request.
            guard model.workspace == nil, !model.isOpening, let url = requests.take() else { return }
            if WindowRegistry.shared.focusWindow(showing: url) { return }
            Task { await model.open(url) }
        }
    }
}
