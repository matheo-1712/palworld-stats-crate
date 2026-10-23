//! World metadata (`LevelMeta.sav`) parser.

use serde::Serialize;

use super::container::decompress;
use super::gvas::{self, ParseOptions};
use crate::Result;

/// Contents of `LevelMeta.sav`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct LevelMeta {
    /// World name as set on the server (`SaveData.WorldName`).
    pub world_name: Option<String>,
    /// In-game day count (`SaveData.InGameDay`).
    pub in_game_day: Option<i64>,
}

/// Parse a raw (compressed) `LevelMeta.sav`.
pub fn parse_level_meta(raw: &[u8]) -> Result<LevelMeta> {
    let opts = ParseOptions {
        struct_keys: &[],
        skip: &|_| false,
    };
    let (_, top, _) = gvas::parse(&decompress(raw)?, &opts)?;
    let Some(save) = top.struct_props("SaveData") else {
        return Ok(LevelMeta::default());
    };
    Ok(LevelMeta {
        world_name: save.string("WorldName").map(str::to_string),
        in_game_day: save.int("InGameDay"),
    })
}
