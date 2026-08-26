# graf

[![CI](https://github.com/pol-cova/graf/actions/workflows/ci.yml/badge.svg)](https://github.com/pol-cova/graf/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/pol-cova/graf?include_prereleases)](https://github.com/pol-cova/graf/releases)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

graf is a native editor for LaTeX and Typst. It keeps source, compilation, and PDF preview in one local workspace.

> graf is alpha software. Keep important documents under version control or backed up.

## Features

- Native GPUI editor with syntax highlighting, search, completion, and undo history
- Project tree, tabs, document outline, diagnostics, and quick open
- Background compilation with Tectonic and Typst
- PDF preview beside the source
- Citation, bibliography, label, and reference indexing
- Local files with crash recovery

## Install

Prebuilt macOS releases are available from [GitHub Releases](https://github.com/pol-cova/graf/releases) and Homebrew:

```bash
brew install --cask pol-cova/tap/graf
```

Linux users can build from source. Release automation also publishes a Linux archive.

## Requirements

- Stable Rust for source builds
- [Tectonic](https://tectonic-typesetting.github.io/) for LaTeX
- [Typst](https://typst.app/) for Typst documents
- `sips` on macOS or `pdftoppm` on other platforms for PDF preview

## Build from source

Install stable Rust, then run:

```bash
git clone https://github.com/pol-cova/graf.git
cd graf
cargo run
```

graf opens the current directory as the workspace. Build the application bundle and DMG with:

```bash
./scripts/build_app.sh
```

## Development

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
```

Press `Command-Shift-D` to show GPUI frame timings. Run `./scripts/profile_app.sh` to record an Instruments Time Profiler trace.

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. Use [GitHub Discussions](https://github.com/pol-cova/graf/discussions) for support and follow [SECURITY.md](SECURITY.md) for private vulnerability reports.

## Acknowledgements

graf is written in [Rust](https://www.rust-lang.org/) and uses [GPUI](https://www.gpui.rs/) from the [Zed](https://github.com/zed-industries/zed) project for its native interface. Its Rust dependencies include [Serde](https://serde.rs/), [unicode-segmentation](https://crates.io/crates/unicode-segmentation), [log](https://crates.io/crates/log), [env_logger](https://crates.io/crates/env_logger), and [tempfile](https://crates.io/crates/tempfile). See [Cargo.toml](Cargo.toml) and [Cargo.lock](Cargo.lock) for the complete dependency list and pinned versions.

graf also works with separately installed tools: [Tectonic](https://tectonic-typesetting.github.io/) and [Typst](https://typst.app/) compile documents, while macOS `sips` and Poppler's [`pdftoppm`](https://poppler.freedesktop.org/) rasterize PDF previews.

## License

graf is available under the [Apache License 2.0](LICENSE).
