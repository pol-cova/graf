import AppKit
import PDFKit
import SwiftUI

/// The side panel: the last good PDF in a PDFKit view.
struct PreviewPanel: View {
    @Bindable var workspace: Workspace

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(pageLabel)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                Spacer()
                if case .failed = workspace.status, workspace.pdf != nil {
                    Label("Showing last good build", systemImage: "clock.arrow.circlepath")
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                }
                Button {
                    withAnimation(.previewSpring) { workspace.previewOpen = false }
                } label: {
                    Image(systemName: "xmark")
                }
                .buttonStyle(.borderless)
                .help("Close Preview (⌘P)")
            }
            .padding(.horizontal, 14)
            .frame(height: 36)

            if let pdf = workspace.pdf {
                PDFPreview(document: pdf, page: $workspace.previewPage)
            } else {
                ContentUnavailableView {
                    Label(emptyTitle, systemImage: "doc.richtext")
                } description: {
                    Text(emptyDescription)
                }
            }
        }
        .background(Color.grafSurface)
    }

    private var pageLabel: String {
        guard let pdf = workspace.pdf else { return "Preview" }
        return "Page \(workspace.previewPage + 1) of \(pdf.pageCount)"
    }

    private var emptyTitle: String {
        if case .compiling = workspace.status { return "Building…" }
        return "No preview yet"
    }

    private var emptyDescription: String {
        if case let .failed(_, message) = workspace.status { return message }
        return "The PDF appears here after the first build."
    }
}

/// PDFKit view that swaps in a new document at the page the writer was
/// reading, so a rebuild never jumps back to page one.
struct PDFPreview: NSViewRepresentable {
    let document: PDFDocument
    @Binding var page: Int

    func makeCoordinator() -> Coordinator { Coordinator(page: $page) }

    func makeNSView(context: Context) -> PDFView {
        let view = PDFView()
        view.autoScales = true
        view.displayMode = .singlePageContinuous
        view.displaysPageBreaks = true
        view.backgroundColor = Theme.surface
        view.document = document
        if let target = document.page(at: page) { view.go(to: target) }
        NotificationCenter.default.addObserver(
            context.coordinator,
            selector: #selector(Coordinator.pageChanged(_:)),
            name: .PDFViewPageChanged,
            object: view
        )
        return view
    }

    func updateNSView(_ view: PDFView, context: Context) {
        guard view.document !== document else { return }
        // Keep the reader's place: same page, same point on it.
        let destination = view.currentDestination
        let pageIndex = destination?.page.flatMap { view.document?.index(for: $0) } ?? page
        let point = destination?.point
        view.document = document
        guard let target = document.page(at: min(pageIndex, max(document.pageCount - 1, 0))) else { return }
        if let point {
            view.go(to: PDFDestination(page: target, at: point))
        } else {
            view.go(to: target)
        }
    }

    @MainActor
    final class Coordinator: NSObject {
        let page: Binding<Int>

        init(page: Binding<Int>) { self.page = page }

        @objc func pageChanged(_ notification: Notification) {
            guard let view = notification.object as? PDFView,
                  let current = view.currentPage,
                  let index = view.document?.index(for: current)
            else { return }
            if page.wrappedValue != index { page.wrappedValue = index }
        }
    }
}

/// The small page at the window's edge. It lifts on hover and grows into
/// the preview panel when clicked.
struct PagePeek: View {
    let document: PDFDocument
    let pageIndex: Int
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        VStack(spacing: 8) {
            Group {
                if let image = thumbnail {
                    Image(nsImage: image).resizable().interpolation(.high)
                } else {
                    Color.white
                }
            }
            .frame(width: 112, height: 146)
            .clipShape(RoundedRectangle(cornerRadius: 3))
            .overlay(RoundedRectangle(cornerRadius: 3).strokeBorder(.separator))
            .shadow(color: .black.opacity(hovering ? 0.14 : 0.05), radius: hovering ? 14 : 2, y: hovering ? 8 : 1)
            .scaleEffect(hovering ? 1.1 : 1, anchor: .trailing)

            Text("p. \(pageIndex + 1)")
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
        }
        .contentShape(Rectangle())
        .onHover { inside in
            withAnimation(.easeOut(duration: 0.16)) { hovering = inside }
        }
        .onTapGesture(perform: action)
        .help("Open Preview (⌘P)")
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Page \(pageIndex + 1) preview")
        .accessibilityHint("Opens the PDF preview")
        .accessibilityAddTraits(.isButton)
        .accessibilityAction(.default, action)
    }

    private var thumbnail: NSImage? {
        document.page(at: pageIndex)?.thumbnail(of: NSSize(width: 224, height: 292), for: .cropBox)
    }
}

extension Animation {
    /// The one spring every panel uses, so column and panel move together.
    static let previewSpring = Animation.spring(response: 0.3, dampingFraction: 0.9)
}
