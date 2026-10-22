//! World save (`Level.sav`) parser: players, owned-pal counts and guilds.

use std::collections::BTreeMap;

use serde::Serialize;

use super::container::decompress;
use super::gvas::{self, ParseOptions, Props, Value};
use super::rawdata::{GroupRaw, GuildMember, decode_character, decode_group};
use crate::{GuildId, ParseWarning, PlayerUid, Result};

const WORLD: &str = ".worldSaveData";
const CHARACTER_MAP: &str = ".worldSaveData.CharacterSaveParameterMap";
const GROUP_MAP: &str = ".worldSaveData.GroupSaveDataMap";

/// A player character found in the world save.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CharacterInfo {
    pub uid: PlayerUid,
    pub name: String,
    pub level: Option<u8>,
    pub exp: Option<i64>,
    pub arena_rank_point: Option<i32>,
    pub group_id: Option<GuildId>,
}

/// A guild (group of type Guild).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GuildInfo {
    pub id: GuildId,
    pub name: String,
    pub admin: PlayerUid,
    pub members: Vec<GuildMember>,
    pub base_count: u32,
    pub base_camp_level: i32,
}

/// Everything extracted from `Level.sav`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct LevelSave {
    pub players: Vec<CharacterInfo>,
    /// Number of pals owned per player.
    pub owned_pals: BTreeMap<PlayerUid, u32>,
    pub guilds: Vec<GuildInfo>,
}

fn warn(warnings: &mut Vec<ParseWarning>, path: String, message: impl Into<String>) {
    warnings.push(ParseWarning {
        path,
        message: message.into(),
    });
}

/// Optional integer property: absent -> `None` silently; wrong type -> `None` + warning.
fn opt_int(props: &Props, name: &str, path: &str, w: &mut Vec<ParseWarning>) -> Option<i64> {
    props.get(name)?;
    let v = props.int(name);
    if v.is_none() {
        warn(w, format!("{path}.{name}"), "unexpected type");
    }
    v
}

/// Struct entries of a map-typed top-level property.
fn map_entries<'a>(top: &'a Props, name: &str, w: &mut Vec<ParseWarning>) -> &'a [(Value, Value)] {
    match top.struct_props("worldSaveData").and_then(|p| p.get(name)) {
        Some(Value::Map(m)) => m,
        Some(_) => {
            warn(w, format!("{WORLD}.{name}"), "unexpected type");
            &[]
        }
        None => &[],
    }
}

/// Parse a raw (compressed) `Level.sav`.
///
/// Synchronous; run inside `tokio::task::spawn_blocking` from async code.
pub fn parse_level_save(raw: &[u8]) -> Result<(LevelSave, Vec<ParseWarning>)> {
    parse_gvas(&decompress(raw)?)
}

fn parse_gvas(data: &[u8]) -> Result<(LevelSave, Vec<ParseWarning>)> {
    let skip = |path: &str| {
        path.strip_prefix(WORLD)
            .and_then(|rest| rest.strip_prefix('.'))
            .is_some_and(|child| {
                !child.contains('.')
                    && child != "CharacterSaveParameterMap"
                    && child != "GroupSaveDataMap"
            })
    };
    let opts = ParseOptions {
        struct_keys: &[CHARACTER_MAP],
        skip: &skip,
    };
    let (_, top, mut warnings) = gvas::parse(data, &opts)?;
    let mut level = LevelSave::default();

    for (i, (key, value)) in map_entries(&top, "CharacterSaveParameterMap", &mut warnings)
        .iter()
        .enumerate()
    {
        let path = format!("{CHARACTER_MAP}[{i}]");
        character(key, value, &path, &mut level, &mut warnings);
    }
    for (i, (_, value)) in map_entries(&top, "GroupSaveDataMap", &mut warnings)
        .iter()
        .enumerate()
    {
        let path = format!("{GROUP_MAP}[{i}]");
        group(value, &path, &mut level, &mut warnings);
    }
    Ok((level, warnings))
}

