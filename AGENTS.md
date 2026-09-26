# AGENTS.md

## Project

Graf is a local-first technical writing workspace for macOS. The main interaction is `write -> compile -> preview -> revise`. The design goal is zero cognitive load: the writer thinks about the argument, and Graf handles syntax, build state, files, references, and saving.

Graf is moving to a native Swift front end on top of a Rust core (ADR 0001). The GPUI front end has been removed. Current work is Phase 1 of `.docs/Graf-swift-plan.md`: the `graf-ffi` bridge, then the Swift shell, editor, and preview. Until the Swift app ships, there is no runnable app in the repository.

## Read before changing code

1. Read `.docs/Graf-spec.md`, `.docs/Graf-design-principles.md`, and `.docs/Graf-swift-plan.md` before major work.
2. Inspect the existing module and its callers before adding an abstraction.
3. Check the current Git diff and keep unrelated user changes intact.
4. Confirm that the work belongs to the current phase of the Swift plan or is required to fix an existing feature.
5. Never stage or commit `.docs/` or `docs/`; they are local planning material.

## Product constraints

- Keep project content in ordinary local files.
- Never upload document content without an explicit user action.
- Use native Apple UI: SwiftUI with AppKit where it matters (TextKit 2 for text, PDFKit for preview). Do not add Electron, a WebView, React, or browser UI.
- Keep logic in `graf-core`. The Swift front end renders and routes input; it does not reimplement compiling, parsing, or persistence.
- Swift owns the live text. Pass snapshots to Rust when compiling, saving, linting, or building the outline, never on every keystroke.
- Run compilation, project scans, and other expensive work off the main thread on both sides of the bridge.
- Keep compiler-specific behavior inside `crates/graf-core/src/compiler/`.
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

- Follow `.docs/Graf-design-principles.md`. Every feature must remove a question from the writer's head, not add one.
- The default screen shows the text and nothing else. Chrome appears when asked for and leaves on its own.
- Use one accent color (hyperlink blue) only for links between source and output, and red only for the broken token and its hint.
- Prefer system controls, SF Symbols, and standard macOS behavior (menus, text services, VoiceOver) over custom widgets.
- Keep controls restrained, keyboard accessible, and visible at narrow window sizes.
- Motion must explain where something came from or went. No decorative animation.

## Required checks

Run these before finishing:

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
```

Do not introduce warnings from Graf code. Swift build and test commands will be added here when the Xcode project lands.

## Architecture

- `crates/graf-core/src/compiler/`: engine interface, diagnostics, Tectonic, Typst, and compile controller
- `crates/graf-core/src/project/`: documents, project tree, persistence, settings, templates, recovery, bibliography, outline, linting, and stats
- `crates/graf-core/src/text/`: text buffer, completion, find and replace, and table formatting
- `crates/graf-core/src/util.rs`: app data paths and temporary directories
- `crates/graf-ffi/` (planned): UniFFI bridge that exposes a coarse API to Swift
- `apple/` (planned): the SwiftUI and AppKit application

## Platform notes

The primary target is Apple Silicon macOS. Keep `graf-core` free of macOS assumptions in data models so other front ends remain possible. Tectonic and Typst run as external commands. PDF display belongs to PDFKit in the Swift front end.
