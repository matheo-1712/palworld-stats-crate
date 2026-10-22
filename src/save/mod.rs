//! Palworld `.sav` handling.

mod container;
pub mod gvas;
pub(crate) mod rawdata;

pub use container::decompress;
