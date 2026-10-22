//! Read Palworld dedicated-server stats.
//!
//! `.sav` parsing is synchronous; call it inside `tokio::task::spawn_blocking`.

pub mod error;
pub mod ids;

pub use error::{Error, ParseWarning, Result};
pub use ids::*;
