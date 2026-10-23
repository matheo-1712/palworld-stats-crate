//! World save (`Level.sav`) parser: players, owned-pal counts and guilds.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::container::decompress;
use super::gvas::{self, ParseOptions, Props, Value};
use super::player::ticks_to_utc;
use super::rawdata::{GroupRaw, GuildMember, decode_character, decode_group};
use crate::{GuildId, ParseWarning, PlayerUid, Result, guid_to_string};

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
    /// Status points spent per stat (`GotStatusPointList`). Keys are the game's internal
    /// (Japanese) stat names, e.g. `最大HP`.
    pub status_points: BTreeMap<String, i32>,
    /// Extra status points per stat (`GotExStatusPointList`), same keys.
    pub ex_status_points: BTreeMap<String, i32>,
}

/// One pal owned by a player.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OwnedPal {
    /// Species id (`CharacterID`), e.g. `WoolFox`. Alpha pals keep their `BOSS_` prefix.
    pub species: String,
    /// Captured alpha (species id starts with `BOSS_`).
    pub is_alpha: bool,
    /// Lucky / shiny pal (`IsRarePal`).
    pub is_lucky: bool,
    /// Level (1 when absent from the save).
    pub level: u8,
    pub exp: i64,
    pub nickname: Option<String>,
    /// `Male` / `Female`, without the enum prefix.
    pub gender: Option<String>,
    /// Condensation rank, 1 (base) to 5 (4 stars).
    pub rank: u8,
    /// Soul upgrades (`Rank_HP`, `Rank_Attack`, `Rank_Defence`, `Rank_CraftSpeed`).
    pub soul_hp: u8,
    pub soul_attack: u8,
    pub soul_defense: u8,
    pub soul_craft_speed: u8,
    /// IVs (`Talent_HP`, `Talent_Shot`, `Talent_Defense`), 0–100.
    pub talent_hp: u8,
    pub talent_attack: u8,
    pub talent_defense: u8,
    /// Passive skill ids (`PassiveSkillList`).
    pub passives: Vec<String>,
    /// Active skills learned (`MasteredWaza`).
    pub skills_learned: u32,
    /// Awakened (`bIsAwakening`).
    pub awakened: bool,
    /// Applied skin (`SkinName`).
    pub skin: Option<String>,
    /// Friendship points (`FriendshipPoint`).
    pub friendship: i32,
    /// When the pal was obtained (`OwnedTime`).
    pub obtained_at: Option<DateTime<Utc>>,
    /// Container holding the pal (`SlotId.ContainerId.ID`), as a GUID string.
    pub container_id: Option<String>,
    /// Slot in that container (`SlotId.SlotIndex`): the party order for party pals.
    pub slot_index: Option<i32>,
    /// Party, box or elsewhere. Set by `parse_world` from the owner's player file;
    /// `None` when that file is missing.
    pub location: Option<PalLocation>,
}

/// Where an owned pal is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PalLocation {
    /// In the player's party (`OtomoCharacterContainerId`).
    Party,
    /// In the player's Palbox (`PalStorageContainerId`).
    Box,
    /// Any other container, in practice a base camp.
    Other,
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
    /// Every pal owned, per player.
    pub pals: BTreeMap<PlayerUid, Vec<OwnedPal>>,
    pub guilds: Vec<GuildInfo>,
}

/// Integer property clamped into `u8`; absent -> `default`.
fn small(props: &Props, name: &str, default: u8) -> u8 {
    props.int(name).map_or(default, |v| {
        u8::try_from(v.clamp(0, 255)).unwrap_or(default)
    })
}

/// `[{StatusName, StatusPoint}]` -> map. Absent -> empty.
fn status_points(props: &Props, name: &str) -> BTreeMap<String, i32> {
    let mut out = BTreeMap::new();
    if let Some(Value::Array(a)) = props.get(name) {
        for item in a {
            if let Value::Struct { props, .. } = item
                && let Some(stat) = props.string("StatusName")
            {
                let pts = props
                    .int("StatusPoint")
                    .and_then(|v| i32::try_from(v).ok())
                    .unwrap_or(0);
                out.insert(stat.to_string(), pts);
            }
        }
    }
    out
}

fn owned_pal(sp: &Props) -> OwnedPal {
    let slot = sp.struct_props("SlotId");
    let species = sp.string("CharacterID").unwrap_or_default().to_string();
    let names = |name: &str| -> Vec<String> {
        match sp.get(name) {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|v| match v {
                    Value::Name(s) | Value::Str(s) | Value::Enum(s) => Some(s.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    };
    OwnedPal {
        is_alpha: species.to_ascii_uppercase().starts_with("BOSS_"),
        species,
        is_lucky: sp.bool("IsRarePal").unwrap_or(false),
        // Absent properties are engine defaults: level 1, rank 1, everything else 0.
        level: small(sp, "Level", 1),
        exp: sp.int("Exp").unwrap_or(0),
        nickname: sp.string("NickName").map(str::to_string),
        gender: sp
            .string("Gender")
            .map(|g| g.rsplit("::").next().unwrap_or(g).to_string()),
        rank: small(sp, "Rank", 1),
        soul_hp: small(sp, "Rank_HP", 0),
        soul_attack: small(sp, "Rank_Attack", 0),
        soul_defense: small(sp, "Rank_Defence", 0),
        soul_craft_speed: small(sp, "Rank_CraftSpeed", 0),
        talent_hp: small(sp, "Talent_HP", 0),
        talent_attack: small(sp, "Talent_Shot", 0),
        talent_defense: small(sp, "Talent_Defense", 0),
        passives: names("PassiveSkillList"),
        skills_learned: u32::try_from(names("MasteredWaza").len()).unwrap_or(u32::MAX),
        awakened: sp.bool("bIsAwakening").unwrap_or(false),
        skin: sp.string("SkinName").map(str::to_string),
        friendship: sp
            .int("FriendshipPoint")
            .and_then(|v| i32::try_from(v).ok())
            .unwrap_or(0),
        obtained_at: sp.datetime("OwnedTime").and_then(ticks_to_utc),
        container_id: slot
            .and_then(|s| s.struct_props("ContainerId"))
            .and_then(|c| c.guid("ID"))
            .map(|g| guid_to_string(&g)),
        slot_index: slot
            .and_then(|s| s.int("SlotIndex"))
            .and_then(|i| i32::try_from(i).ok()),
        location: None,
    }
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
            status_points: status_points(sp, "GotStatusPointList"),
            ex_status_points: status_points(sp, "GotExStatusPointList"),
        });
    } else if let Some(owner) = sp.guid("OwnerPlayerUId") {
        let owner = PlayerUid::from_guid(&owner);
        if !owner.is_nil() {
            *level.owned_pals.entry(owner.clone()).or_insert(0) += 1;
            level.pals.entry(owner).or_default().push(owned_pal(sp));
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
