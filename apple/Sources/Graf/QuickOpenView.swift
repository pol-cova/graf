import GrafCore
import SwiftUI

/// Go to… (⌘K): one search over the current file's sections and every file
/// in the project. Return goes there, ⌘Return opens a file in a new tab,
/// Esc goes back to writing with the caret where it was.
struct QuickOpenView: View {
    let workspace: Workspace
    @Binding var isPresented: Bool
    @Environment(\.openWindow) private var openWindow
    @State private var query = ""
    @State private var selection = 0
    @FocusState private var fieldFocused: Bool

    private enum Result: Identifiable {
        case section(OutlineItem)
        case file(ProjectFile)

        var id: String {
            switch self {
            case let .section(item): "section:\(item.line):\(item.title)"
            case let .file(file): "file:\(file.path)"
            }
        }
    }

    var body: some View {
        ZStack(alignment: .top) {
            // The page fades rather than going dark.
            Color(nsColor: .windowBackgroundColor).opacity(0.72)
                .onTapGesture { close() }
                .accessibilityHidden(true)

            VStack(spacing: 0) {
                HStack(spacing: 10) {
                    Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
                    TextField("Go to section or file", text: $query)
                        .textFieldStyle(.plain)
                        .font(Theme.Chrome.headingUI)
                        .focused($fieldFocused)
                        .onSubmit { activate(inNewTab: false) }
                }
                .padding(.horizontal, 16)
                .frame(height: 52)

                Divider()

                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 0) {
                            if results.isEmpty {
                                Text("No matches")
                                    .foregroundStyle(.tertiary)
                                    .padding(16)
                            }
                            ForEach(Array(results.enumerated()), id: \.element.id) { index, result in
                                row(result, selected: index == selection)
                                    .id(index)
                                    .contentShape(Rectangle())
                                    .onTapGesture {
                                        selection = index
                                        activate(inNewTab: false)
                                    }
                            }
                        }
                        .padding(6)
                    }
                    .frame(maxHeight: 340)
                    .onChange(of: selection) { proxy.scrollTo(selection) }
                }

                Divider()

                Text("↩ go   ⌘↩ open in new tab   esc back to writing")
                    .font(Theme.Chrome.captionMonoUI)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 16)
                    .frame(height: 30)
            }
            .frame(width: 580)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
            .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(.separator))
            .shadow(color: .black.opacity(0.14), radius: 30, y: 12)
            .padding(.top, 90)
        }
        .onAppear { fieldFocused = true }
        .onChange(of: query) { selection = 0 }
        .onKeyPress(.downArrow) { move(1) }
        .onKeyPress(.upArrow) { move(-1) }
        .onKeyPress(.escape) {
            close()
            return .handled
        }
        .onKeyPress(.return, phases: .down) { press in
            guard press.modifiers.contains(.command) else { return .ignored }
            activate(inNewTab: true)
            return .handled
        }
    }

    @ViewBuilder
    private func row(_ result: Result, selected: Bool) -> some View {
        HStack(spacing: 12) {
            switch result {
            case let .section(item):
                Text("section")
                    .font(Theme.Chrome.captionMonoUI)
                    .foregroundStyle(selected ? Color.grafLink : .secondary)
                    .frame(width: 56, alignment: .leading)
                Text(item.title).font(selected ? Theme.Chrome.bodyEmphasizedUI : Theme.Chrome.bodyUI)
                Spacer()
                Text("line \(item.line)").font(Theme.Chrome.calloutUI).foregroundStyle(.secondary)
            case let .file(file):
                Text(fileKindLabel(file.kind))
                    .font(Theme.Chrome.captionMonoUI)
                    .foregroundStyle(selected ? Color.grafLink : .secondary)
                    .frame(width: 56, alignment: .leading)
                Text(file.relative)
                    .font(Theme.Chrome.rowMonoUI)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer()
                if file.path == workspace.fileURL.path {
                    Text("open").font(Theme.Chrome.calloutUI).foregroundStyle(.secondary)
                }
            }
        }
        .padding(.horizontal, 10)
        .frame(height: 38)
        .background(
            selected ? Color.grafLink.opacity(0.12) : .clear,
            in: RoundedRectangle(cornerRadius: 7)
        )
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    /// Sections first, since they are closest to the caret, then files.
    private var results: [Result] {
        let sections = workspace.outline
        let files = workspace.project.files
        let sectionHits = filterMatches(query: query, candidates: sections.map(\.title))
        let fileHits = filterMatches(query: query, candidates: files.map(\.relative))
        return sectionHits.map { .section(sections[Int($0)]) } + fileHits.map { .file(files[Int($0)]) }
    }

    private func move(_ step: Int) -> KeyPress.Result {
        guard !results.isEmpty else { return .handled }
        selection = (selection + step + results.count) % results.count
        return .handled
    }

    private func activate(inNewTab: Bool) {
        let results = results
        guard results.indices.contains(selection) else { return }
        switch results[selection] {
        case let .section(item):
            workspace.jump(toLine: Int(item.line))
        case let .file(file):
            let url = URL(fileURLWithPath: file.path)
            if inNewTab {
                openInNewTab(url, using: openWindow)
            } else if !WindowRegistry.shared.focusWindow(showing: url) {
                Task { await workspace.openFile(url) }
            }
        }
        close()
    }

    private func close() {
        isPresented = false
    }

    private func fileKindLabel(_ kind: FileKind) -> String {
        switch kind {
        case .latex: "tex"
        case .typst: "typ"
        case .bibtex: "bib"
        case .style: "sty"
        case .image: "image"
        case .pdf: "pdf"
        case .other: "file"
        }
    }
}
