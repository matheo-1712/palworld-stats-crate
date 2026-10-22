//! `serde` structs for the proposed PocketBase collections (feature `pocketbase`).
//!
//! Conventions: ids and relations are `String`, dates are RFC 3339 strings. Values that can
//! be unknown are `Option` and serialized as `null`.

use crate::save::{GuildInfo, PlayerStats};
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
/// `pal_capture_by_species` is deliberately not stored.
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
            record: Some(PlayerRecord {
                uid: PlayerUid::parse("A1").unwrap(),
                last_online: None,
                platform_hint: None,
                technology_points: Some(5),
                boss_technology_points: None,
                unlocked_technologies: None,
                completed_quests: None,
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
