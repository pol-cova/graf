import AppKit
import GrafCore
import GrafKit
import SwiftUI

/// Name, engine, template. The folder is shown before it exists.
struct NewProjectSheet: View {
    let onCreate: (URL) -> Void
    @Environment(\.dismiss) private var dismiss

    @State private var name = ""
    @State private var engine: Engine = .latex
    @State private var templateID: String?
    @State private var location = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first
        ?? URL(fileURLWithPath: NSHomeDirectory())
    @State private var error: String?
    @State private var isCreating = false
    private let allTemplates = templates()

    var body: some View {
        HStack(spacing: 0) {
            form
                .frame(width: 400)
                .padding(28)
            Divider()
            folderPreview
                .frame(width: 280, alignment: .topLeading)
                .padding(28)
                .background(Color.grafSurface.opacity(0.5))
        }
        .frame(height: 440)
        .onAppear { templateID = visibleTemplates.first?.id }
        .onChange(of: engine) { templateID = visibleTemplates.first?.id }
    }

    private var form: some View {
        VStack(alignment: .leading, spacing: 22) {
            TextField("Project name", text: $name)
                .textFieldStyle(.plain)
                .font(Theme.Chrome.sheetTitleUI)
                .padding(.bottom, 6)
                .overlay(alignment: .bottom) { Divider() }

            Picker("Engine", selection: $engine) {
                Text("LaTeX").tag(Engine.latex)
                Text("Typst").tag(Engine.typst)
            }
            .pickerStyle(.segmented)
            .fixedSize()

            VStack(alignment: .leading, spacing: 8) {
                Text("Start from").font(Theme.Chrome.calloutUI).foregroundStyle(.secondary)
                List(visibleTemplates, id: \.id, selection: $templateID) { template in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(template.name).font(Theme.Chrome.rowMediumUI)
                        Text(template.description).font(Theme.Chrome.calloutUI).foregroundStyle(.secondary)
                    }
                    .padding(.vertical, 3)
                }
                .listStyle(.bordered)
                .frame(height: 160)
            }

            if let error {
                Label(error, systemImage: "exclamationmark.triangle.fill")
                    .foregroundStyle(Color.grafError)
                    .font(Theme.Chrome.calloutUI)
            }

            Spacer(minLength: 0)

            HStack {
                Button(location.abbreviatedPath) { chooseLocation() }
                    .buttonStyle(.link)
                    .foregroundStyle(.secondary)
                    .font(Theme.Chrome.calloutMonoUI)
                    .help("Choose where the project folder is created")
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Create and Start Writing") { create() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(templateID == nil || isCreating)
            }
        }
    }

    private var folderPreview: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("This creates").font(Theme.Chrome.calloutUI).foregroundStyle(.secondary)
            Text(slug + "/")
                .font(Theme.Chrome.rowMonoSemiboldUI)
                .foregroundStyle(Color.grafLink)
            if let template = selectedTemplate {
                HStack {
                    Text(template.fileName)
                    Spacer()
                    Text("opens here").font(Theme.Chrome.captionUI).foregroundStyle(.tertiary)
                }
                .font(Theme.Chrome.rowMonoUI)
                .padding(.leading, 16)
            }
            Spacer()
            Text("Plain files. Works with Git, iCloud Drive, or any other editor.")
                .font(Theme.Chrome.calloutUI)
                .foregroundStyle(.secondary)
        }
    }

    private var visibleTemplates: [Template] {
        allTemplates.filter { $0.engine == engine }
    }

    private var selectedTemplate: Template? {
        allTemplates.first { $0.id == templateID }
    }

    private var slug: String {
        folderSlug(for: name)
    }

    private func chooseLocation() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.directoryURL = location
        panel.prompt = "Choose"
        if panel.runModal() == .OK, let url = panel.url { location = url }
    }

    private func create() {
        guard let templateID else { return }
        let folder = location.appending(path: slug, directoryHint: .isDirectory)
        if FileManager.default.fileExists(atPath: folder.path) {
            error = "A folder named \(slug) already exists here."
            return
        }
        isCreating = true
        Task {
            do {
                _ = try await Task.detached {
                    try createProject(directory: folder.path, templateId: templateID)
                }.value
                dismiss()
                onCreate(folder)
            } catch {
                self.error = error.localizedDescription
                isCreating = false
            }
        }
    }
}

extension URL {
    /// `~/Documents` instead of `/Users/name/Documents`.
    var abbreviatedPath: String {
        (path as NSString).abbreviatingWithTildeInPath
    }
}
