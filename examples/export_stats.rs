//! Exports every player's stats from a Palworld world folder as pretty JSON, in the
//! PocketBase record format (`palworld_stats`, `palworld_guilds`).
//!
//! ```text
//! cargo run --release --example export_stats --features pocketbase -- <world dir> [server name] > stats.json
//! ```
//!
//! `<world dir>` is the `SaveGames/0/<worldId>` folder (`Level.sav` + `Players/`). PocketBase
//! ids are not known here, so `character` holds the player UID and `playtime_seconds` is 0
//! (playtime comes from sessions, not from saves).

use chrono::Utc;
use palworld_stats::pocketbase::{PalworldGuild, PalworldStats};
use palworld_stats::{GuildRole, parse_world};
use serde::Serialize;
use std::path::PathBuf;

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
    name: Option<String>,
    guild: Option<String>,
    guild_role: Option<GuildRole>,
    last_online: Option<String>,
    stats: PalworldStats,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let world = PathBuf::from(
        args.next()
            .ok_or("usage: export_stats <world dir> [server name]")?,
    );
    let server = args.next().unwrap_or_else(|| "palworld".to_string());

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
        .map(|p| PlayerEntry {
            player_uid: p.uid.to_string(),
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
