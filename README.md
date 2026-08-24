# Graf

Graf is a native macOS workspace for writing and previewing technical documents. It combines a source editor, project files, local compilation, PDF preview, references, diagrams, and writing tools in one GPUI application.

Graf is early alpha software. The editor works, but file handling, compiler compatibility, packaging, and accessibility still need release testing.

## What works

- Multiline LaTeX and Typst editing with undo, redo, selection, search, and syntax highlighting
- Project tree, document tabs, outline navigation, and dirty-state tracking
- Background compilation with debounce and stale-result rejection
- Tectonic and Typst command-line backends
- Native macOS PDF preview
- Bibliography, citation, label, and reference indexing
- Editable `.graf` diagrams with SVG and TikZ export
- Command palette, quick open, diagnostics, settings, and crash recovery

All project content stays in ordinary local files. Graf does not require an account or upload documents by default.

## Requirements

- macOS on Apple Silicon
- Latest stable Rust toolchain
- Tectonic available on `PATH` for LaTeX compilation
- Typst available on `PATH` for Typst compilation

## Run locally

```bash
git clone git@github.com:pol-cova/graf.git
cd graf
cargo run
```

Graf opens the current directory as the project workspace.

## Development checks

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Debugging and profiling

Press `Command-Shift-D` to cycle GPUI's performance overlay through hidden, frame-time, and detailed modes. Detailed mode shows the current draw time, slow-frame percentiles, maximum draw time, and frame count.

Capture an Instruments Time Profiler trace with:

```bash
./scripts/profile_app.sh
```

The script builds the `profiling` Cargo profile with release optimizations and debug symbols, then writes a timestamped `.trace` bundle.

## Build the macOS app

```bash
./scripts/build_app.sh
```

The script creates `target/release/bundle/Graf.app` and uses `hdiutil` to create a DMG when available. Signing and notarization are not automated yet.

## Project structure

```text
src/
├── ai/          AI provider and reviewed edit operations
├── canvas/      Native diagram scene, tools, and exporters
├── compiler/    Tectonic and Typst engines plus compile scheduling
├── editor/      Text buffer, syntax, completion, search, and GPUI view
├── plugins/     Plugin manifests and command dispatch
├── preview/     PDF rasterization and preview view
├── project/     Documents, files, settings, recovery, and references
├── ui/          Theme definitions
└── workspace/   Application layout, commands, panels, and modals
```

The product and milestone plan lives in [`.docs/Graf-spec.md`](.docs/Graf-spec.md).

## License

Graf is licensed under the Apache License 2.0. See [`LICENSE`](LICENSE).
