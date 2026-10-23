//! Player save (`Players/<UID>.sav`) parser.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::container::decompress;
use super::gvas::{self, ParseOptions, Props, Value};
use crate::{Error, ParseWarning, PlayerUid, Result};

/// Ticks (100 ns since 0001-01-01) at the Unix epoch.
const UNIX_EPOCH_TICKS: u64 = 621_355_968_000_000_000;

/// Aggregated `RecordData` statistics. `None` means the key was absent.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RecordStats {
    pub pal_capture_total: Option<u64>,
    pub pal_capture_by_species: BTreeMap<String, i32>,
    pub species_captured: Option<i32>,
    pub paldeck_unlocked: Option<u32>,
    pub normal_boss_defeated: Option<u32>,
    pub tower_boss_defeated: Option<u32>,
    pub tower_boss_defeat_total: Option<u64>,
    pub raid_boss_defeat_total: Option<u64>,
    pub predator_defeated: Option<i32>,
    pub pal_butcher_total: Option<u64>,
    pub pal_rankup_total: Option<u64>,
    pub mutation_count: Option<i32>,
    pub awakening_count: Option<i32>,
    pub camp_conquered: Option<i32>,
    pub normal_dungeon_clear: Option<i32>,
    pub fixed_dungeon_clear: Option<i32>,
    pub oilrig_clear: Option<i32>,
    pub found_treasure: Option<i32>,
    pub relic_possess: Option<i32>,
    pub arena_solo_clear_total: Option<u64>,
    pub fishing_total: Option<u64>,
    pub craft_item_total: Option<u64>,
    pub fast_travel_unlocked: Option<u32>,
    pub game_cleared: Option<bool>,

    // Exploration and collection (count of `true` flags, or sums).
    /// Map areas discovered (`FindAreaFlagMap`).
    pub areas_discovered: Option<u32>,
    /// Area barriers unlocked (`AreaBarrierUnlockFlags`).
    pub area_barriers_unlocked: Option<u32>,
    /// World map pieces unlocked (`UnlockedWorldMapFlags`).
    pub world_maps_unlocked: Option<u32>,
    /// Journal notes picked up (`NoteObtainForInstanceFlag`).
    pub notes_found: Option<u32>,
    /// Relics picked up on the map (`RelicObtainForInstanceFlag`).
    pub relics_picked_up: Option<u32>,
    /// Unique world items picked up (`ItemPickupObtainForInstanceFlag`).
    pub items_picked_up: Option<u32>,
    /// Conversations with NPCs (sum of `NPCTalkCountMap`).
    pub npc_talks_total: Option<u64>,
    /// NPC achievement rewards obtained (`NPCAchivementRewardFlag`).
    pub npc_achievements: Option<u32>,
    /// Emotes learned from NPCs (`CompletedEmoteNPCIDArray`).
    pub emotes_learned: Option<u32>,
    /// Special bosses defeated, total (sum of `SpecificBossDefeatFlag`).
    pub specific_boss_defeat_total: Option<u64>,

    // Per-key details behind the totals above.
    /// `PalCaptureBonusCount`: captures that granted a bonus, per species.
    pub capture_bonus_by_species: BTreeMap<String, i32>,
    /// `PalButcherCount`: pals butchered, per species.
    pub butcher_by_species: BTreeMap<String, i32>,
    /// `PalRankupCount`: rank-ups, per reached rank ("2".."5").
    pub rankup_by_rank: BTreeMap<String, i32>,
    /// `FishingCountMap`: fish caught, per fish.
    pub fishing_by_species: BTreeMap<String, i32>,
    /// `CraftItemCount`: items produced, per item.
    pub craft_by_item: BTreeMap<String, i32>,
    /// `TowerBossDefeatCount`: victories per tower boss.
    pub tower_boss_defeats: BTreeMap<String, i32>,
    /// `RaidBossDefeatCount`: victories per raid boss.
    pub raid_boss_defeats: BTreeMap<String, i32>,
    /// `ArenaSoloClearCount`: solo arena victories per rank.
    pub arena_solo_clears: BTreeMap<String, i32>,
    /// `RelicPossessNumMap`: relics owned per type (`CapturePower`, …).
    pub relics_by_type: BTreeMap<String, i32>,
    /// `SpecificBossDefeatFlag`: victories per special boss.
    pub specific_boss_defeats: BTreeMap<String, i32>,
}

