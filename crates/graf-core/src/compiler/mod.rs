pub mod controller;
pub mod diagnostics;
pub mod engine;
pub mod resolve;
pub mod tectonic;
pub mod typst;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EngineKind {
    #[default]
    Latex,
    Typst,
}
