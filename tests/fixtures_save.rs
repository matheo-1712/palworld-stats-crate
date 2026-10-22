use palworld_stats::save::decompress;
use std::path::{Path, PathBuf};

fn world() -> Option<PathBuf> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("86150974C2A144B79C51A0354CCBE1FD");
    if p.exists() {
        Some(p)
    } else {
        eprintln!("skip: fixtures absent");
        None
    }
}

fn collect_sav(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_sav(&path, out);
        } else if path.extension().is_some_and(|e| e == "sav") {
            out.push(path);
        }
    }
}

#[test]
fn decompresses_real_level_and_player() {
    let Some(w) = world() else { return };
    let level = decompress(&std::fs::read(w.join("Level.sav")).unwrap()).unwrap();
    assert_eq!(level.len(), 91_601_811);
    assert_eq!(&level[..4], b"GVAS");
    let p =
        decompress(&std::fs::read(w.join("Players/01C37DCB000000000000000000000000.sav")).unwrap())
            .unwrap();
    assert_eq!(p.len(), 103_601);
}

#[test]
fn every_fixture_file_decompresses() {
    let Some(w) = world() else { return };
    let mut files = Vec::new();
    collect_sav(&w, &mut files);
    assert!(!files.is_empty());
    for f in files {
        let out = decompress(&std::fs::read(&f).unwrap())
            .unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        assert_eq!(&out[..4], b"GVAS", "{}", f.display());
    }
}

#[test]
fn truncated_body_is_error() {
    let Some(w) = world() else { return };
    let raw = std::fs::read(w.join("Players/01C37DCB000000000000000000000000.sav")).unwrap();
    assert!(decompress(&raw[..raw.len() / 2]).is_err());
}

#[test]
fn corrupted_oodle_body_is_error_not_panic() {
    let Some(w) = world() else { return };
    let raw = std::fs::read(w.join("Players/01C37DCB000000000000000000000000.sav")).unwrap();

    let mut xored = raw.clone();
    for b in &mut xored[12..] {
        *b ^= 0x5A;
    }
    assert!(decompress(&xored).is_err());

    let mut ff = raw.clone();
    let end = (12 + 4096).min(ff.len());
    ff[12..end].fill(0xFF);
    assert!(decompress(&ff).is_err());
}

