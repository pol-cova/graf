import AppKit
import SwiftUI

@main
struct GrafApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    var body: some Scene {
        WindowGroup(id: "project") {
            RootView()
        }
        // Folders and files opened from Finder, `open`, or the command line
        // create a window when none is free to take them.
        .handlesExternalEvents(matching: ["*"])
        .defaultSize(width: 1180, height: 820)
        .windowToolbarStyle(.unifiedCompact(showsTitle: true))
        .commands { GrafCommands() }
    }
}

/// Makes the app a regular foreground app when launched straight from the
/// build folder, and brings it forward.
final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate()
        #if DEBUG
        DebugSnapshot.scheduleIfRequested()
        #endif
    }
}

#if DEBUG
/// Development aid: `GRAF_SNAPSHOT=/path/out.png` renders the key window to
/// a PNG after `GRAF_SNAPSHOT_DELAY` seconds (default 6), then quits. It
/// draws the app's own views, so it needs no screen-recording permission.
/// Halfway through, `GRAF_SNAPSHOT_ACTION=<menu title>` clicks a menu item
/// (for example "Show Preview") and `GRAF_SNAPSHOT_TYPE=<text>` types into
/// the editor at the caret, which exercises the edit, save, and compile path.
@MainActor
enum DebugSnapshot {
    static func scheduleIfRequested() {
        let environment = ProcessInfo.processInfo.environment
        guard let path = environment["GRAF_SNAPSHOT"] else { return }
        let delay = Double(environment["GRAF_SNAPSHOT_DELAY"] ?? "") ?? 6
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(delay / 2))
            // Menu commands act on the key window, as they would for a user.
            NSApp.activate()
            NSApp.windows.first { $0.isVisible && $0.frame.width > 300 }?.makeKeyAndOrderFront(nil)
            try? await Task.sleep(for: .milliseconds(300))
            if let text = environment["GRAF_SNAPSHOT_TYPE"] {
                type(text)
            }
            if let key = environment["GRAF_SNAPSHOT_KEY"] {
                pressCommand(key)
            }
            try? await Task.sleep(for: .seconds(delay / 2))
            capture(to: URL(fileURLWithPath: path))
            NSApp.terminate(nil)
        }
    }

    /// Presses ⌘ plus `key` through the main menu, exactly as the keyboard
    /// shortcut would.
    private static func pressCommand(_ key: String) {
        guard let window = NSApp.keyWindow,
              let event = NSEvent.keyEvent(
                  with: .keyDown, location: .zero, modifierFlags: .command,
                  timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
                  context: nil, characters: key, charactersIgnoringModifiers: key,
                  isARepeat: false, keyCode: 0
              )
        else { return }
        let handled = NSApp.mainMenu?.performKeyEquivalent(with: event) ?? false
        FileHandle.standardError.write(Data("snapshot: ⌘\(key) handled=\(handled)\n".utf8))
    }

    private static func type(_ text: String) {
        guard let window = NSApp.windows.first(where: { $0.isVisible && $0.frame.width > 300 }),
              let textView = window.contentView?.firstDescendant(ofType: NSTextView.self)
        else { return }
        window.makeFirstResponder(textView)
        textView.insertText(text, replacementRange: textView.selectedRange())
    }

    private static func capture(to url: URL) {
        guard let window = NSApp.windows.first(where: { $0.isVisible && $0.frame.width > 300 }) else {
            FileHandle.standardError.write(Data("snapshot: no visible window among \(NSApp.windows.map(\.frame))\n".utf8))
            return
        }
        guard let view = window.contentView?.superview ?? window.contentView,
              let bitmap = view.bitmapImageRepForCachingDisplay(in: view.bounds)
        else {
            FileHandle.standardError.write(Data("snapshot: could not allocate a bitmap\n".utf8))
            return
        }
        view.cacheDisplay(in: view.bounds, to: bitmap)
        do {
            try bitmap.representation(using: .png, properties: [:])?.write(to: url)
        } catch {
            FileHandle.standardError.write(Data("snapshot: \(error)\n".utf8))
        }
    }
}

private extension NSView {
    func firstDescendant<T: NSView>(ofType type: T.Type) -> T? {
        for subview in subviews {
            if let match = subview as? T ?? subview.firstDescendant(ofType: type) { return match }
        }
        return nil
    }
}
#endif

struct GrafCommands: Commands {
    @Environment(\.openWindow) private var openWindow
    @ObservedObject private var registry = WindowRegistry.shared
    private var window: WindowModel? { registry.keyModel }
    private var workspace: Workspace? { window?.workspace }

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("New Window") { openWindow(id: "project") }
                .keyboardShortcut("n", modifiers: [.command, .shift])
            Button("New Project…") { window?.showNewProject = true }
                .keyboardShortcut("n")
            Button("Open…") { window?.chooseAndOpen() }
                .keyboardShortcut("o")
        }
        CommandGroup(replacing: .saveItem) {
            Button("Save and Compile") {
                Task { await workspace?.commitNow() }
            }
            .keyboardShortcut("s")
        }
        // ⌘P opens the preview; printing belongs to the PDF, not the source.
        CommandGroup(replacing: .printItem) {}
        CommandGroup(after: .sidebar) {
            Toggle("Preview", isOn: Binding(
                get: { workspace?.previewOpen ?? false },
                set: { open in withAnimation(.previewSpring) { workspace?.previewOpen = open } }
            ))
            .keyboardShortcut("p")

            Toggle("Outline", isOn: Binding(
                get: { window?.sidebar != .detailOnly && window != nil },
                set: { show in withAnimation { window?.sidebar = show ? .all : .detailOnly } }
            ))
            .keyboardShortcut("1")

            Toggle("Focus Mode", isOn: Binding(
                get: { workspace?.focusMode ?? false },
                set: { workspace?.focusMode = $0 }
            ))
            .keyboardShortcut("f", modifiers: [.command, .shift])
        }
    }
}
