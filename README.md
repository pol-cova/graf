# graf

[![CI](https://github.com/pol-cova/graf/actions/workflows/ci.yml/badge.svg)](https://github.com/pol-cova/graf/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/pol-cova/graf?include_prereleases)](https://github.com/pol-cova/graf/releases)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

<p align="center">
  <img src="assets/icons/app-icon.png" width="144" alt="graf app icon">
</p>

graf is a native macOS editor for LaTeX and Typst. It keeps source, compilation, and PDF preview in one local workspace, so you can think about what you're writing instead of the tools.

> graf is being rebuilt as a native Swift app on top of its Rust core. The main branch currently contains the core library only, and there is no runnable app until the Swift front end lands. The last release built from the previous front end is still available below.

## Install

The last prebuilt macOS release (v1.0.0-alpha) is available from [GitHub Releases](https://github.com/pol-cova/graf/releases) and Homebrew:

```bash
brew install --cask pol-cova/tap/graf
```

## Repository layout

- `crates/graf-core`: the Rust core. It handles compiling with Tectonic and Typst, diagnostics, project files, settings, templates, crash recovery, bibliography and label indexing, outline, linting, stats, and plain-text editing logic. It has no UI dependencies.
- `crates/graf-ffi`: the bridge that exposes the core to Swift through [UniFFI](https://mozilla.github.io/uniffi-rs/). Run `./scripts/build_core_xcframework.sh` to build `target/apple/GrafCore.xcframework` and the generated Swift bindings.
- `apple/` (planned): the SwiftUI and AppKit app, using TextKit 2 for the editor and PDFKit for the preview.

## Requirements

- Stable Rust
- [Tectonic](https://tectonic-typesetting.github.io/) and [Typst](https://typst.app/) on your `PATH` to run the compiler tests against real backends. Release builds bundle both.

## Development

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
```

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. Use [GitHub Discussions](https://github.com/pol-cova/graf/discussions) for support and follow [SECURITY.md](SECURITY.md) for private vulnerability reports.

## Acknowledgements

graf's core is written in [Rust](https://www.rust-lang.org/).

- [Tectonic](https://tectonic-typesetting.github.io/) (MIT) compiles LaTeX. Bundled into release builds.
- [Typst](https://typst.app/) (Apache-2.0) compiles Typst. Bundled into release builds.
- [UniFFI](https://mozilla.github.io/uniffi-rs/) (MPL-2.0) generates the Swift bindings.
- [Serde](https://serde.rs/), [log](https://crates.io/crates/log), and [tempfile](https://crates.io/crates/tempfile). See [Cargo.lock](Cargo.lock) for the complete dependency list.

License texts for the bundled compilers live in [`bundle/licenses/`](bundle/licenses) and ship inside the app under `Resources/bin/LICENSES`.

## License

graf is available under the [Apache License 2.0](LICENSE).
