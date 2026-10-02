# Changelog

Notable changes to graf are documented here.

## Unreleased

- Moved compiling, projects, and text logic into the `graf-core` library crate.
- Removed the GPUI front end, including the `.graf` canvas editor, ahead of the native Swift app.
- Removed the GPUI app bundle, profiling, and release scripts. Releases resume with the Swift app.
- Added the `graf-ffi` UniFFI bridge and a script that packages it as `GrafCore.xcframework` with Swift bindings.
- Moved project scaffolding from a template into `graf-core`.
- Added the native Swift app: a TextKit 2 editor with dimmed markup, prose type, and Focus mode; save and compile on pause; a PDFKit preview that keeps your place and the last good PDF; inline errors; an outline and files sidebar; a template-based new project sheet; and a launch screen that reopens your last project.
- Spell checking now skips LaTeX and Typst markup.
- Crash recovery in the Swift app: unsaved text is journaled on every pause and offered back with Restore or Discard when the project reopens. Several windows on one project no longer overwrite each other's journal entries.
- Citation, reference, environment, and command completion in the native completion list, opened by `\cite{`, `\ref{`, and `\begin{`, with labels from every file in the project and the local Zotero library.
- Settings window (⌘,): text size, Focus mode default, tab width, compile when pausing, compile delay, and Zotero.
- Files open in native macOS tabs; a file that is already open brings its tab forward instead of opening twice, and files inside an open project build through that project's root document.
- Go to… (⌘K) searches the current file's sections and every project file.
- Find, Find and Replace, Find Next, and Use Selection come from the system Find menu.
- Fixed citation completion using only the last `.bib` file when a project had several.
- The default compile delay is now 800ms, so builds run when you pause.
- Fixed Typst errors missing their file and line, so they can be shown on the line that caused them.
- Graf now requires macOS 15.

## 1.0.0-alpha - 2026-08-24

- Added the native LaTeX and Typst editing workspace.
- Added project navigation, document tabs, search, completion, and diagnostics.
- Added background compilation and retained PDF preview.
- Added persistent editor and layout settings.
- Added native file dialogs, recovery, and external-change protection.
- Added macOS application and DMG packaging.
