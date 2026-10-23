use palworld_stats::{Lang, pal_name, pal_species_name, passive_name};
use std::collections::BTreeSet;
use std::path::Path;

#[test]
fn pal_names_by_language() {
    assert_eq!(pal_name("SheepBall", Lang::En), Some("Lamball"));
    assert_eq!(pal_name("SheepBall", Lang::Fr), Some("Lamball"));
    assert_eq!(pal_name("BOSS_SheepBall", Lang::Fr), Some("Énorme et Moelleux Lamball"));
}

#[test]
fn lookups_ignore_case() {
    assert_eq!(pal_name("sheepball", Lang::En), Some("Lamball"));
    assert_eq!(pal_name("Boss_SheepBall", Lang::En), Some("Big Floof Lamball"));
}

#[test]
fn species_name_drops_variant_title() {
    assert_eq!(pal_species_name("BOSS_SheepBall", Lang::Fr), Some("Lamball"));
    assert_eq!(pal_species_name("SheepBall", Lang::Fr), Some("Lamball"));
}

#[test]
fn unknown_ids_are_none() {
    assert_eq!(pal_name("NotAPal", Lang::Fr), None);
    assert_eq!(pal_species_name("NotAPal", Lang::Fr), None);
    assert_eq!(passive_name("NotAPassive", Lang::Fr), None);
}

/// Every species and passive found in the real fixture world has a name.
#[test]
fn real_world_ids_have_names() {
    let w = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("86150974C2A144B79C51A0354CCBE1FD");
    if !w.exists() {
        eprintln!("skip: fixtures absent");
        return;
    }
    let level = std::fs::read(w.join("Level.sav")).unwrap();
    let players: Vec<(String, Vec<u8>)> = std::fs::read_dir(w.join("Players"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .map(|p| {
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read(&p).unwrap(),
            )
        })
        .collect();
    let ws = palworld_stats::parse_world(&level, players).unwrap();

    let mut missing = BTreeSet::new();
    for pal in ws.players.iter().flat_map(|p| &p.pals) {
        if pal_name(&pal.species, Lang::Fr).is_none() {
            missing.insert(format!("pal {}", pal.species));
        }
        for passive in &pal.passives {
            if passive_name(passive, Lang::Fr).is_none() {
                missing.insert(format!("passive {passive}"));
            }
        }
    }
    for record in ws.players.iter().filter_map(|p| p.record.as_ref()) {
        // "Human" is the game's single capture counter for every human, not a character id
        for species in record.records.pal_capture_by_species.keys().filter(|s| *s != "Human") {
            if pal_name(species, Lang::Fr).is_none() {
                missing.insert(format!("captured {species}"));
            }
        }
    }
    assert!(missing.is_empty(), "ids without a name: {missing:?}");
}
