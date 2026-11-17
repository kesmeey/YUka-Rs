//! Platform-independent formats for the reconstruction of the Yuka engine.
#[cfg(target_os = "android")]
mod android;
#[cfg(any(target_os = "android", test))]
mod android_bundle;
pub mod archive;
pub mod desktop;
pub mod graphics;
pub mod native_save;
pub mod resources;
pub mod runtime;
pub mod script;
#[cfg(not(target_os = "android"))]
pub mod sound;
#[cfg(any(target_os = "android", test))]
pub mod sound_android;
#[cfg(target_os = "android")]
pub use sound_android as sound;
pub mod vm;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
