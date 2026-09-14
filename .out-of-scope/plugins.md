# Plugins (formatter/dispatch subsystem)

**Declined for v1** (2026-09, decided by maintainer). Original request: plugin host with per-language formatter dispatch (`dispatch_format`, `PluginManifest`, `PluginHost`). The implementation existed but was unreachable in production (test-only skeleton, flagged in the 2026-09 audit).

## Why declined

Graf 1.0's positioning is "a great local-first editor + compile/preview". The plugin contract adds 'the guardrail of a public manifest API' whose adoption/maintenance pricing is not yet acceptable pre-v1 (see the maintainer statement in issue #42).

## If it comes back

- The architecture pins it should sit behind: `CommandId` (an `Extensions` arm returns easily), `DocumentKind` (dispatch on language), and `workspace/state.rs` (one place to re-hook the host).
- The old implementation lives in git history at `src/plugins/` before commit `e8abe2f` (PR #64).
- Options on the table include: wire formatter dispatch through `syn. Internal`/built-in `format` before the plugins opens anything beyond that.
