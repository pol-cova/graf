import GrafCore
import SwiftUI

/// A project window: the text, and nothing else until asked for.
struct WorkspaceView: View {
    @Bindable var workspace: Workspace
    @Binding var sidebar: NavigationSplitViewVisibility
    @Namespace private var page

    var body: some View {
        NavigationSplitView(columnVisibility: $sidebar) {
            NavigatorView(workspace: workspace)
                .navigationSplitViewColumnWidth(min: 220, ideal: 260, max: 340)
        } detail: {
            GeometryReader { proxy in
                HStack(spacing: 0) {
                    writingColumn
                    if workspace.previewOpen {
                        Divider()
                        PreviewPanel(workspace: workspace)
                            .frame(width: proxy.size.width * 0.46)
                            .matchedGeometryEffect(id: "page", in: page)
                            .transition(.move(edge: .trailing).combined(with: .opacity))
                    }
                }
            }
        }
        .toolbar { toolbar }
        .navigationTitle(workspace.fileName)
        .navigationSubtitle(workspace.project.name)
    }

    private var writingColumn: some View {
        EditorView(workspace: workspace)
            .overlay(alignment: .trailing) {
                if !workspace.previewOpen, let pdf = workspace.pdf {
                    PagePeek(document: pdf, pageIndex: workspace.previewPage) {
                        withAnimation(.previewSpring) { workspace.previewOpen = true }
                    }
                    .matchedGeometryEffect(id: "page", in: page)
                    .padding(.trailing, 28)
                    .transition(.opacity)
                }
            }
            .safeAreaInset(edge: .bottom, spacing: 0) { footer }
    }

    private var footer: some View {
        HStack(spacing: 16) {
            if let stats = workspace.stats {
                Text("\(stats.words.formatted()) words · \(max(Int(stats.readingMinutes.rounded()), 1)) min read")
            }
            if let error = workspace.fileError {
                Label(error, systemImage: "exclamationmark.triangle.fill")
                    .foregroundStyle(Color.grafError)
                    .lineLimit(1)
            }
            Spacer()
            Toggle("Focus", isOn: $workspace.focusMode)
                .toggleStyle(.button)
                .buttonStyle(.borderless)
                .fontWeight(workspace.focusMode ? .medium : .regular)
                .foregroundStyle(workspace.focusMode ? .primary : .secondary)
                .help("Dim everything but the paragraph you're writing (⇧⌘F)")
        }
        .font(.system(size: 12))
        .foregroundStyle(.secondary)
        .padding(.horizontal, 20)
        .frame(height: 32)
        .background(.background)
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .status) {
            HStack(spacing: 6) {
                Circle()
                    .fill(statusColor)
                    .frame(width: 6, height: 6)
                    // Breathes on a 1.2s cycle while a build runs.
                    .phaseAnimator([1.0, 0.35]) { dot, phase in
                        dot.opacity(workspace.status == .compiling ? phase : 1)
                    } animation: { _ in .easeInOut(duration: 0.6) }
                Text(workspace.status.label)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                if workspace.isDirty {
                    Text("·").foregroundStyle(.tertiary)
                    Text("Edited").foregroundStyle(.tertiary)
                }
            }
            .font(.system(size: 12))
            .accessibilityElement(children: .combine)
        }
        ToolbarItem(placement: .primaryAction) {
            Button {
                withAnimation(.previewSpring) { workspace.previewOpen.toggle() }
            } label: {
                Label("Preview", systemImage: "sidebar.right")
            }
            .help(workspace.previewOpen ? "Close Preview (⌘P)" : "Open Preview (⌘P)")
        }
    }

    private var statusColor: Color {
        switch workspace.status {
        case .failed: .grafError
        case .built: .grafLink
        case .compiling: .grafLink.opacity(0.5)
        case .idle, .edited: Color(nsColor: .tertiaryLabelColor)
        }
    }
}
