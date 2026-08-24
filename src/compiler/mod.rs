#[allow(dead_code)]
pub mod controller;
#[allow(dead_code)]
pub mod diagnostics;
#[allow(dead_code)]
pub mod engine;
#[allow(dead_code)]
pub mod tectonic;
#[allow(dead_code)]
pub mod typst;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EngineKind {
    #[default]
    Latex,
    Typst,
}
