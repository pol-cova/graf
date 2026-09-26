import GrafCore
import SwiftUI

/// The sidebar: the outline of the current file, then the project's files.
/// Hidden by default; the writer asks for it.
struct NavigatorView: View {
    let workspace: Workspace
    @State private var filter = ""
    @State private var filesExpanded = false

    var body: some View {
        List {
            Section {
                if visibleOutline.isEmpty {
                    Text(workspace.outline.isEmpty ? "No sections yet" : "No matches")
                        .foregroundStyle(.tertiary)
                }
                ForEach(Array(visibleOutline.enumerated()), id: \.offset) { _, item in
                    Button {
                        workspace.jump(toLine: Int(item.line))
                    } label: {
                        Text(item.title)
                            .lineLimit(1)
                            .padding(.leading, CGFloat(max(Int(item.level) - 1, 0)) * 14)
                    }
                    .buttonStyle(.plain)
                }
            } header: {
                HStack {
                    Text("Outline")
                    Spacer()
                    if let words = workspace.stats?.words {
                        Text("\(words.formatted()) words").foregroundStyle(.tertiary)
                    }
                }
            }

            Section("Files", isExpanded: $filesExpanded) {
                ForEach(visibleFiles, id: \.path) { file in
                    Button {
                        Task { await workspace.openFile(URL(fileURLWithPath: file.path)) }
                    } label: {
                        Label(file.relative, systemImage: icon(for: file.kind))
                            .lineLimit(1)
                            .foregroundStyle(isCurrent(file) ? Color.grafLink : .primary)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .listStyle(.sidebar)
        .searchable(text: $filter, placement: .sidebar, prompt: "Filter")
    }

    private var visibleOutline: [OutlineItem] {
        guard !filter.isEmpty else { return workspace.outline }
        return workspace.outline.filter { $0.title.localizedCaseInsensitiveContains(filter) }
    }

    private var visibleFiles: [ProjectFile] {
        guard !filter.isEmpty else { return workspace.project.files }
        return workspace.project.files.filter { $0.relative.localizedCaseInsensitiveContains(filter) }
    }

    private func isCurrent(_ file: ProjectFile) -> Bool {
        file.path == workspace.fileURL.path
    }

    private func icon(for kind: FileKind) -> String {
        switch kind {
        case .latex, .typst: "doc.text"
        case .bibtex: "books.vertical"
        case .style: "gearshape"
        case .image: "photo"
        case .pdf: "doc.richtext"
        case .other: "doc"
        }
    }
}
