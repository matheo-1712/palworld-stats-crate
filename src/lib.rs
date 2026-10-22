//! Read Palworld dedicated-server stats.
//!
//! `.sav` parsing is synchronous; call it inside `tokio::task::spawn_blocking`.

pub mod error;
pub mod ids;
pub mod rest;
pub mod save;

pub use error::{Error, ParseWarning, Result};
pub use ids::*;
pub use rest::{OnlinePlayer, RestClient, ServerInfo, ServerMetrics};