fn character(
    key: &Value,
    value: &Value,
    path: &str,
    level: &mut LevelSave,
    w: &mut Vec<ParseWarning>,
) {
    let (Value::Struct { props: kp, .. }, Value::Struct { props: vp, .. }) = (key, value) else {
        warn(w, path.to_string(), "unexpected entry type");
        return;
    };
    let Some(Value::Bytes(bytes)) = vp.get("RawData") else {
        warn(w, path.to_string(), "missing RawData bytes");
        return;
    };
    let raw = match decode_character(bytes) {
        Ok(r) => r,
        Err(e) => {
            warn(w, path.to_string(), format!("character decode: {e}"));
            return;
        }
    };
    let Some(sp) = raw.props.struct_props("SaveParameter") else {
        warn(w, path.to_string(), "missing SaveParameter");
        return;
    };
    let sp_path = format!("{path}.SaveParameter");

    let is_player = match sp.get("IsPlayer") {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => {
            warn(w, format!("{sp_path}.IsPlayer"), "unexpected type");
            false
        }
    };

    if is_player {
        let Some(uid) = kp.guid("PlayerUId").map(|g| PlayerUid::from_guid(&g)) else {
            warn(w, path.to_string(), "player without PlayerUId key");
            return;
        };
        if uid.is_nil() {
            warn(w, path.to_string(), "player with nil PlayerUId");
            return;
        }
        let name = match sp.get("NickName") {
            None => String::new(),
            Some(_) => sp
                .string("NickName")
                .map(str::to_string)
                .unwrap_or_else(|| {
                    warn(w, format!("{sp_path}.NickName"), "unexpected type");
                    String::new()
                }),
        };
        // An absent property means the engine default (Level 1, Exp 0, no arena points).
        let lvl = if sp.get("Level").is_none() {
            Some(1)
        } else {
            opt_int(sp, "Level", &sp_path, w).and_then(|v| u8::try_from(v).ok())
        };
        let exp = if sp.get("Exp").is_none() {
            Some(0)
        } else {
            opt_int(sp, "Exp", &sp_path, w)
        };
        let arena = if sp.get("ArenaRankPoint").is_none() {
            Some(0)
        } else {
            opt_int(sp, "ArenaRankPoint", &sp_path, w).and_then(|v| i32::try_from(v).ok())
        };
        let group_id = (raw.group_id != [0u8; 16]).then(|| GuildId::from_guid(&raw.group_id));
        level.players.push(CharacterInfo {
            uid,
            name,
            level: lvl,
            exp,
            arena_rank_point: arena,
            group_id,
        });
    } else if let Some(owner) = sp.guid("OwnerPlayerUId") {
        let owner = PlayerUid::from_guid(&owner);
        if !owner.is_nil() {
            *level.owned_pals.entry(owner).or_insert(0) += 1;
        }
    } else if sp.get("OwnerPlayerUId").is_some() {
        warn(w, format!("{sp_path}.OwnerPlayerUId"), "unexpected type");
    }
}

fn group(value: &Value, path: &str, level: &mut LevelSave, w: &mut Vec<ParseWarning>) {
    let Value::Struct { props, .. } = value else {
        warn(w, path.to_string(), "unexpected entry type");
        return;
    };
    let (Some(group_type), Some(Value::Bytes(bytes))) =
        (props.string("GroupType"), props.get("RawData"))
    else {
        warn(w, path.to_string(), "missing GroupType or RawData bytes");
        return;
    };
    match decode_group(group_type, bytes) {
        Ok(GroupRaw::Guild {
            id,
            name,
            base_camp_level,
            base_count,
            admin,
            members,
        }) => level.guilds.push(GuildInfo {
            id,
            name,
            admin,
            members,
            base_count,
            base_camp_level,
        }),
        Ok(GroupRaw::Other) => {}
        Err(e) => warn(w, path.to_string(), format!("group decode: {e}")),
    }
}
