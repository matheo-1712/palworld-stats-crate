//! Read Palworld dedicated-server stats.
//!
//! `.sav` parsing is synchronous; call it inside `tokio::task::spawn_blocking`.

pub mod error;
pub mod ids;
pub mod logs;
pub mod names;
pub mod presence;
pub mod rest;
pub mod save;

pub use error::{Error, ParseWarning, Result};
pub use ids::*;
pub use logs::{LogEvent, LogEventKind, parse_log_line};
pub use names::{Lang, pal_name, pal_species_name, passive_name};
pub use presence::{PresenceEvent, diff_presence};
pub use rest::{OnlinePlayer, RestClient, ServerInfo, ServerMetrics};
pub use save::{
    GuildInfo, GuildMember, GuildRole, LevelMeta, OwnedPal, PalLocation, PalSummary, PlayerRecord,
    PlayerStats, RecordStats, WorldStats, parse_level_meta, parse_world,
};

#[cfg(feature = "pocketbase")]
pub mod pocketbase;
