# Contributing to graf

Thanks for helping improve graf.

## Before you start

For substantial changes, open an issue first so the approach can be discussed. Bug fixes and focused documentation improvements can go directly to a pull request.

## Development setup

You need Xcode 16 or later and stable Rust with the `aarch64-apple-darwin` target.

```bash
git clone https://github.com/pol-cova/graf.git
cd graf
./scripts/build_core_xcframework.sh
swift build --package-path apple
```

## Making changes

- Keep changes focused and avoid unrelated refactors.
- Preserve local-first behavior and native macOS UI (SwiftUI, AppKit, TextKit, PDFKit).
- Keep logic in `graf-core`. The Swift app renders and routes input.
- Do not block the main thread with compilation, file scanning, or rendering work.
- Add tests for non-trivial logic.
- Update public documentation when behavior changes.

Run the full validation suite before submitting:

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
swift test --package-path apple
```

## Pull requests

Describe the problem, the chosen solution, and how you tested it. Include screenshots for interface changes. A maintainer may ask you to split a large pull request before review.

By contributing, you agree that your contribution is licensed under the Apache License 2.0.
