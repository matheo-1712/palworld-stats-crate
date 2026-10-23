//! `serde` structs for the proposed PocketBase collections (feature `pocketbase`).
//!
//! Conventions: ids and relations are `String`, dates are RFC 3339 strings. Values that can
//! be unknown are `Option` and serialized as `null`.

use std::collections::BTreeMap;

use crate::save::{GuildInfo, OwnedPal, PlayerStats, RecordStats};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Collection `palworld_characters`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PalworldCharacter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub server: String,
    /// Relation to `players`; empty until the `PlayerUid` <-> `PlatformId` link is known.
    pub player: Option<String>,
    pub player_uid: String,
    pub name: String,
    pub level: Option<u8>,
    pub exp: Option<i64>,
    pub guild: Option<String>,
    pub last_online: Option<String>,
    pub first_seen: String,
}

/// Collection `palworld_stats` (current values, 1:1 with a character).
/// Per-key details (`*_by_*`, `skins`, `status_points`, ...) are PocketBase `json` fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PalworldStats {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub character: String,
    pub server: String,
    pub playtime_seconds: u64,
    pub updated_at: String,
    pub level: Option<u8>,
    pub exp: Option<i64>,
    pub owned_pals: u32,
    pub arena_rank_point: Option<i32>,
    pub technology_points: Option<i32>,
    pub boss_technology_points: Option<i32>,
    pub unlocked_technologies: Option<u32>,
    pub completed_quests: Option<u32>,
    pub pal_capture_total: Option<u64>,
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
    pub areas_discovered: Option<u32>,
    pub area_barriers_unlocked: Option<u32>,
    pub world_maps_unlocked: Option<u32>,
    pub notes_found: Option<u32>,
    pub relics_picked_up: Option<u32>,
    pub items_picked_up: Option<u32>,
    pub npc_talks_total: Option<u64>,
    pub npc_achievements: Option<u32>,
    pub emotes_learned: Option<u32>,
    pub specific_boss_defeat_total: Option<u64>,
    pub quests_in_progress: Option<u32>,
    pub skins_owned: Option<u32>,
    // Pal collection summary (from Level.sav).
    pub pals_unique_species: u32,
    pub pals_lucky: u32,
    pub pals_alpha: u32,
    pub pals_max_level: u8,
    pub pals_average_level: Option<f64>,
    pub pals_max_rank: u32,
    pub pals_awakened: u32,
    pub pals_perfect_talents: u32,
    pub pals_four_passives: u32,
    // Details (json fields). Empty maps when the player file is missing.
    pub capture_by_species: BTreeMap<String, i32>,
    pub capture_bonus_by_species: BTreeMap<String, i32>,
    pub butcher_by_species: BTreeMap<String, i32>,
    pub rankup_by_rank: BTreeMap<String, i32>,
    pub fishing_by_species: BTreeMap<String, i32>,
    pub craft_by_item: BTreeMap<String, i32>,
    pub tower_boss_defeats: BTreeMap<String, i32>,
    pub raid_boss_defeats: BTreeMap<String, i32>,
    pub arena_solo_clears: BTreeMap<String, i32>,
    pub relics_by_type: BTreeMap<String, i32>,
    pub specific_boss_defeats: BTreeMap<String, i32>,
    pub skins: BTreeMap<String, i32>,
    pub status_points: BTreeMap<String, i32>,
    pub ex_status_points: BTreeMap<String, i32>,
}

