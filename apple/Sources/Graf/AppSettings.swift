import GrafCore
import Observation
import SwiftUI

/// The user's settings, shared by every window. Stored by the Rust core in
/// the app's settings file; edits save after a short pause.
@MainActor
@Observable
final class AppSettings {
    static let shared = AppSettings()

    var values: GrafCore.Settings {
        didSet {
            guard values != oldValue else { return }
            scheduleSave()
        }
    }
    private(set) var saveError: String?
    @ObservationIgnored private var pendingSave: Task<Void, Never>?

    private init() {
        values = loadSettings()
    }

    var proseSize: CGFloat { CGFloat(values.proseFontSize) }
    var compileDelay: Duration { .milliseconds(Int(values.compileDelayMs)) }

    private func scheduleSave() {
        pendingSave?.cancel()
        let snapshot = values
        pendingSave = Task {
            try? await Task.sleep(for: .milliseconds(400))
            guard !Task.isCancelled else { return }
            do {
                try await Task.detached { try saveSettings(settings: snapshot) }.value
                saveError = nil
            } catch {
                saveError = error.localizedDescription
            }
        }
    }
}

/// Graf › Settings (⌘,).
struct SettingsView: View {
    @Bindable private var settings = AppSettings.shared

    var body: some View {
        Form {
            Section("Writing") {
                Slider(value: proseSize, in: 14...28, step: 1) {
                    Text("Text size")
                } minimumValueLabel: {
                    Text("A").font(Theme.Chrome.captionUI)
                } maximumValueLabel: {
                    Text("A").font(Theme.Chrome.headingUI)
                }
                LabeledContent("Preview") {
                    Text("Readers rarely move in a straight line.")
                        .font(.system(size: CGFloat(settings.values.proseFontSize), design: .serif))
                        .lineLimit(1)
                }
                Toggle("Start new windows in Focus mode", isOn: $settings.values.focusMode)
                Picker("Tab key inserts", selection: $settings.values.tabSize) {
                    Text("2 spaces").tag(UInt32(2))
                    Text("4 spaces").tag(UInt32(4))
                    Text("8 spaces").tag(UInt32(8))
                }
            }

            Section("Building") {
                Toggle("Compile when I pause", isOn: $settings.values.autoCompile)
                Picker("Pause before compiling", selection: $settings.values.compileDelayMs) {
                    Text("0.3 seconds").tag(UInt64(300))
                    Text("0.5 seconds").tag(UInt64(500))
                    Text("0.8 seconds").tag(UInt64(800))
                    Text("1.5 seconds").tag(UInt64(1500))
                    Text("3 seconds").tag(UInt64(3000))
                }
                .disabled(!settings.values.autoCompile)
                Text("⌘S always saves and compiles right away.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Section("Citations") {
                Toggle("Include my Zotero library", isOn: $settings.values.useZotero)
                Text("Reads the Better BibTeX export in ~/Zotero. Nothing leaves your Mac.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            if let error = settings.saveError {
                Label(error, systemImage: "exclamationmark.triangle.fill")
                    .foregroundStyle(Color.grafError)
            }
        }
        .formStyle(.grouped)
        .frame(width: 460)
        .fixedSize(horizontal: false, vertical: true)
    }

    private var proseSize: Binding<Double> {
        Binding(
            get: { Double(settings.values.proseFontSize) },
            set: { settings.values.proseFontSize = Float($0) }
        )
    }
}
