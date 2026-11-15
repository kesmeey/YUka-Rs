//! Platform-independent formats for the reconstruction of the Yuka engine.
pub mod archive;
pub mod desktop;
pub mod graphics;
pub mod resources;
pub mod runtime;
pub mod script;
pub mod sound;
pub mod vm;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