impl PalworldStats {
    /// Builds the current-values row. Record-derived fields are `None` when the player
    /// file was not available.
    pub fn from_player(
        server: &str,
        character: &str,
        p: &PlayerStats,
        playtime_seconds: u64,
        now: DateTime<Utc>,
    ) -> Self {
        let rec = p.record.as_ref();
        let r = rec.map(|r| &r.records);
        Self {
            id: None,
            character: character.to_string(),
            server: server.to_string(),
            playtime_seconds,
            updated_at: now.to_rfc3339(),
            level: p.level,
            exp: p.exp,
            owned_pals: p.owned_pals,
            arena_rank_point: p.arena_rank_point,
            technology_points: rec.and_then(|r| r.technology_points),
            boss_technology_points: rec.and_then(|r| r.boss_technology_points),
            unlocked_technologies: rec.and_then(|r| r.unlocked_technologies),
            completed_quests: rec.and_then(|r| r.completed_quests),
            pal_capture_total: r.and_then(|r| r.pal_capture_total),
            species_captured: r.and_then(|r| r.species_captured),
            paldeck_unlocked: r.and_then(|r| r.paldeck_unlocked),
            normal_boss_defeated: r.and_then(|r| r.normal_boss_defeated),
            tower_boss_defeated: r.and_then(|r| r.tower_boss_defeated),
            tower_boss_defeat_total: r.and_then(|r| r.tower_boss_defeat_total),
            raid_boss_defeat_total: r.and_then(|r| r.raid_boss_defeat_total),
            predator_defeated: r.and_then(|r| r.predator_defeated),
            pal_butcher_total: r.and_then(|r| r.pal_butcher_total),
            pal_rankup_total: r.and_then(|r| r.pal_rankup_total),
            mutation_count: r.and_then(|r| r.mutation_count),
            awakening_count: r.and_then(|r| r.awakening_count),
            camp_conquered: r.and_then(|r| r.camp_conquered),
            normal_dungeon_clear: r.and_then(|r| r.normal_dungeon_clear),
            fixed_dungeon_clear: r.and_then(|r| r.fixed_dungeon_clear),
            oilrig_clear: r.and_then(|r| r.oilrig_clear),
            found_treasure: r.and_then(|r| r.found_treasure),
            relic_possess: r.and_then(|r| r.relic_possess),
            arena_solo_clear_total: r.and_then(|r| r.arena_solo_clear_total),
            fishing_total: r.and_then(|r| r.fishing_total),
            craft_item_total: r.and_then(|r| r.craft_item_total),
            fast_travel_unlocked: r.and_then(|r| r.fast_travel_unlocked),
            game_cleared: r.and_then(|r| r.game_cleared),
            areas_discovered: r.and_then(|r| r.areas_discovered),
            area_barriers_unlocked: r.and_then(|r| r.area_barriers_unlocked),
            world_maps_unlocked: r.and_then(|r| r.world_maps_unlocked),
            notes_found: r.and_then(|r| r.notes_found),
            relics_picked_up: r.and_then(|r| r.relics_picked_up),
            items_picked_up: r.and_then(|r| r.items_picked_up),
            npc_talks_total: r.and_then(|r| r.npc_talks_total),
            npc_achievements: r.and_then(|r| r.npc_achievements),
            emotes_learned: r.and_then(|r| r.emotes_learned),
            specific_boss_defeat_total: r.and_then(|r| r.specific_boss_defeat_total),
            quests_in_progress: rec.and_then(|r| r.quests_in_progress),
            skins_owned: rec.map(|r| u32::try_from(r.skins.len()).unwrap_or(u32::MAX)),
            pals_unique_species: p.pal_summary.unique_species,
            pals_lucky: p.pal_summary.lucky,
            pals_alpha: p.pal_summary.alpha,
            pals_max_level: p.pal_summary.max_level,
            pals_average_level: p.pal_summary.average_level,
            pals_max_rank: p.pal_summary.max_rank,
            pals_awakened: p.pal_summary.awakened,
            pals_perfect_talents: p.pal_summary.perfect_talents,
            pals_four_passives: p.pal_summary.four_passives,
            capture_by_species: detail(r, |r| &r.pal_capture_by_species),
            capture_bonus_by_species: detail(r, |r| &r.capture_bonus_by_species),
            butcher_by_species: detail(r, |r| &r.butcher_by_species),
            rankup_by_rank: detail(r, |r| &r.rankup_by_rank),
            fishing_by_species: detail(r, |r| &r.fishing_by_species),
            craft_by_item: detail(r, |r| &r.craft_by_item),
            tower_boss_defeats: detail(r, |r| &r.tower_boss_defeats),
            raid_boss_defeats: detail(r, |r| &r.raid_boss_defeats),
            arena_solo_clears: detail(r, |r| &r.arena_solo_clears),
            relics_by_type: detail(r, |r| &r.relics_by_type),
            specific_boss_defeats: detail(r, |r| &r.specific_boss_defeats),
            skins: rec.map(|r| r.skins.clone()).unwrap_or_default(),
            status_points: p.status_points.clone(),
            ex_status_points: p.ex_status_points.clone(),
        }
    }
}

