//! Path of Exile 2 companion logic, free of any Tauri dependency.
//!
//! Its own crate rather than a module of `handy`: linking `handy`'s test binary
//! pulls in native Vulkan/ggml libraries through `transcribe-cpp`, and on Windows
//! that binary cannot start at all. Here the tests build and run in seconds, and
//! the "no I/O in the parser" rule the design asks for is enforced by the crate
//! boundary rather than by discipline.

pub mod items;
pub mod store;
