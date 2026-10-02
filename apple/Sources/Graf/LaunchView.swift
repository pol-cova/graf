import GrafKit
import SwiftUI

/// "Pick up where you left off." The last project is already selected, so
/// Return reopens it at the line where the writer stopped.
struct LaunchView: View {
    let model: WindowModel
    @State private var recents = RecentProjects().all.filter {
        FileManager.default.fileExists(atPath: $0.path)
    }
    @State private var selection: RecentProject.ID?

    var body: some View {
        VStack(alignment: .leading, spacing: 40) {
            VStack(alignment: .leading, spacing: 8) {
                Text(recents.isEmpty ? "Start writing." : "Pick up where you left off.")
                    .font(Theme.Chrome.wordmarkUI)
                Text(subtitle)
                    .font(Theme.Chrome.bodyUI)
                    .foregroundStyle(.secondary)
            }

            if !recents.isEmpty {
                List(recents, selection: $selection) { project in
                    RecentRow(project: project)
                        .tag(project.id)
                }
                .listStyle(.plain)
                .scrollContentBackground(.hidden)
                .frame(height: min(CGFloat(recents.count) * 56, 56 * 5))
                .contextMenu(forSelectionType: RecentProject.ID.self) { ids in
                    Button("Remove from Recents") {
                        ids.forEach { RecentProjects().remove(path: $0) }
                        recents.removeAll { ids.contains($0.id) }
                    }
                } primaryAction: { ids in
                    if let id = ids.first { open(id) }
                }
                .onKeyPress(.return) {
                    guard let selection else { return .ignored }
                    open(selection)
                    return .handled
                }
            }

            HStack(spacing: 24) {
                Button("New Project") { model.showNewProject = true }
                    .keyboardShortcut("n")
                Button("Open…") { model.chooseAndOpen() }
                    .keyboardShortcut("o")
                if model.isOpening { ProgressView().controlSize(.small) }
            }
            .buttonStyle(.link)
            .foregroundStyle(.primary)

            if let error = model.openError {
                Label(error, systemImage: "exclamationmark.triangle.fill")
                    .foregroundStyle(Color.grafError)
                    .font(Theme.Chrome.rowUI)
            }
        }
        .frame(maxWidth: 560, alignment: .leading)
        .padding(48)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.background)
        .onAppear { selection = recents.first?.id }
    }

    private var subtitle: String {
        guard let first = recents.first else {
            return "Create a project or open a folder with LaTeX or Typst files."
        }
        if let file = first.lastFile {
            return "Return opens \(first.name) at \(file)."
        }
        return "Return opens \(first.name)."
    }

    private func open(_ id: RecentProject.ID) {
        Task { await model.open(URL(fileURLWithPath: id)) }
    }
}

private struct RecentRow: View {
    let project: RecentProject

    var body: some View {
        HStack(spacing: 16) {
            VStack(alignment: .leading, spacing: 2) {
                Text(project.name)
                    .font(Theme.Chrome.headingSerifUI)
                Text(project.lastFile ?? project.path)
                    .font(Theme.Chrome.calloutUI)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer()
            Text(project.openedAt, format: .relative(presentation: .named))
                .font(Theme.Chrome.calloutUI)
                .foregroundStyle(.secondary)
        }
        .padding(.vertical, 6)
        .help(project.path)
    }
}