/// Clones a detail map out of an optional `RecordStats`.
fn detail(
    r: Option<&RecordStats>,
    f: impl Fn(&RecordStats) -> &BTreeMap<String, i32>,
) -> BTreeMap<String, i32> {
    r.map(|r| f(r).clone()).unwrap_or_default()
}

/// Collection `palworld_pals`: one row per owned pal (like `captured_pokemons`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PalworldPal {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub server: String,
    /// Relation to `palworld_characters` (the owner).
    pub character: String,
    pub species: String,
    pub is_alpha: bool,
    pub is_lucky: bool,
    pub level: u8,
    pub exp: i64,
    pub nickname: Option<String>,
    pub gender: Option<String>,
    pub rank: u8,
    pub soul_hp: u8,
    pub soul_attack: u8,
    pub soul_defense: u8,
    pub soul_craft_speed: u8,
    pub talent_hp: u8,
    pub talent_attack: u8,
    pub talent_defense: u8,
    /// json field.
    pub passives: Vec<String>,
    pub skills_learned: u32,
    pub awakened: bool,
    pub skin: Option<String>,
    pub friendship: i32,
    pub obtained_at: Option<String>,
    pub updated_at: String,
}

impl PalworldPal {
    pub fn from_pal(server: &str, character: &str, pal: &OwnedPal, now: DateTime<Utc>) -> Self {
        Self {
            id: None,
            server: server.to_string(),
            character: character.to_string(),
            species: pal.species.clone(),
            is_alpha: pal.is_alpha,
            is_lucky: pal.is_lucky,
            level: pal.level,
            exp: pal.exp,
            nickname: pal.nickname.clone(),
            gender: pal.gender.clone(),
            rank: pal.rank,
            soul_hp: pal.soul_hp,
            soul_attack: pal.soul_attack,
            soul_defense: pal.soul_defense,
            soul_craft_speed: pal.soul_craft_speed,
            talent_hp: pal.talent_hp,
            talent_attack: pal.talent_attack,
            talent_defense: pal.talent_defense,
            passives: pal.passives.clone(),
            skills_learned: pal.skills_learned,
            awakened: pal.awakened,
            skin: pal.skin.clone(),
            friendship: pal.friendship,
            obtained_at: pal.obtained_at.map(|t| t.to_rfc3339()),
            updated_at: now.to_rfc3339(),
        }
    }
}

/// Where a session boundary came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionSource {
    Log,
    Poll,
}

/// Collection `palworld_sessions`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PalworldSession {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub server: String,
    pub character: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_seconds: Option<u64>,
    pub source: SessionSource,
}

/// Collection `palworld_stats_snapshots`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PalworldStatsSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub character: String,
    pub server: String,
    pub taken_at: String,
    pub playtime_seconds: u64,
    pub level: Option<u8>,
    pub exp: Option<i64>,
    pub pal_capture_count: Option<u64>,
    pub paldeck_unique: Option<u32>,
    /// `normal_boss_defeated + tower_boss_defeated`; `None` when both are unknown.
    pub boss_defeated: Option<u32>,
}

impl PalworldStatsSnapshot {
    pub fn from_player(
        server: &str,
        character: &str,
        p: &PlayerStats,
        playtime_seconds: u64,
        now: DateTime<Utc>,
    ) -> Self {
        let r = p.record.as_ref().map(|r| &r.records);
        let normal = r.and_then(|r| r.normal_boss_defeated);
        let tower = r.and_then(|r| r.tower_boss_defeated);
        let boss_defeated = match (normal, tower) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0).saturating_add(b.unwrap_or(0))),
        };
        Self {
            id: None,
            character: character.to_string(),
            server: server.to_string(),
            taken_at: now.to_rfc3339(),
            playtime_seconds,
            level: p.level,
            exp: p.exp,
            pal_capture_count: r.and_then(|r| r.pal_capture_total),
            paldeck_unique: r.and_then(|r| r.paldeck_unlocked),
            boss_defeated,
        }
    }

    /// True when any field other than `id` / `taken_at` differs.
    pub fn differs_from(&self, other: &Self) -> bool {
        self.character != other.character
            || self.server != other.server
            || self.playtime_seconds != other.playtime_seconds
            || self.level != other.level
            || self.exp != other.exp
            || self.pal_capture_count != other.pal_capture_count
            || self.paldeck_unique != other.paldeck_unique
            || self.boss_defeated != other.boss_defeated
    }
}

