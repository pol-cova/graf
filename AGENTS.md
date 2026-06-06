# AGENTS.md

## Project

Graf is a fast, native workspace for technical writing built with Rust and GPUI.

## Current Milestone

M7 — Distribution & Polish

## Engineering Rules

1. Read the spec (`.docs/Graf-spec.md`) before major work.
2. Identify and work only on the current milestone.
3. Inspect existing code before creating replacements.
4. Avoid duplicate abstractions.
5. Keep patches reasonably small.
6. Keep `cargo check` passing at all times.
7. Add tests for non-trivial core logic.
8. Run relevant tests before completing work.
9. Never block the UI thread with expensive operations.
10. Avoid speculative optimization.
11. Avoid adding future features early.
12. No WebViews or Electron — use native GPUI rendering.
13. Preserve local-first architecture.
14. Document important architectural decisions.
15. Update the spec when a deliberate architectural change is made.

## Code Quality

Before completing any milestone:

```bash
cargo fmt --check
cargo check
cargo clippy
cargo test
```

No warnings introduced by Graf code.

## Architecture

- Language: Rust
- UI: GPUI (native rendering)
- LaTeX: Tectonic (later)
- PDF: PDFium + pdfium-render (later)
- Parsing: Tree-sitter (later)
- Primary target: macOS / Apple Silicon

## Repository Layout

```
src/
├── main.rs        # Entry point, logging init
├── app.rs         # GPUI application setup, window creation
├── workspace.rs   # Workspace shell layout (top bar, panels, status bar)
├── compiler/
│   ├── mod.rs         # Compiler module root
│   ├── engine.rs      # DocumentEngine trait, CompileRequest/Output/Error
│   ├── tectonic.rs    # Tectonic LaTeX backend implementation
│   ├── diagnostics.rs # Diagnostic data types and log parsing
│   └── controller.rs  # Compile state machine, debounce, and stale rejection
├── editor/
│   ├── mod.rs     # Editor module root
│   ├── buffer.rs  # TextBuffer with transaction-based undo/redo
│   ├── syntax.rs  # Fast lexical LaTeX syntax tokenizer
│   └── view.rs    # Multiline GPUI editor view (IME, selection, scroll, line numbers)
├── preview/
│   ├── mod.rs      # Preview module root
│   ├── renderer.rs # PdfRenderer trait and native rasterizer
│   └── view.rs     # GPUI PreviewView with page scrolling and retained output
├── project/
│   ├── mod.rs      # Project module root
│   ├── document.rs # Open document state, dirty tracking, disk persistence
│   └── tree.rs     # Project filesystem tree and directory scanner
└── ui/
    ├── mod.rs     # UI module root
    └── theme.rs   # Centralised colour constants
```
