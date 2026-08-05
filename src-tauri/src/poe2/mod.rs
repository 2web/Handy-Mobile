//! Tauri-side wiring for the Path of Exile 2 section.
//!
//! The logic lives in the `poe2-core` crate; what belongs here is the glue that
//! needs an `AppHandle`.

pub mod commands;
pub mod watcher;