#[test]
fn parses_real_player_record() {
    let Some(w) = world() else { return };
    let raw = std::fs::read(w.join("Players/3C36EC75000000000000000000000000.sav")).unwrap();
    let (p, warnings) = palworld_stats::save::parse_player_save(&raw).unwrap();
    assert_eq!(p.uid.as_str(), "3C36EC75000000000000000000000000");
    let r = &p.records;
    assert_eq!(r.species_captured, Some(257));
    assert_eq!(r.normal_dungeon_clear, Some(22));
    assert_eq!(r.fixed_dungeon_clear, Some(30));
    assert_eq!(r.oilrig_clear, Some(23));
    assert_eq!(r.camp_conquered, Some(23));
    assert_eq!(r.found_treasure, Some(11));
    assert_eq!(r.mutation_count, Some(45));
    assert_eq!(r.awakening_count, Some(2));
    assert_eq!(r.predator_defeated, Some(2));
    assert_eq!(r.relic_possess, Some(2));
    assert_eq!(r.game_cleared, Some(true));
    assert_eq!(r.pal_capture_by_species.len(), 269);
    assert!(r.paldeck_unlocked.unwrap() > 0 && r.paldeck_unlocked.unwrap() <= 257);
    assert!(p.unlocked_technologies.unwrap() > 0);
    assert!(p.last_online.is_some());
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn every_real_player_file_parses() {
    let Some(w) = world() else { return };
    let mut n = 0;
    for entry in std::fs::read_dir(w.join("Players")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !name.ends_with(".sav") || name.ends_with("_dps.sav") {
            continue;
        }
        let raw = std::fs::read(&path).unwrap();
        let (p, warnings) =
            palworld_stats::save::parse_player_save(&raw).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(p.uid.as_str(), path.file_stem().unwrap().to_string_lossy());
        assert!(warnings.is_empty(), "{name}: {warnings:?}");
        if name.starts_with("5A713712") {
            assert_eq!(p.records.species_captured, Some(0));
        }
        n += 1;
    }
    assert!(n > 0);
}

#[test]
fn parses_real_level() {
    let Some(w) = world() else { return };
    let (l, warnings) =
        palworld_stats::save::parse_level_save(&std::fs::read(w.join("Level.sav")).unwrap())
            .unwrap();
    assert_eq!(l.players.len(), 6);
    let rere = l
        .players
        .iter()
        .find(|c| c.uid.as_str() == "3C36EC75000000000000000000000000")
        .unwrap();
    assert_eq!(rere.name, "RereBleue");
    assert_eq!(rere.level, Some(80));
    assert_eq!(rere.exp, Some(45_859_908));
    assert_eq!(rere.arena_rank_point, Some(585));
    assert!(!l.guilds.is_empty(), "{warnings:?}");
    assert!(l.guilds.iter().all(|g| !g.members.is_empty()));
    assert!(l.owned_pals.values().sum::<u32>() > 0);
    assert!(warnings.is_empty(), "{warnings:?}");
    eprintln!(
        "level: players={} guilds={} owned_pals={}",
        l.players.len(),
        l.guilds.len(),
        l.owned_pals.values().sum::<u32>()
    );
}

#[test]
fn level_parse_is_fast() {
    let Some(w) = world() else { return };
    let raw = std::fs::read(w.join("Level.sav")).unwrap();
    let t = std::time::Instant::now();
    palworld_stats::save::parse_level_save(&raw).unwrap();
    let el = t.elapsed();
    eprintln!("level parse: {el:?}");
    assert!(el < std::time::Duration::from_secs(3), "{el:?}");
}

#[test]
fn parses_real_world() {
    let Some(w) = world() else { return };
    let level = std::fs::read(w.join("Level.sav")).unwrap();
    let read = |with_ext: bool| -> Vec<(String, Vec<u8>)> {
        std::fs::read_dir(w.join("Players"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .map(|p| {
                let name = if with_ext {
                    p.file_name().unwrap().to_string_lossy().into_owned()
                } else {
                    p.file_stem().unwrap().to_string_lossy().into_owned()
                };
                (name, std::fs::read(&p).unwrap())
            })
            .collect()
    };
    // File names with the `.sav` extension must work like bare stems.
    let with_ext = palworld_stats::save::parse_world(&level, read(true)).unwrap();
    assert_eq!(with_ext.players.len(), 6);
    assert_eq!(
        with_ext
            .players
            .iter()
            .filter(|p| p.record.is_some())
            .count(),
        6
    );
    assert!(with_ext.warnings.is_empty(), "{:?}", with_ext.warnings);
    let ws = palworld_stats::save::parse_world(&level, read(false)).unwrap();
    assert_eq!(ws.players.len(), 6);
    assert!(
        ws.players
            .iter()
            .all(|p| p.record.is_some() && p.name.is_some() && p.level.unwrap_or(0) > 0)
    );
    // A player with no Level/Exp property gets the engine defaults.
    let slowe = ws
        .players
        .iter()
        .find(|p| p.uid.as_str().starts_with("5A713712"))
        .unwrap();
    assert_eq!(slowe.level, Some(1));
    assert_eq!(slowe.exp, Some(31));
    assert!(ws.warnings.is_empty(), "{:?}", ws.warnings);
    serde_json::to_string(&ws).unwrap();
}

#[test]
fn dps_and_unknown_files_are_ignored() {
    let Some(w) = world() else { return };
    let level = std::fs::read(w.join("Level.sav")).unwrap();
    let junk = vec![1u8, 2, 3];
    let ws = palworld_stats::save::parse_world(
        &level,
        vec![
            ("X_dps".to_string(), junk.clone()),
            ("X_DPS.sav".to_string(), junk.clone()),
            ("LevelMeta".to_string(), junk.clone()),
            ("LevelMeta.sav".to_string(), junk.clone()),
        ],
    )
    .unwrap();
    assert!(ws.warnings.is_empty(), "{:?}", ws.warnings);
    assert_eq!(ws.players.len(), 6);
    assert!(ws.players.iter().all(|p| p.record.is_none()));

    let ws =
        palworld_stats::save::parse_world(&level, vec![("garbage.sav".to_string(), junk)]).unwrap();
    assert_eq!(ws.warnings.len(), 1, "{:?}", ws.warnings);
    assert_eq!(ws.warnings[0].path, "Players/garbage.sav");
    assert_eq!(ws.warnings[0].message, "not a player save file name");
}
