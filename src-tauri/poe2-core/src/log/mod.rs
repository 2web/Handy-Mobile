//! Reading Path of Exile 2's `Client.txt` and deriving player state from it.

#[cfg(test)]
mod acceptance;
pub mod events;
pub mod parser;
pub mod state;
pub mod tail;
pub mod zones;
