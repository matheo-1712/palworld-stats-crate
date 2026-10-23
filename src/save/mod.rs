//! Palworld `.sav` handling.

mod container;
pub mod gvas;
pub(crate) mod rawdata;

pub use container::decompress;
mod player;
pub use player::{PlayerRecord, RecordStats, parse_player_save};
mod level;
pub use level::{CharacterInfo, GuildInfo, LevelSave, OwnedPal, PalLocation, parse_level_save};
mod meta;
pub use meta::{LevelMeta, parse_level_meta};
pub use rawdata::GuildMember;
mod world;
pub use world::{GuildRole, PalSummary, PlayerStats, WorldStats, parse_world};
