# AGENTS.md

## Project

Graf is a local-first technical writing workspace for macOS. It is written in Rust and rendered with GPUI. The main interaction is `write -> compile -> preview -> revise`.

The repository is in M7, Distribution and Polish. Prefer reliability, accessibility, packaging, documentation, and performance fixes over new product areas.

## Read before changing code

1. Read `.docs/Graf-spec.md` before major work.
2. Inspect the existing module and its callers before adding an abstraction.
3. Check the current Git diff and keep unrelated user changes intact.
4. Confirm that the work belongs to M7 or is required to fix an existing feature.
5. Never stage or commit `.docs/` or `docs/`; they are local planning material.

## Product constraints

- Keep project content in ordinary local files.
- Never upload document content without an explicit user action.
- Use native GPUI rendering. Do not add Electron, a WebView, React, or browser UI.
- Run compilation, PDF rendering, project scans, and other expensive work off the UI thread.
- Keep compiler-specific behavior inside `src/compiler/`.
- Keep persistent document state separate from temporary view state.
- Preserve the last valid preview when a compile fails.
- Reject stale background results by revision.
- Do not generate fake compiler output when a backend is unavailable.

## Code rules

- Keep patches focused and reuse existing types.
- Prefer explicit errors over `unwrap`, ignored `Result` values, or silent fallback in runtime code.
- Use atomic writes for documents, settings, and recovery data.
- Add tests for parsing, persistence, revision handling, and other non-trivial core logic.
- Avoid broad warning suppressions. A narrow `allow` needs a reason.
- Do not add a dependency until the standard library and current dependencies have been considered.
- Update the spec only when the architecture or planned behavior deliberately changes.
- Add an ADR under `docs/adr/` only for a lasting architectural decision.

## UI rules

- Match a compact native editor, with Zed as the main visual reference.
- Use the shared colors in `src/ui/theme.rs`; do not hardcode colors in views.
- Prefer text labels or monochrome symbols over emoji.
- Keep controls restrained, keyboard accessible, and visible at narrow window sizes.
- Avoid gradients, glass effects, oversized controls, and decorative animation.

## Required checks

Run these before finishing:

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
```

Do not introduce warnings from Graf code. The known `block` future-compatibility warning comes from an upstream GPUI dependency.

## Architecture

- `src/app.rs`: GPUI application and native window setup
- `src/workspace/`: shell, tabs, panels, commands, modals, and task coordination
- `src/editor/`: text buffer, input, rendering, syntax, completion, and search
- `src/project/`: documents, project tree, persistence, settings, recovery, and references
- `src/compiler/`: engine interface, diagnostics, Tectonic, Typst, and compile controller
- `src/preview/`: PDF rasterization and preview state
- `src/canvas/`: `.graf` scene model, editor, history, and exporters
- `src/ai/`: provider boundary, operations, and reviewed diffs
- `src/plugins/`: plugin manifests and command dispatch
- `src/ui/`: shared theme values

## Platform notes

The primary target is Apple Silicon macOS. Avoid macOS assumptions in core data models, but platform-specific UI and PDF code may live behind internal interfaces. Tectonic and Typst currently run as external commands. PDF rasterization currently uses the macOS `sips` tool.
