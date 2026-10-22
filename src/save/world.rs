//! Merge of `Level.sav` and `Players/*.sav` into a single world view.

use std::collections::BTreeMap;

use serde::Serialize;

use super::level::{CharacterInfo, GuildInfo, LevelSave, parse_level_save};
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
    pub record: Option<PlayerRecord>,
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
            let (name, level, exp, arena_rank_point) = match character {
                Some(c) => (Some(c.name), c.level, c.exp, c.arena_rank_point),
                None => (member_name, None, None, None),
            };
            PlayerStats {
                owned_pals: owned_pals.get(&uid).copied().unwrap_or(0),
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

/// Parses `Level.sav` and the player files and merges them by `PlayerUid`.
///
/// `players`: (file stem, raw bytes) from the `Players/` directory, e.g.
/// `("01C37DCB000000000000000000000000", bytes)`. Stems ending in `_dps` or not
/// parseable as a `PlayerUid` are ignored. A player file that fails to parse becomes a
/// warning (path `Players/<stem>`), not an error. Synchronous: call it inside
/// `tokio::task::spawn_blocking`.
pub fn parse_world<I>(level: &[u8], players: I) -> Result<WorldStats>
where
    I: IntoIterator<Item = (String, Vec<u8>)>,
{
    let (level, mut warnings) = parse_level_save(level)?;
    let mut records = Vec::new();
    for (stem, bytes) in players {
        if stem.ends_with("_dps") || PlayerUid::parse(&stem).is_err() {
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
            completed_quests: None,
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
}
