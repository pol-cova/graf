//! Graf's platform-independent core: compiler backends and the compile
//! controller, project files and persistence, and plain-text editing logic.
//! Front ends own rendering and input; everything they need to reason about a
//! document lives here.

pub mod compiler;
pub mod project;
pub mod text;
pub mod util;
