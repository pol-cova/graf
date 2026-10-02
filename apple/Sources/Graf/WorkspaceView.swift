import GrafCore
import SwiftUI

/// A project window: the text, and nothing else until asked for.
struct WorkspaceView: View {
    @Bindable var workspace: Workspace
    @Binding var sidebar: NavigationSplitViewVisibility
    /// Bumped by the Go to… command (⌘K) to show the quick open bar.
    let quickOpenRequest: Int
    @State private var quickOpen = false
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
            .safeAreaInset(edge: .top, spacing: 0) {
                if !workspace.recovered.isEmpty {
                    RecoveryBar(workspace: workspace)
                        .transition(.move(edge: .top).combined(with: .opacity))
                }
            }
            .overlay {
                if quickOpen {
                    QuickOpenView(workspace: workspace, isPresented: $quickOpen)
                        .transition(.opacity.combined(with: .scale(scale: 0.98, anchor: .top)))
                }
            }
            .animation(.easeOut(duration: 0.12), value: quickOpen)
            .onChange(of: quickOpenRequest) { quickOpen = true }
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

/// One quiet line offering unsaved text a previous session left behind.
private struct RecoveryBar: View {
    let workspace: Workspace

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "clock.arrow.circlepath")
                .foregroundStyle(Color.grafLink)
            Text(message)
                .lineLimit(1)
            Spacer()
            Button("Discard") { withAnimation { workspace.discardRecovered() } }
                .buttonStyle(.borderless)
            Button("Restore") { Task { await workspace.restoreRecovered() } }
                .buttonStyle(.borderedProminent)
                .controlSize(.small)
        }
        .font(.system(size: 13))
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
        .background(.bar)
        .overlay(alignment: .bottom) { Divider() }
    }

    private var message: String {
        let changes = workspace.recovered
        guard let newest = changes.max(by: { $0.timestamp < $1.timestamp }) else { return "" }
        let time = Date(timeIntervalSince1970: TimeInterval(newest.timestamp))
            .formatted(date: .omitted, time: .shortened)
        if changes.count == 1 {
            return "Recovered unsaved changes to \(URL(fileURLWithPath: newest.path).lastPathComponent) from \(time)"
        }
        return "Recovered unsaved changes to \(changes.count) files, latest from \(time)"
    }
}
