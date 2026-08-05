//! What the player's captured gear adds up to.
//!
//! Pure functions over items already loaded from the store: nothing here reads
//! the database or the filesystem.

#[cfg(test)]
mod acceptance;
pub mod resistances;
pub mod slots;
pub mod summary;
