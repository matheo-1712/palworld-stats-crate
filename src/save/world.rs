//! Merge of `Level.sav` and `Players/*.sav` into a single world view.

use std::collections::BTreeMap;

use serde::Serialize;

use super::level::{CharacterInfo, GuildInfo, LevelSave, OwnedPal, PalLocation, parse_level_save};
use super::player::{PlayerRecord, parse_player_save};
use crate::error::{ParseWarning, Result};
use crate::ids::{GuildId, PlayerUid};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GuildRole {
    Admin,
    Member,
}

/// Everything known about one player, merged from Level.sav and the player file.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlayerStats {
    pub uid: PlayerUid,
    /// `CharacterInfo.name`, else a guild member name.
    pub name: Option<String>,
    pub level: Option<u8>,
    pub exp: Option<i64>,
    pub arena_rank_point: Option<i32>,
    pub guild_id: Option<GuildId>,
    pub guild_role: Option<GuildRole>,
    pub owned_pals: u32,
    /// Aggregates over `pals`.
    pub pal_summary: PalSummary,
    /// Every pal the player owns (from Level.sav).
    pub pals: Vec<OwnedPal>,
    /// Status points spent per stat (internal Japanese stat names as keys).
    pub status_points: BTreeMap<String, i32>,
    /// Extra status points per stat, same keys.
    pub ex_status_points: BTreeMap<String, i32>,
    pub record: Option<PlayerRecord>,
}

/// Aggregates over a player's pals, ready for leaderboards.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PalSummary {
    pub total: u32,
    /// Distinct species (an alpha counts as its base species).
    pub unique_species: u32,
    pub lucky: u32,
    pub alpha: u32,
    pub max_level: u8,
    /// Mean level, `None` when the player owns no pal.
    pub average_level: Option<f64>,
    /// Pals at condensation rank 5 (4 stars).
    pub max_rank: u32,
    pub awakened: u32,
    /// Pals with 100 in all three talents.
    pub perfect_talents: u32,
    /// Pals with four passive skills.
    pub four_passives: u32,
    /// Pals in the party / in the Palbox (0 when the player file is missing).
    pub in_party: u32,
    pub in_box: u32,
}

impl PalSummary {
    pub fn from_pals(pals: &[OwnedPal]) -> Self {
        let count = |f: &dyn Fn(&OwnedPal) -> bool| {
            u32::try_from(pals.iter().filter(|p| f(p)).count()).unwrap_or(u32::MAX)
        };
        let mut species: Vec<String> = pals
            .iter()
            .map(|p| {
                let s = p.species.to_ascii_lowercase();
                s.strip_prefix("boss_").map(str::to_string).unwrap_or(s)
            })
            .collect();
        species.sort();
        species.dedup();
        let levels: u64 = pals.iter().map(|p| u64::from(p.level)).sum();
        Self {
            total: u32::try_from(pals.len()).unwrap_or(u32::MAX),
            unique_species: u32::try_from(species.len()).unwrap_or(u32::MAX),
            lucky: count(&|p| p.is_lucky),
            alpha: count(&|p| p.is_alpha),
            max_level: pals.iter().map(|p| p.level).max().unwrap_or(0),
            average_level: (!pals.is_empty()).then(|| levels as f64 / pals.len() as f64),
            max_rank: count(&|p| p.rank >= 5),
            awakened: count(&|p| p.awakened),
            perfect_talents: count(&|p| {
                p.talent_hp >= 100 && p.talent_attack >= 100 && p.talent_defense >= 100
            }),
            four_passives: count(&|p| p.passives.len() >= 4),
            in_party: count(&|p| p.location == Some(PalLocation::Party)),
            in_box: count(&|p| p.location == Some(PalLocation::Box)),
        }
    }
}

