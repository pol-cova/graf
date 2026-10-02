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
        .commands {
            // Find, Find and Replace, Find Next, Use Selection, Spelling:
            // the system versions, routed to the text view's find bar.
            TextEditingCommands()
            GrafCommands()
        }

        Settings {
            SettingsView()
        }
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
/// Halfway through, `GRAF_SNAPSHOT_KEY` presses a shortcut ("p", or
/// "shift+cmd+f"; space-separated keys run in order), `GRAF_SNAPSHOT_TYPE`
/// types at the caret, and `GRAF_SNAPSHOT_TYPE_AFTER` types after the keys.
/// Typing exercises the edit, save, and compile path.
@MainActor
enum DebugSnapshot {
    static func scheduleIfRequested() {
        let environment = ProcessInfo.processInfo.environment
        guard let path = environment["GRAF_SNAPSHOT"] else { return }
        let delay = Double(environment["GRAF_SNAPSHOT_DELAY"] ?? "") ?? 6
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(delay / 2))
            // Menu commands act on the key window, as they would for a user.
            // Cooperative activation yields to whatever app is in front, so
            // this debug harness takes focus the old way.
            NSApp.activate(ignoringOtherApps: true)
            NSApp.windows.first { $0.isVisible && $0.frame.width > 300 }?.makeKeyAndOrderFront(nil)
            try? await Task.sleep(for: .milliseconds(300))
            if let text = environment["GRAF_SNAPSHOT_TYPE"] {
                type(text)
            }
            if let keys = environment["GRAF_SNAPSHOT_KEY"] {
                // Space-separated shortcuts run in order, a beat apart.
                for shortcut in keys.split(separator: " ") {
                    press(String(shortcut))
                    try? await Task.sleep(for: .milliseconds(400))
                }
            }
            if let text = environment["GRAF_SNAPSHOT_TYPE_AFTER"] {
                type(text)
            }
            try? await Task.sleep(for: .seconds(delay / 2))
            capture(to: URL(fileURLWithPath: path))
            NSApp.terminate(nil)
        }
    }

    /// Presses a shortcut such as `p`, `shift+cmd+f`, or `opt+cmd+f` through
    /// the main menu, exactly as the keyboard would. A bare key means ⌘key.
    private static func press(_ shortcut: String) {
        var parts = shortcut.split(separator: "+").map(String.init)
        guard let key = parts.popLast(), let window = NSApp.keyWindow else { return }
        var flags: NSEvent.ModifierFlags = parts.isEmpty ? .command : []
        for part in parts {
            switch part {
            case "cmd": flags.insert(.command)
            case "shift": flags.insert(.shift)
            case "opt": flags.insert(.option)
            case "ctrl": flags.insert(.control)
            default: break
            }
        }
        guard let event = NSEvent.keyEvent(
            with: .keyDown, location: .zero, modifierFlags: flags,
            timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
            context: nil, characters: key, charactersIgnoringModifiers: key,
            isARepeat: false, keyCode: 0
        ) else { return }
        let handled = NSApp.mainMenu?.performKeyEquivalent(with: event) ?? false
        FileHandle.standardError.write(Data("snapshot: \(shortcut) handled=\(handled)\n".utf8))
    }

    private static func type(_ text: String) {
        guard let window = NSApp.windows.first(where: { $0.isVisible && $0.frame.width > 300 }),
              let textView = window.contentView?.firstDescendant(ofType: NSTextView.self)
        else { return }
        window.makeFirstResponder(textView)
        textView.insertText(text, replacementRange: textView.selectedRange())
    }

    /// Writes the main window to `url`, and every other visible window
    /// (Settings, a completion list) next to it as `name-1.png`, `name-2.png`.
    private static func capture(to url: URL) {
        let windows = NSApp.windows
            .filter { $0.isVisible && $0.frame.width > 40 }
            .sorted { $0.frame.width * $0.frame.height > $1.frame.width * $1.frame.height }
        guard !windows.isEmpty else {
            FileHandle.standardError.write(Data("snapshot: no visible window\n".utf8))
            return
        }
        for (index, window) in windows.enumerated() {
            let name = url.deletingPathExtension().lastPathComponent
            let target = index == 0 ? url : url.deletingLastPathComponent().appending(path: "\(name)-\(index).png")
            guard let view = window.contentView?.superview ?? window.contentView,
                  let bitmap = view.bitmapImageRepForCachingDisplay(in: view.bounds)
            else { continue }
            view.cacheDisplay(in: view.bounds, to: bitmap)
            do {
                try bitmap.representation(using: .png, properties: [:])?.write(to: target)
                FileHandle.standardError.write(Data("snapshot: \(target.lastPathComponent) \(String(describing: Swift.type(of: window))) \(Int(window.frame.width))x\(Int(window.frame.height))\n".utf8))
            } catch {
                FileHandle.standardError.write(Data("snapshot: \(error)\n".utf8))
            }
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
            Divider()
            Button("Go to…") { window?.quickOpenRequest += 1 }
                .keyboardShortcut("k")
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
