//! Exports every player's stats from a Palworld world folder as pretty JSON, in the
//! PocketBase record format (`palworld_stats`, `palworld_guilds`).
//!
//! ```text
//! cargo run --release --example export_stats --features pocketbase -- <world dir> [server name] [--ids <uid map.json>] > stats.json
//! ```
//!
//! `<world dir>` is the `SaveGames/0/<worldId>` folder (`Level.sav` + `Players/`). PocketBase
//! ids are not known here, so `character` holds the player UID and `playtime_seconds` is 0
//! (playtime comes from sessions, not from saves).
//!
//! Saves do not contain Steam IDs. `--ids` takes a JSON object mapping player UIDs to platform
//! ids, as the consumer records them from REST polls:
//! `{"3C36EC75000000000000000000000000": "steam_76561198000000000"}`. Players missing from the
//! map get `steam_id: null`.

use chrono::Utc;
use palworld_stats::pocketbase::{PalworldGuild, PalworldStats};
use palworld_stats::{GuildRole, PlatformId, PlayerUid, parse_world};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

const USAGE: &str = "usage: export_stats <world dir> [server name] [--ids <uid map.json>]";

#[derive(Serialize)]
struct Export {
    server: String,
    generated_at: String,
    players: Vec<PlayerEntry>,
    guilds: Vec<PalworldGuild>,
    warnings: Vec<palworld_stats::ParseWarning>,
}

#[derive(Serialize)]
struct PlayerEntry {
    player_uid: String,
    /// Steam ID digits (`7656…`), `null` when unknown or not a Steam player.
    steam_id: Option<String>,
    /// Full platform id (`steam_…`, `xbox_…`), `null` when unknown.
    platform_id: Option<String>,
    name: Option<String>,
    guild: Option<String>,
    guild_role: Option<GuildRole>,
    last_online: Option<String>,
    stats: PalworldStats,
}

/// Reads the UID → platform id map. Keys are normalised so any UID spelling matches.
fn read_ids(path: &str) -> Result<HashMap<PlayerUid, PlatformId>, Box<dyn std::error::Error>> {
    let raw: BTreeMap<String, String> = serde_json::from_slice(&std::fs::read(path)?)?;
    raw.into_iter()
        .map(|(uid, id)| Ok((PlayerUid::parse(&uid)?, PlatformId::from(id.as_str()))))
        .collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut positional = Vec::new();
    let mut ids = HashMap::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--ids" {
            ids = read_ids(&args.next().ok_or(USAGE)?)?;
        } else {
            positional.push(arg);
        }
    }
    let mut positional = positional.into_iter();
    let world = PathBuf::from(positional.next().ok_or(USAGE)?);
    let server = positional.next().unwrap_or_else(|| "palworld".to_string());

    let level = std::fs::read(world.join("Level.sav"))?;
    let mut players = Vec::new();
    for entry in std::fs::read_dir(world.join("Players"))? {
        let path = entry?.path();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        players.push((name, std::fs::read(&path)?));
    }

    let ws = parse_world(&level, players)?;
    let now = Utc::now();

    let mut entries: Vec<PlayerEntry> = ws
        .players
        .iter()
        .map(|p| {
            let platform = ids.get(&p.uid);
            PlayerEntry {
                player_uid: p.uid.to_string(),
                steam_id: match platform {
                    Some(PlatformId::Steam(id)) => Some(id.clone()),
                    _ => None,
                },
                platform_id: platform.map(|id| id.to_string()),
                name: p.name.clone(),
                guild: p
                    .guild_id
                    .as_ref()
                    .and_then(|id| ws.guilds.iter().find(|g| &g.id == id))
                    .map(|g| g.name.clone()),
                guild_role: p.guild_role,
                last_online: p
                    .record
                    .as_ref()
                    .and_then(|r| r.last_online)
                    .map(|t| t.to_rfc3339()),
                stats: PalworldStats::from_player(&server, p.uid.as_str(), p, 0, now),
            }
        })
        .collect();
    // Leaderboard order: highest level first, then most XP.
    entries.sort_by_key(|e| std::cmp::Reverse((e.stats.level, e.stats.exp)));

    let guilds = ws
        .guilds
        .iter()
        .map(|g| PalworldGuild::from_guild(&server, g, Some(g.admin.to_string()), now))
        .collect();

    let export = Export {
        server,
        generated_at: now.to_rfc3339(),
        players: entries,
        guilds,
        warnings: ws.warnings,
    };
    println!("{}", serde_json::to_string_pretty(&export)?);
    Ok(())
}
