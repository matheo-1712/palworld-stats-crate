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
