//! Palworld `.sav` handling.

mod container;
pub mod gvas;
pub(crate) mod rawdata;

pub use container::decompress;
mod player;
pub use player::{PlayerRecord, RecordStats, parse_player_save};