impl PlayerStats {
    /// The pals in the player's party, in party order.
    pub fn party(&self) -> Vec<&OwnedPal> {
        let mut party: Vec<&OwnedPal> = self
            .pals
            .iter()
            .filter(|p| p.location == Some(PalLocation::Party))
            .collect();
        party.sort_by_key(|p| p.slot_index);
        party
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct WorldStats {
    pub players: Vec<PlayerStats>,
    pub guilds: Vec<GuildInfo>,
    pub warnings: Vec<ParseWarning>,
}

fn merge(level: LevelSave, records: Vec<PlayerRecord>, warnings: Vec<ParseWarning>) -> WorldStats {
    let LevelSave {
        players,
        owned_pals,
        mut pals,
        guilds,
    } = level;
    let mut chars: BTreeMap<PlayerUid, CharacterInfo> =
        players.into_iter().map(|c| (c.uid.clone(), c)).collect();
    let mut recs: BTreeMap<PlayerUid, PlayerRecord> =
        records.into_iter().map(|r| (r.uid.clone(), r)).collect();

    let mut uids: Vec<PlayerUid> = chars.keys().chain(recs.keys()).cloned().collect();
    uids.sort();
    uids.dedup();

    let players = uids
        .into_iter()
        .map(|uid| {
            let character = chars.remove(&uid);
            let record = recs.remove(&uid);
            let mut guild_id = None;
            let mut guild_role = None;
            let mut member_name = None;
            for g in &guilds {
                let member = g.members.iter().find(|m| m.uid == uid);
                if g.admin == uid || member.is_some() {
                    guild_id = Some(g.id.clone());
                    guild_role = Some(if g.admin == uid {
                        GuildRole::Admin
                    } else {
                        GuildRole::Member
                    });
                    member_name = member.map(|m| m.name.clone());
                    break;
                }
            }
            let (name, level, exp, arena_rank_point, status_points, ex_status_points) =
                match character {
                    Some(c) => (
                        Some(c.name),
                        c.level,
                        c.exp,
                        c.arena_rank_point,
                        c.status_points,
                        c.ex_status_points,
                    ),
                    None => (
                        member_name,
                        None,
                        None,
                        None,
                        BTreeMap::new(),
                        BTreeMap::new(),
                    ),
                };
            let mut player_pals = pals.remove(&uid).unwrap_or_default();
            if let Some(r) = &record {
                for pal in &mut player_pals {
                    pal.location = Some(match pal.container_id.as_deref() {
                        Some(c) if Some(c) == r.party_container_id.as_deref() => PalLocation::Party,
                        Some(c) if Some(c) == r.box_container_id.as_deref() => PalLocation::Box,
                        _ => PalLocation::Other,
                    });
                }
            }
            PlayerStats {
                owned_pals: owned_pals.get(&uid).copied().unwrap_or(0),
                pal_summary: PalSummary::from_pals(&player_pals),
                pals: player_pals,
                status_points,
                ex_status_points,
                uid,
                name,
                level,
                exp,
                arena_rank_point,
                guild_id,
                guild_role,
                record,
            }
        })
        .collect();

    WorldStats {
        players,
        guilds,
        warnings,
    }
}

/// Strips a trailing `.sav` (case-insensitive) from a file name.
fn strip_sav(name: &str) -> &str {
    let n = name.len();
    if n >= 4 && name.is_char_boundary(n - 4) && name[n - 4..].eq_ignore_ascii_case(".sav") {
        &name[..n - 4]
    } else {
        name
    }
}

/// Parses `Level.sav` and the player files and merges them by `PlayerUid`.
///
/// `players`: (file stem, raw bytes) from the `Players/` directory, e.g.
/// `("01C37DCB000000000000000000000000", bytes)`. The file name may keep its `.sav`
/// extension (case-insensitive). Names ending in `_dps`, and `LevelMeta`/`Level`, are
/// ignored silently; any other name that is not a valid `PlayerUid` is skipped with a
/// warning (path `Players/<name>`). A player file that fails to parse becomes a
/// warning (path `Players/<stem>`), not an error. Synchronous: call it inside
/// `tokio::task::spawn_blocking`.
pub fn parse_world<I>(level: &[u8], players: I) -> Result<WorldStats>
where
    I: IntoIterator<Item = (String, Vec<u8>)>,
{
    let (level, mut warnings) = parse_level_save(level)?;
    let mut records = Vec::new();
    for (name, bytes) in players {
        let stem = strip_sav(&name).to_string();
        let lower = stem.to_ascii_lowercase();
        if lower.ends_with("_dps") || lower == "levelmeta" || lower == "level" {
            continue;
        }
        if PlayerUid::parse(&stem).is_err() {
            warnings.push(ParseWarning {
                path: format!("Players/{name}"),
                message: "not a player save file name".into(),
            });
            continue;
        }
        match parse_player_save(&bytes) {
            Ok((record, ws)) => {
                warnings.extend(ws.into_iter().map(|w| ParseWarning {
                    path: if w.path.is_empty() {
                        format!("Players/{stem}")
                    } else {
                        format!("Players/{stem}/{}", w.path)
                    },
                    message: w.message,
                }));
                records.push(record);
            }
            Err(e) => warnings.push(ParseWarning {
                path: format!("Players/{stem}"),
                message: e.to_string(),
            }),
        }
    }
    Ok(merge(level, records, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::player::RecordStats;
    use crate::save::rawdata::GuildMember;

    fn uid(s: &str) -> PlayerUid {
        PlayerUid::parse(s).unwrap()
    }

    fn character(u: &str, name: &str) -> CharacterInfo {
        CharacterInfo {
            uid: uid(u),
            name: name.into(),
            level: Some(10),
            exp: Some(5),
            arena_rank_point: Some(1),
            group_id: None,
            status_points: Default::default(),
            ex_status_points: Default::default(),
        }
    }

    fn record(u: &str) -> PlayerRecord {
        PlayerRecord {
            uid: uid(u),
            last_online: None,
            platform_hint: None,
            technology_points: None,
            boss_technology_points: None,
            unlocked_technologies: None,
            unlocked_technology_names: Vec::new(),
            completed_quests: None,
            quests_in_progress: None,
            skins: Default::default(),
            party_container_id: None,
            box_container_id: None,
            records: RecordStats::default(),
        }
    }

    fn guild(admin: &str, members: &[(&str, &str)]) -> GuildInfo {
        GuildInfo {
            id: GuildId::from_guid(&[7; 16]),
            name: "G".into(),
            admin: uid(admin),
            members: members
                .iter()
                .map(|(u, n)| GuildMember {
                    uid: uid(u),
                    name: (*n).into(),
                    last_online_ticks: 0,
                })
                .collect(),
            base_count: 1,
            base_camp_level: 1,
        }
    }

    #[test]
    fn orphans_are_kept() {
        let level = LevelSave {
            players: vec![character("AA", "Alice")],
            ..Default::default()
        };
        let ws = merge(level, vec![record("BB")], vec![]);
        assert_eq!(ws.players.len(), 2);
        let a = &ws.players[0];
        assert_eq!(a.name.as_deref(), Some("Alice"));
        assert_eq!(a.level, Some(10));
        assert!(a.record.is_none());
        let b = &ws.players[1];
        assert_eq!(b.uid, uid("BB"));
        assert_eq!(b.name, None);
        assert_eq!(b.level, None);
        assert!(b.record.is_some());
    }

    #[test]
    fn guild_role_assigned() {
        let level = LevelSave {
            players: vec![character("AA", "Alice"), character("BB", "Bob")],
            guilds: vec![guild(
                "AA",
                &[
                    ("AA", "Alice"),
                    ("BB", "Bob"),
                    ("CC", "Cy"),
                    ("EE", "Ghost"),
                ],
            )],
            ..Default::default()
        };
        let ws = merge(level, vec![record("CC"), record("DD")], vec![]);
        let by = |u: &str| ws.players.iter().find(|p| p.uid == uid(u)).unwrap();
        assert_eq!(by("AA").guild_role, Some(GuildRole::Admin));
        assert_eq!(by("BB").guild_role, Some(GuildRole::Member));
        assert!(by("BB").guild_id.is_some());
        // Name fallback from the guild member list for a record without a character.
        assert_eq!(by("CC").name.as_deref(), Some("Cy"));
        assert_eq!(by("DD").guild_role, None);
        assert_eq!(by("DD").guild_id, None);
        // Guild-only members (EE) are not added.
        assert_eq!(ws.players.len(), 4);
    }

    fn pal(species: &str, level: u8, rank: u8) -> OwnedPal {
        OwnedPal {
            species: species.into(),
            is_alpha: species.starts_with("BOSS_"),
            is_lucky: false,
            level,
            exp: 0,
            nickname: None,
            gender: None,
            rank,
            soul_hp: 0,
            soul_attack: 0,
            soul_defense: 0,
            soul_craft_speed: 0,
            talent_hp: 100,
            talent_attack: 100,
            talent_defense: 50,
            passives: vec!["a".into(), "b".into(), "c".into(), "d".into()],
            skills_learned: 0,
            awakened: false,
            skin: None,
            friendship: 0,
            obtained_at: None,
            container_id: None,
            slot_index: None,
            location: None,
        }
    }

    #[test]
    fn pal_summary_aggregates() {
        let mut lucky = pal("WoolFox", 10, 1);
        lucky.is_lucky = true;
        lucky.talent_defense = 100;
        let pals = vec![lucky, pal("BOSS_WoolFox", 30, 5), pal("Anubis", 50, 2)];
        let s = PalSummary::from_pals(&pals);
        assert_eq!(s.total, 3);
        assert_eq!(s.unique_species, 2); // the alpha WoolFox counts as WoolFox
        assert_eq!(s.lucky, 1);
        assert_eq!(s.alpha, 1);
        assert_eq!(s.max_level, 50);
        assert_eq!(s.average_level, Some(30.0));
        assert_eq!(s.max_rank, 1);
        assert_eq!(s.perfect_talents, 1);
        assert_eq!(s.four_passives, 3);
        assert_eq!(PalSummary::from_pals(&[]).average_level, None);
    }
}