/// One player's save data.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlayerRecord {
    pub uid: PlayerUid,
    pub last_online: Option<DateTime<Utc>>,
    pub platform_hint: Option<String>,
    pub technology_points: Option<i32>,
    pub boss_technology_points: Option<i32>,
    pub unlocked_technologies: Option<u32>,
    /// Names of the unlocked recipes (`UnlockedRecipeTechnologyNames`).
    pub unlocked_technology_names: Vec<String>,
    pub completed_quests: Option<u32>,
    /// Quests in progress (`OrderedQuestArray_FullRelease`).
    pub quests_in_progress: Option<u32>,
    /// Skins owned, by skin name (`SkinInventoryInfo.InGameData`).
    pub skins: BTreeMap<String, i32>,
    pub records: RecordStats,
}

pub(super) fn ticks_to_utc(ticks: u64) -> Option<DateTime<Utc>> {
    let secs = ticks.checked_sub(UNIX_EPOCH_TICKS)? / 10_000_000;
    DateTime::from_timestamp(i64::try_from(secs).ok()?, 0)
}

struct Ctx<'a> {
    props: &'a Props,
    prefix: &'static str,
    warnings: &'a mut Vec<ParseWarning>,
}

impl Ctx<'_> {
    fn bad(&mut self, name: &str, message: &str) {
        self.warnings.push(ParseWarning {
            path: format!("{}.{name}", self.prefix),
            message: message.to_string(),
        });
    }

    /// Absent -> default 0; present but not an in-range integer -> None + warning.
    fn int(&mut self, name: &str) -> Option<i32> {
        if self.props.get(name).is_none() {
            return Some(0);
        }
        let v = self.props.int(name).and_then(|v| i32::try_from(v).ok());
        if v.is_none() {
            self.bad(name, "unexpected type");
        }
        v
    }

    /// Absent -> `Some(false)`.
    fn flag(&mut self, name: &str) -> Option<bool> {
        if self.props.get(name).is_none() {
            return Some(false);
        }
        let v = self.props.bool(name);
        if v.is_none() {
            self.bad(name, "unexpected type");
        }
        v
    }

    /// Absent -> empty; present but not a map -> None + warning.
    fn map(&mut self, name: &str) -> Option<&[(Value, Value)]> {
        match self.props.get(name) {
            None => Some(&[]),
            Some(Value::Map(m)) => Some(m.as_slice()),
            Some(_) => {
                self.bad(name, "unexpected type");
                None
            }
        }
    }

    /// Sum of the non-negative `Int` values of a map.
    fn sum(&mut self, name: &str) -> Option<u64> {
        let m = self.map(name)?;
        let mut total = 0u64;
        let mut ok = true;
        for (_, v) in m {
            match v {
                Value::Int(i) => total += u64::try_from(*i).unwrap_or(0),
                _ => ok = false,
            }
        }
        if !ok {
            self.bad(name, "unexpected map value type");
            return None;
        }
        Some(total)
    }

    /// Count of `true` values of a map.
    fn count_true(&mut self, name: &str) -> Option<u32> {
        let m = self.map(name)?;
        let mut n = 0usize;
        let mut ok = true;
        for (_, v) in m {
            match v {
                Value::Bool(b) => n += usize::from(*b),
                _ => ok = false,
            }
        }
        if !ok {
            self.bad(name, "unexpected map value type");
            return None;
        }
        Some(u32::try_from(n).unwrap_or(u32::MAX))
    }

    /// Map with `Int` values, keyed by name. Enum keys lose their `EType::` prefix.
    /// Absent -> empty; bad shape -> empty + warning.
    fn int_map(&mut self, name: &str) -> BTreeMap<String, i32> {
        let Some(m) = self.map(name) else {
            return BTreeMap::new();
        };
        let mut out = BTreeMap::new();
        let mut ok = true;
        for (k, v) in m {
            let key = match k {
                Value::Name(s) | Value::Str(s) => s.as_str(),
                Value::Enum(s) => s.rsplit("::").next().unwrap_or(s),
                _ => {
                    ok = false;
                    continue;
                }
            };
            match v {
                Value::Int(i) => {
                    out.insert(key.to_string(), *i);
                }
                _ => ok = false,
            }
        }
        if !ok {
            self.bad(name, "unexpected map entry type");
        }
        out
    }

    /// Absent -> 0; present but not an array -> None + warning.
    fn array_len(&mut self, name: &str) -> Option<u32> {
        match self.props.get(name) {
            None => Some(0),
            Some(Value::Array(a)) => Some(u32::try_from(a.len()).unwrap_or(u32::MAX)),
            Some(_) => {
                self.bad(name, "unexpected type");
                None
            }
        }
    }
}