/// Collection `palworld_guilds`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PalworldGuild {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub server: String,
    pub guild_id: String,
    pub name: String,
    /// Relation to `palworld_characters`.
    pub admin: Option<String>,
    pub member_count: u32,
    pub base_count: u32,
    #[serde(rename = "base_level")]
    pub base_camp_level: i32,
    pub updated_at: String,
}

impl PalworldGuild {
    pub fn from_guild(
        server: &str,
        g: &GuildInfo,
        admin_character: Option<String>,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            id: None,
            server: server.to_string(),
            guild_id: g.id.as_str().to_string(),
            name: g.name.clone(),
            admin: admin_character,
            member_count: u32::try_from(g.members.len()).unwrap_or(u32::MAX),
            base_count: g.base_count,
            base_camp_level: g.base_camp_level,
            updated_at: now.to_rfc3339(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PlayerUid;
    use crate::save::{PlayerRecord, RecordStats};

    fn player(level: u8) -> PlayerStats {
        let records = RecordStats {
            species_captured: Some(257),
            normal_boss_defeated: Some(3),
            tower_boss_defeated: Some(2),
            ..Default::default()
        };
        PlayerStats {
            uid: PlayerUid::parse("A1").unwrap(),
            name: Some("Bob".into()),
            level: Some(level),
            exp: Some(10),
            arena_rank_point: None,
            guild_id: None,
            guild_role: None,
            owned_pals: 4,
            pal_summary: Default::default(),
            pals: Vec::new(),
            status_points: Default::default(),
            ex_status_points: Default::default(),
            record: Some(PlayerRecord {
                uid: PlayerUid::parse("A1").unwrap(),
                last_online: None,
                platform_hint: None,
                technology_points: Some(5),
                boss_technology_points: None,
                unlocked_technologies: None,
                unlocked_technology_names: Vec::new(),
                completed_quests: None,
                quests_in_progress: None,
                skins: Default::default(),
                records,
            }),
        }
    }

    #[test]
    fn stats_from_player_flattens_records() {
        let s = PalworldStats::from_player("srv", "chr", &player(5), 60, Utc::now());
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"species_captured\":257"), "{json}");
        assert!(!json.contains("pal_capture_by_species"));
        assert!(json.contains("\"capture_by_species\":{}"), "{json}");
        assert!(!json.contains("\"id\""));
    }

    #[test]
    fn guild_level_serializes_as_base_level() {
        let g = PalworldGuild {
            id: None,
            server: "s".into(),
            guild_id: "g".into(),
            name: "n".into(),
            admin: None,
            member_count: 1,
            base_count: 1,
            base_camp_level: 4,
            updated_at: "t".into(),
        };
        let json = serde_json::to_string(&g).unwrap();
        assert!(json.contains("\"base_level\":4"), "{json}");
        assert!(!json.contains("base_camp_level"), "{json}");
    }

    #[test]
    fn session_source_serializes_lowercase() {
        let s = PalworldSession {
            id: None,
            server: "s".into(),
            character: "c".into(),
            started_at: "t".into(),
            ended_at: None,
            duration_seconds: None,
            source: SessionSource::Log,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"source\":\"log\""), "{json}");
    }

    #[test]
    fn snapshot_differs() {
        let t0 = Utc::now();
        let a = PalworldStatsSnapshot::from_player("s", "c", &player(5), 10, t0);
        assert_eq!(a.boss_defeated, Some(5));
        let same = PalworldStatsSnapshot::from_player("s", "c", &player(5), 10, t0);
        assert!(!a.differs_from(&same));
        let lvl = PalworldStatsSnapshot::from_player("s", "c", &player(6), 10, t0);
        assert!(a.differs_from(&lvl));
        let later = PalworldStatsSnapshot::from_player(
            "s",
            "c",
            &player(5),
            10,
            t0 + chrono::Duration::seconds(5),
        );
        assert!(!a.differs_from(&later));
    }
}
