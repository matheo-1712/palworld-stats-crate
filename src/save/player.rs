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
    pub completed_quests: Option<u32>,
    pub records: RecordStats,
}

fn ticks_to_utc(ticks: u64) -> Option<DateTime<Utc>> {
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
    let completed_quests = c.array_len("CompletedQuestArray_FullRelease");

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

    let mut by_species = BTreeMap::new();
    let pal_capture_total = r.sum("PalCaptureCount");
    if let Some(Value::Map(m)) = r.props.get("PalCaptureCount") {
        for (k, v) in m {
            if let (Value::Name(k), Value::Int(v)) = (k, v) {
                by_species.insert(k.clone(), *v);
            }
        }
    }
    let records = RecordStats {
        pal_capture_total,
        pal_capture_by_species: by_species,
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
    };

    Ok((
        PlayerRecord {
            uid,
            last_online,
            platform_hint,
            technology_points,
            boss_technology_points,
            unlocked_technologies,
            completed_quests,
            records,
        },
        warnings,
    ))
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
}
