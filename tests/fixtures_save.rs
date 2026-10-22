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