/// Parse a raw (compressed) player `.sav` file.
///
/// Synchronous; run inside `tokio::task::spawn_blocking` from async code.
pub fn parse_player_save(raw: &[u8]) -> Result<(PlayerRecord, Vec<ParseWarning>)> {
    parse_gvas(&decompress(raw)?)
}

fn parse_gvas(data: &[u8]) -> Result<(PlayerRecord, Vec<ParseWarning>)> {
    let opts = ParseOptions {
        struct_keys: &[],
        skip: &|_| false,
    };
    let (_, top, mut warnings) = gvas::parse(data, &opts)?;
    let save = top.struct_props("SaveData").ok_or_else(|| Error::Gvas {
        offset: 0,
        msg: "missing SaveData".to_string(),
    })?;
    let uid = save
        .guid("PlayerUId")
        .map(|g| PlayerUid::from_guid(&g))
        .ok_or_else(|| Error::Gvas {
            offset: 0,
            msg: "missing SaveData.PlayerUId".to_string(),
        })?;

    let mut c = Ctx {
        props: save,
        prefix: ".SaveData",
        warnings: &mut warnings,
    };
    let last_online = save.datetime("LastOnlineDateTime").and_then(ticks_to_utc);
    let platform_hint = save.string("PlayerPlatform").map(str::to_string);
    let technology_points = c.int("TechnologyPoint");
    let boss_technology_points = c.int("bossTechnologyPoint");
    let unlocked_technologies = c.array_len("UnlockedRecipeTechnologyNames");
    let unlocked_technology_names = match save.get("UnlockedRecipeTechnologyNames") {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|v| match v {
                Value::Name(s) | Value::Str(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    let completed_quests = c.array_len("CompletedQuestArray_FullRelease");
    let quests_in_progress = c.array_len("OrderedQuestArray_FullRelease");
    let skins = skins(save, &mut c);

    let empty = Props::default();
    let rd = match save.get("RecordData") {
        None => &empty,
        Some(Value::Struct { props, .. }) => props,
        Some(_) => {
            c.bad("RecordData", "unexpected type");
            &empty
        }
    };
    let mut r = Ctx {
        props: rd,
        prefix: ".SaveData.RecordData",
        warnings: &mut warnings,
    };

    let pal_capture_total = r.sum("PalCaptureCount");
    let records = RecordStats {
        pal_capture_total,
        pal_capture_by_species: r.int_map("PalCaptureCount"),
        species_captured: r.int("TribeCaptureCount"),
        paldeck_unlocked: r.count_true("PaldeckUnlockFlag"),
        normal_boss_defeated: r.count_true("NormalBossDefeatFlag"),
        tower_boss_defeated: r.count_true("TowerBossDefeatFlag"),
        tower_boss_defeat_total: r.sum("TowerBossDefeatCount"),
        raid_boss_defeat_total: r.sum("RaidBossDefeatCount"),
        predator_defeated: r.int("PredatorDefeatCount"),
        pal_butcher_total: r.sum("PalButcherCount"),
        pal_rankup_total: r.sum("PalRankupCount"),
        mutation_count: r.int("MutationCount"),
        awakening_count: r.int("AwakeningCount"),
        camp_conquered: r.int("CampConqueredCount"),
        normal_dungeon_clear: r.int("NormalDungeonClearCount"),
        fixed_dungeon_clear: r.int("FixedDungeonClearCount"),
        oilrig_clear: r.int("OilrigClearCount"),
        found_treasure: r.int("FoundTreasureCount"),
        relic_possess: r.int("RelicPossessNum"),
        arena_solo_clear_total: r.sum("ArenaSoloClearCount"),
        fishing_total: r.sum("FishingCountMap"),
        craft_item_total: r.sum("CraftItemCount"),
        fast_travel_unlocked: r.count_true("FastTravelPointUnlockFlag"),
        game_cleared: r.flag("bIsGameCleared"),
        areas_discovered: r.count_true("FindAreaFlagMap"),
        area_barriers_unlocked: r.count_true("AreaBarrierUnlockFlags"),
        world_maps_unlocked: r.count_true("UnlockedWorldMapFlags"),
        notes_found: r.count_true("NoteObtainForInstanceFlag"),
        relics_picked_up: r.count_true("RelicObtainForInstanceFlag"),
        items_picked_up: r.count_true("ItemPickupObtainForInstanceFlag"),
        npc_talks_total: r.sum("NPCTalkCountMap"),
        npc_achievements: r.count_true("NPCAchivementRewardFlag"),
        emotes_learned: r.array_len("CompletedEmoteNPCIDArray"),
        specific_boss_defeat_total: r.sum("SpecificBossDefeatFlag"),
        capture_bonus_by_species: r.int_map("PalCaptureBonusCount"),
        butcher_by_species: r.int_map("PalButcherCount"),
        rankup_by_rank: r.int_map("PalRankupCount"),
        fishing_by_species: r.int_map("FishingCountMap"),
        craft_by_item: r.int_map("CraftItemCount"),
        tower_boss_defeats: r.int_map("TowerBossDefeatCount"),
        raid_boss_defeats: r.int_map("RaidBossDefeatCount"),
        arena_solo_clears: r.int_map("ArenaSoloClearCount"),
        relics_by_type: r.int_map("RelicPossessNumMap"),
        specific_boss_defeats: r.int_map("SpecificBossDefeatFlag"),
    };

    Ok((
        PlayerRecord {
            uid,
            last_online,
            platform_hint,
            technology_points,
            boss_technology_points,
            unlocked_technologies,
            unlocked_technology_names,
            completed_quests,
            quests_in_progress,
            skins,
            records,
        },
        warnings,
    ))
}

/// `SkinInventoryInfo.InGameData`: array of `{SkinName, Num}`. Absent -> empty.
fn skins(save: &Props, c: &mut Ctx) -> BTreeMap<String, i32> {
    let mut out = BTreeMap::new();
    let Some(info) = save.get("SkinInventoryInfo") else {
        return out;
    };
    let items = match info {
        Value::Struct { props, .. } => match props.get("InGameData") {
            None => return out,
            Some(Value::Array(a)) => a,
            Some(_) => {
                c.bad("SkinInventoryInfo.InGameData", "unexpected type");
                return out;
            }
        },
        _ => {
            c.bad("SkinInventoryInfo", "unexpected type");
            return out;
        }
    };
    for item in items {
        if let Value::Struct { props, .. } = item
            && let Some(name) = props.string("SkinName")
        {
            let num = props
                .int("Num")
                .and_then(|n| i32::try_from(n).ok())
                .unwrap_or(1);
            out.insert(name.to_string(), num);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_conversion() {
        assert_eq!(ticks_to_utc(UNIX_EPOCH_TICKS).unwrap().timestamp(), 0);
        assert_eq!(
            ticks_to_utc(UNIX_EPOCH_TICKS + 10_000_000 * 86_400)
                .unwrap()
                .timestamp(),
            86_400
        );
        assert_eq!(ticks_to_utc(UNIX_EPOCH_TICKS - 1), None);
        assert_eq!(ticks_to_utc(0), None);
    }

    fn save_doc(extra: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
        use crate::save::gvas::build::*;
        let mut inner = Vec::new();
        let mut gh = fs("Guid");
        gh.extend_from_slice(&[0u8; 16]);
        tag(&mut inner, "PlayerUId", "StructProperty", &gh, &[0x11; 16]);
        extra(&mut inner);
        none(&mut inner);
        let mut sh = fs("PalWorldPlayerSaveData");
        sh.extend_from_slice(&[0u8; 16]);
        let mut b = header_bytes();
        tag(&mut b, "SaveData", "StructProperty", &sh, &inner);
        none(&mut b);
        b.extend_from_slice(&[0, 0, 0, 0]);
        b
    }

    #[test]
    fn absent_properties_default_without_warnings() {
        let (p, w) = parse_gvas(&save_doc(|_| {})).unwrap();
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(p.records.species_captured, Some(0));
        assert_eq!(p.records.pal_capture_total, Some(0));
        assert_eq!(p.records.paldeck_unlocked, Some(0));
        assert_eq!(p.records.game_cleared, Some(false));
        assert_eq!(p.technology_points, Some(0));
        assert_eq!(p.completed_quests, Some(0));
        assert_eq!(p.last_online, None);
        assert_eq!(p.platform_hint, None);
    }

    #[test]
    fn wrong_type_gives_none_and_warning() {
        use crate::save::gvas::build::*;
        let doc = save_doc(|inner| {
            let mut rd = Vec::new();
            tag(&mut rd, "TribeCaptureCount", "StrProperty", &[], &fs("x"));
            none(&mut rd);
            let mut sh = fs("PalLocalRecordData");
            sh.extend_from_slice(&[0u8; 16]);
            tag(inner, "RecordData", "StructProperty", &sh, &rd);
        });
        let (p, w) = parse_gvas(&doc).unwrap();
        assert_eq!(p.records.species_captured, None);
        assert_eq!(w.len(), 1, "{w:?}");
        assert_eq!(w[0].path, ".SaveData.RecordData.TribeCaptureCount");
    }

    #[test]
    fn int_map_strips_enum_prefix() {
        use crate::save::gvas::build::*;
        let doc = save_doc(|inner| {
            // Map<EnumProperty, IntProperty> with one entry.
            let mut payload = vec![0, 0, 0, 0, 1, 0, 0, 0];
            payload.extend_from_slice(&fs("EPalRelicType::CapturePower"));
            payload.extend_from_slice(&3i32.to_le_bytes());
            let mut hdr = fs("EnumProperty");
            hdr.extend_from_slice(&fs("IntProperty"));
            let mut rd = Vec::new();
            tag(&mut rd, "RelicPossessNumMap", "MapProperty", &hdr, &payload);
            none(&mut rd);
            let mut sh = fs("PalLocalRecordData");
            sh.extend_from_slice(&[0u8; 16]);
            tag(inner, "RecordData", "StructProperty", &sh, &rd);
        });
        let (p, w) = parse_gvas(&doc).unwrap();
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(p.records.relics_by_type.get("CapturePower"), Some(&3));
    }
}
