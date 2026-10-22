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
    fn missing(&mut self, name: &str) {
        self.warnings.push(ParseWarning {
            path: format!("{}.{name}", self.prefix),
            message: "missing".to_string(),
        });
    }

    fn opt<T>(&mut self, name: &str, v: Option<T>) -> Option<T> {
        if v.is_none() {
            self.missing(name);
        }
        v
    }

    fn int(&mut self, name: &str) -> Option<i32> {
        let v = self.props.int(name).and_then(|v| i32::try_from(v).ok());
        self.opt(name, v)
    }

    fn map(&mut self, name: &str) -> Option<&[(Value, Value)]> {
        let v = match self.props.get(name) {
            Some(Value::Map(m)) => Some(m.as_slice()),
            _ => None,
        };
        self.opt(name, v)
    }

    /// Sum of the non-negative `Int` values of a map.
    fn sum(&mut self, name: &str) -> Option<u64> {
        let m = self.map(name)?;
        Some(
            m.iter()
                .map(|(_, v)| match v {
                    Value::Int(i) => u64::try_from(*i).unwrap_or(0),
                    _ => 0,
                })
                .sum(),
        )
    }

    /// Count of `true` values of a map.
    fn count_true(&mut self, name: &str) -> Option<u32> {
        let m = self.map(name)?;
        let n = m
            .iter()
            .filter(|(_, v)| matches!(v, Value::Bool(true)))
            .count();
        Some(u32::try_from(n).unwrap_or(u32::MAX))
    }
}

fn array_len(v: Option<&Value>) -> Option<u32> {
    match v {
        Some(Value::Array(a)) => Some(u32::try_from(a.len()).unwrap_or(u32::MAX)),
        _ => None,
    }
}

/// Parse a raw (compressed) player `.sav` file.
///
/// Synchronous; run inside `tokio::task::spawn_blocking` from async code.
pub fn parse_player_save(raw: &[u8]) -> Result<(PlayerRecord, Vec<ParseWarning>)> {
    let data = decompress(raw)?;
    let opts = ParseOptions {
        struct_keys: &[],
        skip: &|_| false,
    };
    let (_, top, mut warnings) = gvas::parse(&data, &opts)?;
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
    let last_online = c.opt("LastOnlineDateTime", last_online);
    let platform_hint = save.string("PlayerPlatform").map(str::to_string);
    let platform_hint = c.opt("PlayerPlatform", platform_hint);
    let technology_points = c.int("TechnologyPoint");
    let boss_technology_points = c.int("bossTechnologyPoint");
    let unlocked_technologies = array_len(save.get("UnlockedRecipeTechnologyNames"));
    let unlocked_technologies = c.opt("UnlockedRecipeTechnologyNames", unlocked_technologies);
    let completed_quests = array_len(save.get("CompletedQuestArray_FullRelease"));
    let completed_quests = c.opt("CompletedQuestArray_FullRelease", completed_quests);

    let empty = Props::default();
    let rd = save.struct_props("RecordData");
    if rd.is_none() {
        c.missing("RecordData");
    }
    let mut r = Ctx {
        props: rd.unwrap_or(&empty),
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
        game_cleared: {
            let v = r.props.bool("bIsGameCleared");
            r.opt("bIsGameCleared", v)
        },
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
}
