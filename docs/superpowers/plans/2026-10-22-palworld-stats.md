# palworld-stats Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `palworld-stats` Rust library crate that Otternel calls to read Palworld player statistics from the server REST API, the server logs, and the `.sav` save files.

**Architecture:** A stateless library with no disk I/O and no database. It takes bytes and strings and returns serde-serializable structs. Modules: `ids`, `rest` (async reqwest), `presence`, `logs`, and `save`. `save` has a container decompressor (Oodle via `oozextract`, zlib via `flate2`), a generic GVAS reader that skips unneeded properties by size, Palworld RawData decoders, and per-file parsers merged by `parse_world`. An optional `pocketbase` feature carries the proposed collection structs.

**Tech Stack:** Rust edition 2024, `oozextract` 0.5.5, `flate2`, `thiserror` 2, `serde`/`serde_json`, `chrono`, `reqwest` (rustls, json), `regex`, `log`; dev: `tokio`, `wiremock`.

**Spec:** `docs/superpowers/specs/2026-10-22-palworld-stats-design.md`

## Global Constraints

- Crate name `palworld-stats`, lib only (no binary), license MIT, `publish = false`, edition 2024.
- No disk I/O, no Docker, no database, no global state in library code. Inputs are `&[u8]` / `&str`, outputs are structs.
- No dependency on the proprietary Oodle library, Python, or any external tool.
- Library code never panics on input data: no `unwrap`/`expect`/indexing that can go out of bounds on file contents. Every read is bounds-checked and returns `Error`.
- A missing or unexpected-type property never fails a parse. It yields `None` plus a `ParseWarning`. Only an invalid header, a decompression failure, or an unreadable GVAS header/top level are `Err`.
- Player IP addresses from the REST API are never exposed in any public type.
- `.sav` parsing is synchronous. Public docs tell callers to run it inside `tokio::task::spawn_blocking`.
- `fixtures/` (real saves, about 100 MB, real player names) is git-ignored. Integration tests that need it return early with a `eprintln!` skip notice when it is absent.
- Fixture world path: `fixtures/86150974C2A144B79C51A0354CCBE1FD/` (`Level.sav`, `LevelMeta.sav`, `Players/*.sav`; `backup/` is ignored).
- Commits end with the line `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

- **Corrupted or truncated `.sav`** (copied while the server was writing) → `Err`, never a panic or a huge allocation. Pinned in Task 2 (`truncated_body_is_error`, `absurd_length_is_error`).
- **New or renamed property after a Palworld update** → the property is kept as `Value::Raw` with a warning and the rest still parses. Pinned in Task 3 (`unknown_property_type_becomes_raw`).
- **`*_dps.sav` and non-player files passed to `parse_world`** → silently ignored. Pinned in Task 7 (`dps_and_unknown_files_are_ignored`).
- **A player file with no character in `Level.sav`** (or a character with no player file) → still listed with `None` fields. Pinned in Task 7 (`orphans_are_kept`).
- **UID format mismatch** between the REST `playerId` (case, dashes, length) and the save file name → both normalize to the same `PlayerUid`. Pinned in Task 1 (`uid_normalization`).

---

## File Structure

```
Cargo.toml
.gitignore                 # /target, /fixtures
LICENSE                    # MIT
README.md                  # usage + Otternel integration (Task 10)
src/lib.rs                 # module decls + re-exports
src/error.rs               # Error, Result, ParseWarning
src/ids.rs                 # PlatformId, PlayerUid, GuildId, guid_to_string
src/save/mod.rs            # pub use of save API
src/save/container.rs      # decompress()
src/save/gvas.rs           # Reader, Value, Props, GvasHeader, ParseOptions
src/save/rawdata.rs        # decode_character(), decode_group()
src/save/player.rs         # parse_player_save() -> PlayerRecord
src/save/level.rs          # parse_level_save() -> LevelSave
src/save/world.rs          # parse_world() -> WorldStats
src/rest.rs                # RestClient + DTOs
src/presence.rs            # diff_presence()
src/logs.rs                # parse_log_line()
src/pocketbase.rs          # feature "pocketbase"
tests/fixtures_save.rs     # integration tests on real saves
tests/rest.rs              # wiremock tests
```

---

### Task 1: Crate scaffold, errors, identifiers

**Files:**
- Create: `Cargo.toml`, `.gitignore`, `LICENSE`, `src/lib.rs`, `src/error.rs`, `src/ids.rs`

**Interfaces:**
- Produces:
  - `pub enum Error { InvalidHeader(String), Decompress(String), Gvas { offset: usize, msg: String }, Http(String), Unauthorized, Status(u16), InvalidId(String) }`, derived via `thiserror`. Reqwest errors are converted to `Http(e.to_string())` in Task 8.
  - `pub type Result<T> = std::result::Result<T, Error>;`
  - `#[derive(Debug, Clone, PartialEq, Serialize)] pub struct ParseWarning { pub path: String, pub message: String }`
  - `pub fn guid_to_string(bytes: &[u8; 16]) -> String`: the four little-endian `u32` words, each formatted `{:08X}`, concatenated (32 uppercase hex chars, no dashes). This is the UE FGuid order and matches the `Players/<UID>.sav` file names.
  - `#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)] #[serde(transparent)] pub struct PlayerUid(String)` with `PlayerUid::parse(s: &str) -> Result<PlayerUid>`, `PlayerUid::from_guid(bytes: &[u8;16]) -> PlayerUid`, `as_str()`, `Display`, `is_nil()`.
  - `pub struct GuildId(String)`: same derives, plus `GuildId::from_guid(&[u8;16])`, `as_str()`, `Display`.
  - `#[derive(Debug, Clone, PartialEq, Eq, Hash)] pub enum PlatformId { Steam(String), Xbox(String), Other(String) }` where the inner value excludes the prefix (except `Other`, which holds the full string). It implements `From<&str>` and `Display` (round-trips `"steam_7656…"`, `"xbox_…"`), and serde as that string.
- Consumes: nothing.

- [ ] **Step 1: Initialise the repo and manifest**

Run `git init` in the project root. `Cargo.toml`: `[package] name = "palworld-stats"`, `version = "0.1.0"`, `edition = "2024"`, `license = "MIT"`, `publish = false`. Deps: `oozextract = "0.5.5"`, `flate2 = "1"`, `thiserror = "2"`, `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, `chrono = { version = "0.4", features = ["serde"] }`, `reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }`, `regex = "1"`, `log = "0.4"`. `[features] pocketbase = []`. Dev-deps: `tokio = { version = "1", features = ["macros", "rt-multi-thread"] }`, `wiremock = "0.6"`. `.gitignore`: `/target`, `/fixtures`. `LICENSE`: MIT, copyright "2026 L'Antre des Loutres".

- [ ] **Step 2: Write failing tests in `src/ids.rs` (`#[cfg(test)]`)**

```rust
#[test]
fn guid_to_string_matches_save_file_names() {
    let mut b = [0u8; 16];
    b[..4].copy_from_slice(&[0x75, 0xec, 0x36, 0x3c]);
    assert_eq!(guid_to_string(&b), "3C36EC75000000000000000000000000");
}
#[test]
fn uid_normalization() {
    let a = PlayerUid::parse("3c36ec75000000000000000000000000").unwrap();
    let b = PlayerUid::parse("3C36EC75-0000-0000-0000-000000000000").unwrap();
    let c = PlayerUid::parse("3c36ec75").unwrap(); // short REST form: right-padded with '0' to 32
    assert_eq!(a, b);
    assert_eq!(a, c);
    assert_eq!(a.as_str(), "3C36EC75000000000000000000000000");
    assert!(PlayerUid::parse("zz").is_err());
    assert!(PlayerUid::parse("").is_err());
}
#[test]
fn platform_id_round_trip() {
    assert_eq!(PlatformId::from("steam_76561198000000000"), PlatformId::Steam("76561198000000000".into()));
    assert_eq!(PlatformId::from("xbox_2535400000000000"), PlatformId::Xbox("2535400000000000".into()));
    assert_eq!(PlatformId::from("psn_1"), PlatformId::Other("psn_1".into()));
    assert_eq!(PlatformId::Steam("1".into()).to_string(), "steam_1");
    assert_eq!(serde_json::to_string(&PlatformId::Xbox("9".into())).unwrap(), "\"xbox_9\"");
}
```

- [ ] **Step 3: Run `cargo test --lib ids`**. Expected: FAIL (does not compile; items undefined).

- [ ] **Step 4: Implement `src/error.rs`, `src/ids.rs`, and `src/lib.rs`** (`pub mod error; pub mod ids; pub use error::{Error, Result, ParseWarning}; pub use ids::*;`). `PlayerUid::parse`: strip `-`, require 1 to 32 ASCII hex chars, uppercase, right-pad with `'0'` to 32, else `Error::InvalidId`.

- [ ] **Step 5: Run `cargo test --lib ids`**. Expected: 3 passed.

- [ ] **Step 6: Commit** `git add -A && git commit -m "feat: scaffold crate with errors and identifiers"`

---

### Task 2: Save container decompression

**Files:**
- Create: `src/save/mod.rs`, `src/save/container.rs`, `tests/fixtures_save.rs`
- Modify: `src/lib.rs` (add `pub mod save;`)

**Interfaces:**
- Consumes: `Error`, `Result` (Task 1).
- Produces: `pub fn decompress(raw: &[u8]) -> Result<Vec<u8>>` (re-exported as `palworld_stats::save::decompress`).

Format: 12-byte header: `u32 LE` uncompressed length, `u32 LE` compressed length, 3-byte magic, 1-byte save type.
- `PlM` (type `'1'`): Oodle. `oozextract::Extractor::new().read_from_slice(&raw[12..], &mut out)` with `out = vec![0; ulen]`. The result must equal `ulen`.
- `PlZ` type `'1'`: zlib once. Type `'2'`: zlib twice.
- Any other magic: `InvalidHeader`.
- Reject `ulen > 512 MiB` with `InvalidHeader`, so a corrupted header cannot trigger a giant allocation. Reject `raw.len() < 12`.

- [ ] **Step 1: Write failing unit tests in `container.rs`**

```rust
#[test] fn too_short_is_error() { assert!(matches!(decompress(&[0; 5]), Err(Error::InvalidHeader(_)))); }
#[test] fn unknown_magic_is_error() {
    let mut v = vec![4,0,0,0, 4,0,0,0]; v.extend_from_slice(b"XYZ1"); v.extend_from_slice(&[0;4]);
    assert!(matches!(decompress(&v), Err(Error::InvalidHeader(_))));
}
#[test] fn absurd_length_is_error() {
    let mut v = vec![0xff,0xff,0xff,0xff, 4,0,0,0]; v.extend_from_slice(b"PlM1"); v.extend_from_slice(&[0;4]);
    assert!(matches!(decompress(&v), Err(Error::InvalidHeader(_))));
}
#[test] fn plz_single_and_double_zlib() { /* build with flate2::write::ZlibEncoder: b"GVAS-test" -> header(ulen=9, clen, "PlZ", b'1') and the double-compressed variant with b'2'; both decompress to b"GVAS-test" */ }
```

- [ ] **Step 2: Write failing integration tests in `tests/fixtures_save.rs`**

Shared helper: `fn world() -> Option<std::path::PathBuf>` returns `Some(fixtures/86150974C2A144B79C51A0354CCBE1FD)` if it exists, else prints `skip: fixtures absent` and returns `None`.

```rust
#[test] fn decompresses_real_level_and_player() {
    let Some(w) = world() else { return };
    let level = decompress(&std::fs::read(w.join("Level.sav")).unwrap()).unwrap();
    assert_eq!(level.len(), 91_601_811);
    assert_eq!(&level[..4], b"GVAS");
    let p = decompress(&std::fs::read(w.join("Players/01C37DCB000000000000000000000000.sav")).unwrap()).unwrap();
    assert_eq!(p.len(), 103_601);
}
#[test] fn every_fixture_file_decompresses() { /* walk w recursively (including backup/), every *.sav decompresses and starts with b"GVAS" */ }
#[test] fn truncated_body_is_error() {
    let Some(w) = world() else { return };
    let raw = std::fs::read(w.join("Players/01C37DCB000000000000000000000000.sav")).unwrap();
    assert!(decompress(&raw[..raw.len() / 2]).is_err());
}
```

- [ ] **Step 3: Run `cargo test container && cargo test --test fixtures_save`**. Expected: FAIL (compile).
- [ ] **Step 4: Implement `decompress`.**
- [ ] **Step 5: Run the same command.** Expected: all pass (the fixture tests take about 1 s in release; use `cargo test --release` if debug is too slow).
- [ ] **Step 6: Commit** `feat(save): decompress PlM/PlZ save containers`

---

### Task 3: Generic GVAS reader

**Files:**
- Create: `src/save/gvas.rs`
- Modify: `src/save/mod.rs`

**Interfaces:**
- Consumes: `Error`, `Result`, `ParseWarning` (Task 1).
- Produces:

```rust
pub struct GvasHeader { pub save_game_version: i32, pub engine: String /* "5.1.1-0+++UE5+Release-5.1" */, pub save_class: String }
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i32), Int64(i64), UInt32(u32), Float(f32), Str(String), Name(String), Bool(bool),
    Byte(u8), Enum(String), Guid([u8; 16]), DateTime(u64),
    Struct { type_name: String, props: Props },
    Array(Vec<Value>), Bytes(Vec<u8>), Map(Vec<(Value, Value)>),
    Raw(Vec<u8>), // skipped or undecodable payload, kept verbatim
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Props(pub Vec<(String, Value)>);
impl Props {
    pub fn get(&self, name: &str) -> Option<&Value>;
    pub fn struct_props(&self, name: &str) -> Option<&Props>; // Value::Struct -> &props
    pub fn int(&self, name: &str) -> Option<i64>;              // Int | Int64 | UInt32 | Byte
    pub fn string(&self, name: &str) -> Option<&str>;          // Str | Name | Enum
    pub fn bool(&self, name: &str) -> Option<bool>;
    pub fn guid(&self, name: &str) -> Option<[u8; 16]>;        // Value::Guid
    pub fn datetime(&self, name: &str) -> Option<u64>;         // Value::DateTime (UE ticks)
}
pub struct ParseOptions<'a> {
    /// Full dotted paths (e.g. ".worldSaveData.CharacterSaveParameterMap") whose MapProperty
    /// StructProperty keys are property lists rather than a raw 16-byte Guid.
    pub struct_keys: &'a [&'a str],
    /// Returns true for paths to keep as Value::Raw without decoding (skipped by size).
    pub skip: &'a dyn Fn(&str) -> bool,
}
pub struct Reader<'a> { /* bytes, pos, warnings: Vec<ParseWarning> */ }
impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self;
    pub fn header(&mut self) -> Result<GvasHeader>;
    pub fn properties(&mut self, path: &str, opts: &ParseOptions) -> Result<Props>; // until "None"
    pub fn into_warnings(self) -> Vec<ParseWarning>;
}
/// Convenience: header + top-level properties.
pub fn parse(data: &[u8], opts: &ParseOptions) -> Result<(GvasHeader, Props, Vec<ParseWarning>)>;
```

Wire format. This is the algorithm the signatures do not determine. It was verified on the fixtures with UE 5.1.1 and save game version 3.
- FString: `i32` len. `0` = empty. `>0` = len bytes of Latin-1/UTF-8 including the trailing NUL. `<0` = `-len` UTF-16LE units including the trailing NUL.
- Header: `"GVAS"`, `i32` save_game_version, `u32` ue4 version, `u32` ue5 version if version ≥ 3, `u16 u16 u16` engine major/minor/patch, `u32` build, FString branch, `i32` custom version format, `i32` count, then count × (16-byte guid + `i32`), then FString save class.
- Property tag: FString name (`"None"` ends the list), FString type, `u64` size, then type-specific header, then an optional-guid byte (`1` → 16 more bytes), then exactly `size` bytes of payload. Type-specific headers:
  - Struct: FString struct type + 16 bytes.
  - Array/Set/Byte/Enum: FString inner type or enum name.
  - Map: FString key type + FString value type.
  - Bool: `u8` value, and its payload `size` is 0.
- Payloads:
  - Int `i32`, Int64 `i64`, UInt32 `u32`, Float `f32`, Str/Name FString.
  - Enum: FString.
  - Byte: `u8` if the enum name is `"None"`, else FString (→ `Enum`).
  - Struct: `Guid` → 16 bytes, `DateTime` → `u64`, `Vector`/`Quat`/`LinearColor` and any other fixed-size math struct → `Raw` of `size`. Any other struct type → nested `properties`.
  - Array: `u32` count. Inner `ByteProperty` → `Bytes` (count bytes). Inner `StructProperty` → inner tag (FString name, FString type, `u64`, FString struct type, 16 bytes, 1 byte) then count struct values. Inner `NameProperty`/`EnumProperty`/`StrProperty` → count FStrings. Inner `IntProperty` → count `i32`. Anything else → `Raw`.
  - Map: `u32` removed-count (ignored), `u32` count, then count × (key, value). Element encodings:
    - Name/Enum/Str → FString; Int → `i32`; Int64 → `i64`; Bool → `u8`.
    - StructProperty key → 16-byte `Guid`, unless the map path is in `opts.struct_keys`, in which case it is a property list.
    - StructProperty value → property list.
  - SetProperty → `Raw`.
- **Containment rule:** every tag carries its payload `size`. Read the payload as a sub-slice and decode it in isolation. On an unknown type, a decode error, or a payload not fully consumed, store `Value::Raw(payload)`, push a `ParseWarning { path, message }`, and continue after the payload. Paths are dotted, like `".worldSaveData.GroupSaveDataMap"`, for nested properties: parent path + `"." + name`. Map values use parent path + `".Value"`.

Test helpers: a `#[cfg(test)] mod build` in `gvas.rs` with `fstring(&mut Vec<u8>, &str)`, `tag(&mut Vec<u8>, name, type, extra_header: &[u8], payload: &[u8])` (writes size and guid flag 0), `none(&mut Vec<u8>)`, `header_bytes() -> Vec<u8>` (minimal valid header, version 3, 0 custom versions, class `"/Script/Test"`). Task 4 reuses these, so make it `pub(crate)`.

- [ ] **Step 1: Write failing unit tests** in `gvas.rs`:
  - `header_parses`: `save_class == "/Script/Test"`.
  - `scalar_properties`: Int 42, Int64 −5, Str `"héllo"` (UTF-8), Name, Bool true (value in the header), Byte (`None` enum) 80, Enum `"EPalX::Y"`. Each is retrievable via `Props::int`/`string`/`bool`.
  - `utf16_string`: a negative-length FString decodes `"Lóutre✓"`.
  - `nested_struct_and_guid`: a Struct `"Foo"` containing a Guid struct, read back with `struct_props("S").unwrap().guid("G")`.
  - `map_name_to_int_and_bool`: a `Map<Name,Int>` with 2 entries and a `Map<Name,Bool>`.
  - `map_struct_key_default_guid_vs_struct_keys`: the same map bytes, parsed with and without the path in `struct_keys`.
  - `byte_array_becomes_bytes`.
  - `unknown_property_type_becomes_raw`: a tag with type `"WeirdProperty"` and a 3-byte payload is followed by Int `"After"`=7. Expect `Raw(vec![..3])`, `int("After") == Some(7)`, and exactly 1 warning whose path ends with the weird property name.
  - `skip_predicate_keeps_raw_without_warning`.
  - `truncated_input_is_err_not_panic`: cut a valid buffer at every length from 0 to len−1. `parse` returns `Err` or `Ok` and never panics.
- [ ] **Step 2: Run `cargo test --lib gvas`**. Expected: FAIL (compile).
- [ ] **Step 3: Implement `gvas.rs`.**
- [ ] **Step 4: Run `cargo test --lib gvas`**. Expected: all pass.
- [ ] **Step 5: Commit** `feat(save): generic GVAS property reader with size-bounded containment`

---

### Task 4: Palworld RawData decoders

**Files:**
- Create: `src/save/rawdata.rs`

**Interfaces:**
- Consumes: `gvas::{Reader, Props, ParseOptions}`, `ids::{PlayerUid, GuildId, guid_to_string}`.
- Produces:

```rust
pub struct CharacterRaw { pub props: Props /* contains "SaveParameter" */, pub group_id: [u8; 16] }
pub fn decode_character(bytes: &[u8]) -> Result<CharacterRaw>;
// bytes = properties until "None", then 4 unknown bytes, then a 16-byte group guid; must reach EOF.

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GuildMember { pub uid: PlayerUid, pub name: String, pub last_online_ticks: i64 }
pub enum GroupRaw {
    Guild { id: GuildId, name: String, base_camp_level: i32, base_count: u32, admin: PlayerUid, members: Vec<GuildMember> },
    Other, // Neutral, IndependentGuild, Organization: not reported
}
pub fn decode_group(group_type: &str, bytes: &[u8]) -> Result<GroupRaw>;
```

Group byte layout (from palworld-save-tools `rawdata/group.py`, current main):
1. `guid` group_id, FString group_name, `u32` n × (guid, guid) character handles.
2. If the type is Guild, IndependentGuild or Organization: `u8` org_type, `u32` n × guid base_ids (`base_count = n`).
3. If Guild or IndependentGuild: `i32` base_camp_level, `u32` n × guid, FString guild_name.
4. If Guild: guid admin_player_uid, `i32` count × (guid player_uid, `i64` last_online, FString player_name).
5. Must reach EOF, else `Err`.

`group_type` values look like `"EPalGroupType::Guild"`. Report `name = guild_name`, falling back to `group_name` if `guild_name` is empty.

- [ ] **Step 1: Write failing unit tests** using the Task 3 `build` helpers:
  - `character_roundtrip`: SaveParameter struct with Level Byte 80, Exp Int64 45859908, NickName "RereBleue", IsPlayer true, plus 4 bytes and a group guid. Assert the fields and `group_id`.
  - `character_trailing_garbage_is_err`.
  - `guild_roundtrip`: Guild with 2 members and 3 base ids, `base_camp_level` 12. Assert `base_count == 3`, admin, member names, and `name`.
  - `non_guild_is_other`: `"EPalGroupType::Neutral"` with only part 1 → `GroupRaw::Other`.
- [ ] **Step 2: Run `cargo test --lib rawdata`**. Expected: FAIL.
- [ ] **Step 3: Implement** (use `gvas::Reader` for the property part and add bounded `guid`/`fstring`/`i32` reads on `Reader` as `pub(crate)`).
- [ ] **Step 4: Run.** Expected: pass.
- [ ] **Step 5: Commit** `feat(save): decode Palworld character and group RawData`

---

### Task 5: Player save parser

**Files:**
- Create: `src/save/player.rs`
- Modify: `tests/fixtures_save.rs`

**Interfaces:**
- Consumes: `decompress`, `gvas::parse`, `PlayerUid`, `ParseWarning`.
- Produces:

```rust
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RecordStats {
    pub pal_capture_total: Option<u64>,          // sum of PalCaptureCount (Map<Name,Int>)
    pub pal_capture_by_species: BTreeMap<String, i32>, // PalCaptureCount entries
    pub species_captured: Option<i32>,           // TribeCaptureCount (Int)
    pub paldeck_unlocked: Option<u32>,           // count of true in PaldeckUnlockFlag
    pub normal_boss_defeated: Option<u32>,       // count of true in NormalBossDefeatFlag
    pub tower_boss_defeated: Option<u32>,        // count of true in TowerBossDefeatFlag
    pub tower_boss_defeat_total: Option<u64>,    // sum TowerBossDefeatCount
    pub raid_boss_defeat_total: Option<u64>,     // sum RaidBossDefeatCount
    pub predator_defeated: Option<i32>,          // PredatorDefeatCount
    pub pal_butcher_total: Option<u64>,          // sum PalButcherCount
    pub pal_rankup_total: Option<u64>,           // sum PalRankupCount
    pub mutation_count: Option<i32>,             // MutationCount
    pub awakening_count: Option<i32>,            // AwakeningCount
    pub camp_conquered: Option<i32>,             // CampConqueredCount
    pub normal_dungeon_clear: Option<i32>,       // NormalDungeonClearCount
    pub fixed_dungeon_clear: Option<i32>,        // FixedDungeonClearCount
    pub oilrig_clear: Option<i32>,               // OilrigClearCount
    pub found_treasure: Option<i32>,             // FoundTreasureCount
    pub relic_possess: Option<i32>,              // RelicPossessNum
    pub arena_solo_clear_total: Option<u64>,     // sum ArenaSoloClearCount
    pub fishing_total: Option<u64>,              // sum FishingCountMap
    pub craft_item_total: Option<u64>,           // sum CraftItemCount
    pub fast_travel_unlocked: Option<u32>,       // count of true in FastTravelPointUnlockFlag
    pub game_cleared: Option<bool>,              // bIsGameCleared
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlayerRecord {
    pub uid: PlayerUid,                          // SaveData.PlayerUId
    pub last_online: Option<chrono::DateTime<chrono::Utc>>, // SaveData.LastOnlineDateTime (UE ticks)
    pub platform_hint: Option<String>,           // SaveData.PlayerPlatform enum value, raw
    pub technology_points: Option<i32>,          // TechnologyPoint
    pub boss_technology_points: Option<i32>,     // bossTechnologyPoint
    pub unlocked_technologies: Option<u32>,      // len UnlockedRecipeTechnologyNames
    pub completed_quests: Option<u32>,           // len CompletedQuestArray_FullRelease
    pub records: RecordStats,                    // SaveData.RecordData
}
pub fn parse_player_save(raw: &[u8]) -> Result<(PlayerRecord, Vec<ParseWarning>)>;
```

UE ticks → UTC: `(ticks − 621_355_968_000_000_000) / 10_000_000` seconds since the Unix epoch. Ticks ≤ that offset → `None`. Each missing field pushes a warning `{ path: ".SaveData.RecordData.<Name>", message: "missing" }`. A missing `PlayerUId` is an `Err(Gvas)`.

- [ ] **Step 1: Write failing integration tests** (fixture player `3C36EC75000000000000000000000000.sav`, values read from the real file):

```rust
#[test] fn parses_real_player_record() {
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
#[test] fn every_real_player_file_parses() { /* all Players/*.sav except *_dps.sav parse Ok; uid == file stem */ }
```

- [ ] **Step 2: Run `cargo test --release --test fixtures_save player`**. Expected: FAIL.
- [ ] **Step 3: Implement** (`ParseOptions { struct_keys: &[], skip: &|_| false }`. The player file is about 100 KB, so decode it fully).
- [ ] **Step 4: Run.** Expected: pass.
- [ ] **Step 5: Commit** `feat(save): parse player save RecordData`

---

### Task 6: Level save parser

**Files:**
- Create: `src/save/level.rs`
- Modify: `tests/fixtures_save.rs`

**Interfaces:**
- Consumes: `decompress`, `gvas::{Reader, ParseOptions, Props, Value}`, `rawdata::{decode_character, decode_group, GroupRaw, GuildMember}`, ids.
- Produces:

```rust
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CharacterInfo { pub uid: PlayerUid, pub name: String, pub level: Option<u8>, pub exp: Option<i64>, pub arena_rank_point: Option<i32>, pub group_id: Option<GuildId> }
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GuildInfo { pub id: GuildId, pub name: String, pub admin: PlayerUid, pub members: Vec<GuildMember>, pub base_count: u32, pub base_camp_level: i32 }
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct LevelSave { pub players: Vec<CharacterInfo>, pub owned_pals: BTreeMap<PlayerUid, u32>, pub guilds: Vec<GuildInfo> }
pub fn parse_level_save(raw: &[u8]) -> Result<(LevelSave, Vec<ParseWarning>)>;
```

Algorithm:
1. Decompress, read the header, then the top level with `ParseOptions`:
   - `struct_keys = &[".worldSaveData.CharacterSaveParameterMap"]`;
   - `skip` = true for every direct child of `.worldSaveData` except `CharacterSaveParameterMap` and `GroupSaveDataMap`. This avoids decoding `MapObjectSaveData` (44 MB) and the others.
2. `CharacterSaveParameterMap` entries: the value struct has `RawData` (`Bytes`). Run `decode_character` on it, then take `SaveParameter`.
   - `IsPlayer == true` → `CharacterInfo`, with `uid` from the key's `PlayerUId` guid, `name` = `NickName`, `level` = `Level` (accept Byte or Int), `exp` = `Exp`, `arena_rank_point` = `ArenaRankPoint`, and `group_id` from `CharacterRaw.group_id` (`None` if all zero).
   - Otherwise, if `OwnerPlayerUId` is a non-nil guid, increment `owned_pals[owner]`.
   - A per-entry decode failure becomes a warning, and the loop continues.
3. `GroupSaveDataMap` entries: the value struct has `GroupType` (Enum string) and `RawData` (`Bytes`). Run `decode_group`. Keep `GroupRaw::Guild` as `GuildInfo`. A per-entry failure becomes a warning.

Deviation from spec §5.4, deliberate: base count and base level come from the guild RawData (`base_ids`, `base_camp_level`), so `BaseCampSaveData` and `GameTimeSaveData` are skipped. The spec marked the in-game day as optional, and it is dropped.

- [ ] **Step 1: Write failing integration tests:**

```rust
#[test] fn parses_real_level() {
    let Some(w) = world() else { return };
    let (l, warnings) = palworld_stats::save::parse_level_save(&std::fs::read(w.join("Level.sav")).unwrap()).unwrap();
    assert_eq!(l.players.len(), 6);
    let rere = l.players.iter().find(|c| c.uid.as_str() == "3C36EC75000000000000000000000000").unwrap();
    assert_eq!(rere.name, "RereBleue");
    assert_eq!(rere.level, Some(80));
    assert_eq!(rere.exp, Some(45_859_908));
    assert_eq!(rere.arena_rank_point, Some(585));
    assert!(!l.guilds.is_empty());
    assert!(l.guilds.iter().all(|g| !g.members.is_empty()));
    assert!(l.owned_pals.values().sum::<u32>() > 0);
    assert!(warnings.is_empty(), "{warnings:?}");
}
#[test] fn level_parse_is_fast() { /* parse_level_save on Level.sav completes in < 3 s in --release (Instant) */ }
```

- [ ] **Step 2: Run `cargo test --release --test fixtures_save level`**. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: pass. If `warnings` is non-empty, it points at the property or group whose format differs from this plan. Fix the decoder against the fixture rather than loosening the assertion.
- [ ] **Step 5: Commit** `feat(save): parse Level.sav players, owned pals and guilds`

---

### Task 7: World merge (`parse_world`)

**Files:**
- Create: `src/save/world.rs`
- Modify: `src/save/mod.rs` (re-export everything public from Tasks 2 and 4 to 7), `tests/fixtures_save.rs`

**Interfaces:**
- Consumes: `parse_player_save`, `parse_level_save`, `PlayerRecord`, `CharacterInfo`, `GuildInfo`.
- Produces:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] pub enum GuildRole { Admin, Member }
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlayerStats {
    pub uid: PlayerUid,
    pub name: Option<String>,        // CharacterInfo.name, else a GuildMember name
    pub level: Option<u8>, pub exp: Option<i64>, pub arena_rank_point: Option<i32>,
    pub guild_id: Option<GuildId>, pub guild_role: Option<GuildRole>,
    pub owned_pals: u32,
    pub record: Option<PlayerRecord>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct WorldStats { pub players: Vec<PlayerStats>, pub guilds: Vec<GuildInfo>, pub warnings: Vec<ParseWarning> }
/// `players`: (file stem, raw bytes) from the `Players/` directory, e.g. ("01C37DCB000000000000000000000000", bytes).
/// Stems ending in "_dps" or not parseable as a PlayerUid are ignored. A player file that fails to
/// parse becomes a warning (path = "Players/<stem>"), not an error.
pub fn parse_world<I>(level: &[u8], players: I) -> Result<WorldStats> where I: IntoIterator<Item = (String, Vec<u8>)>;
```

Merge by `PlayerUid`. The output is the union of Level players and player files, sorted by uid. `guild_id`/`guild_role` come from the guild membership (`admin` → `Admin`). Warnings concatenate Level warnings and per-player warnings, the latter prefixed with `Players/<stem>`.

- [ ] **Step 1: Write failing tests.** Unit tests in `world.rs` use a hand-built `LevelSave`/`PlayerRecord` through a private `merge(level: LevelSave, records: Vec<PlayerRecord>, warnings) -> WorldStats`:
  - `orphans_are_kept`: a character without a record → `record: None`. A record without a character → `name: None`, `level: None`.
  - `guild_role_assigned`.
  - `dps_and_unknown_files_are_ignored`: `parse_world` with entries `("X_dps", junk)` and `("LevelMeta", junk)` yields no warnings from them. This needs a real `Level.sav`, so put it in `tests/fixtures_save.rs`.

  Integration test:

```rust
#[test] fn parses_real_world() {
    let Some(w) = world() else { return };
    let level = std::fs::read(w.join("Level.sav")).unwrap();
    let players: Vec<(String, Vec<u8>)> = std::fs::read_dir(w.join("Players")).unwrap()
        .map(|e| e.unwrap().path())
        .map(|p| (p.file_stem().unwrap().to_string_lossy().into_owned(), std::fs::read(&p).unwrap()))
        .collect();
    let ws = palworld_stats::save::parse_world(&level, players).unwrap();
    assert_eq!(ws.players.len(), 6);
    assert!(ws.players.iter().all(|p| p.record.is_some() && p.name.is_some() && p.level.unwrap_or(0) > 0));
    assert!(ws.warnings.is_empty(), "{:?}", ws.warnings);
    serde_json::to_string(&ws).unwrap();
}
```

- [ ] **Step 2: Run `cargo test --release world`**. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run `cargo test --release`** (whole suite). Expected: all pass.
- [ ] **Step 5: Commit** `feat(save): merge level and player saves into WorldStats`

---

### Task 8: REST client

**Files:**
- Create: `src/rest.rs`, `tests/rest.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `Error`, `Result`, `PlayerUid`, `PlatformId`.
- Produces:

```rust
pub struct RestClient { /* reqwest::Client, base_url, password */ }
impl RestClient {
    pub fn new(base_url: impl Into<String>, admin_password: impl Into<String>) -> Result<Self>; // default timeout 10 s
    pub fn with_timeout(self, timeout: std::time::Duration) -> Result<Self>;
    pub async fn info(&self) -> Result<ServerInfo>;       // GET {base}/v1/api/info
    pub async fn players(&self) -> Result<Vec<OnlinePlayer>>; // GET {base}/v1/api/players
    pub async fn metrics(&self) -> Result<ServerMetrics>; // GET {base}/v1/api/metrics
}
#[derive(Debug, Clone, PartialEq, Serialize)] pub struct ServerInfo { pub version: String, pub server_name: String, pub description: String, pub world_guid: String }
#[derive(Debug, Clone, PartialEq, Serialize)] pub struct OnlinePlayer {
    pub name: String, pub account_name: String, pub uid: PlayerUid, pub platform_id: PlatformId,
    pub level: Option<i32>, pub ping: Option<f64>, pub location_x: Option<f64>, pub location_y: Option<f64>, pub building_count: Option<i32>,
}
#[derive(Debug, Clone, PartialEq, Serialize)] pub struct ServerMetrics {
    pub server_fps: Option<f64>, pub current_players: u32, pub server_frame_time: Option<f64>,
    pub max_players: u32, pub uptime_seconds: u64, pub days: Option<u32>, pub basecamp_count: Option<u32>,
}
```

- Authentication: HTTP Basic, user `admin`, password = `admin_password`.
- JSON field names come from the official API:
  - `info`: `version`, `servername`, `description`, `worldguid`.
  - `players`: `{"players":[{name, accountName, playerId, userId, ip, ping, location_x, location_y, level, building_count}]}`.
  - `metrics`: `serverfps`, `currentplayernum`, `serverframetime`, `maxplayernum`, `uptime`, `days`, `basecampnum`.
- Private DTOs use `#[serde(default)]` on optional fields. `ip` is not even declared in the DTO, so serde ignores it.
- Error mapping: 401 → `Unauthorized`; other non-2xx → `Status(code)`; transport/decode → `Http(msg)`.
- A player whose `playerId` fails `PlayerUid::parse` is dropped with `log::warn!`.

- [ ] **Step 1: Write failing tests in `tests/rest.rs`** with `wiremock`:
  - `players_maps_fields_and_drops_ip`: 2 players, one Steam and one `xbox_…`, `playerId` lowercase. Assert `uid`, `platform_id`, `level`, and that `serde_json::to_string(&players)` does not contain the IP string.
  - `basic_auth_is_sent`: mock matching the `Authorization: Basic YWRtaW46c2VjcmV0` header (`admin:secret`).
  - `unauthorized_maps_to_error`: 401 → `Err(Error::Unauthorized)`.
  - `empty_players_is_empty_vec`: `{"players":[]}`.
  - `metrics_without_new_fields`: JSON without `days`/`basecampnum` → `None`.
  - `server_down_is_http_error`: client pointed at `http://127.0.0.1:1` → `Err(Error::Http(_))`.
- [ ] **Step 2: Run `cargo test --test rest`**. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: pass.
- [ ] **Step 5: Commit** `feat(rest): async client for the Palworld REST API`

---

### Task 9: Presence diff and log parsing

**Files:**
- Create: `src/presence.rs`, `src/logs.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `OnlinePlayer` (Task 8), `PlatformId`.
- Produces:

```rust
#[derive(Debug, Clone, PartialEq, Serialize)] pub enum PresenceEvent { Joined(OnlinePlayer), Left(OnlinePlayer) }
/// Keyed by uid. Joined events in `current` order, then Left events in `previous` order. No clock: the caller timestamps.
pub fn diff_presence(previous: &[OnlinePlayer], current: &[OnlinePlayer]) -> Vec<PresenceEvent>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] pub enum LogEventKind { Joined, Left }
#[derive(Debug, Clone, PartialEq, Serialize)] pub struct LogEvent { pub kind: LogEventKind, pub name: String, pub platform_id: PlatformId, pub timestamp: Option<chrono::NaiveDateTime> }
pub fn parse_log_line(line: &str) -> Option<LogEvent>;
```

Log regex (compiled once in a `std::sync::LazyLock`). The leading timestamp is optional; any other prefix (the docker log prefix, `[LOG]`, ANSI codes) is tolerated by not anchoring the event part:
`(?:\[(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})\])?.*?\s?(.+?) (joined|left) the server\.? \(User id: ([^)]+)\)`
The timestamp is parsed with `%Y-%m-%d %H:%M:%S`. The name is trimmed. A name containing `" joined the server"` is handled by the non-greedy capture plus the required `(User id: …)` suffix.

- [ ] **Step 1: Write failing unit tests:**
  - `presence_join_and_leave`: previous [A, B], current [B, C] → `[Joined(C), Left(A)]`.
  - `presence_no_change_is_empty`.
  - `log_join_with_timestamp`: `"[2026-10-22 21:03:11] [LOG] RereBleue joined the server. (User id: steam_76561198000000000)"` → `Joined`, name `"RereBleue"`, `Steam("76561198000000000")`, timestamp `2026-10-22T21:03:11`.
  - `log_left_xbox_no_timestamp`: `"[LOG] Loutre Grise left the server. (User id: xbox_2535400000000000)"` → `Left`, name with a space, Xbox id, `timestamp: None`.
  - `log_unrelated_line_is_none`: `"[LOG] Server saved"` → `None`.
- [ ] **Step 2: Run `cargo test --lib presence logs`**. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: pass.
- [ ] **Step 5: Ask the user for 2 real lines from `docker logs palworld-vanilla`** (one join and one leave), add them as a test `log_real_lines`, and adjust the regex if needed. If the user is unavailable, leave the test marked `#[ignore = "needs real log lines"]` and say so in the final report.
- [ ] **Step 6: Commit** `feat: presence diff and join/leave log parsing`

---

### Task 10: PocketBase models and README

**Files:**
- Create: `src/pocketbase.rs`, `README.md`
- Modify: `src/lib.rs` (`#[cfg(feature = "pocketbase")] pub mod pocketbase;`)

**Interfaces:**
- Consumes: `PlayerStats`, `GuildInfo`, `PlayerUid`.
- Produces (all `#[derive(Debug, Clone, Serialize, Deserialize)]`; ids and relations are `String`; dates are RFC 3339 `String`; `#[serde(default, skip_serializing_if = "Option::is_none")] id: Option<String>`). These match spec §7 field for field:
  - `PalworldCharacter { id, server, player: Option<String>, player_uid, name, level, exp, guild: Option<String>, last_online: Option<String>, first_seen: String }`
  - `PalworldStats { id, character, server, playtime_seconds: u64, updated_at: String, /* + every RecordStats field flattened, plus level, exp, owned_pals, arena_rank_point, technology_points, boss_technology_points, unlocked_technologies, completed_quests */ }`. Do not store `pal_capture_by_species`.
  - `PalworldSession { id, server, character, started_at: String, ended_at: Option<String>, duration_seconds: Option<u64>, source: SessionSource }` with `#[serde(rename_all = "lowercase")] enum SessionSource { Log, Poll }`.
  - `PalworldStatsSnapshot { id, character, server, taken_at, playtime_seconds, level, exp, pal_capture_count, paldeck_unique, boss_defeated }`. Here `boss_defeated` = `normal_boss_defeated + tower_boss_defeated`.
  - `PalworldGuild { id, server, guild_id, name, admin: Option<String>, member_count: u32, base_count: u32, base_camp_level: i32, updated_at: String }`
  - Constructors: `PalworldStats::from_player(server: &str, character: &str, p: &PlayerStats, playtime_seconds: u64, now: DateTime<Utc>) -> Self`, `PalworldStatsSnapshot::from_player(server, character, p, playtime_seconds, now) -> Self`, `PalworldGuild::from_guild(server, &GuildInfo, admin_character: Option<String>, now) -> Self`, and `PalworldStatsSnapshot::differs_from(&self, other: &Self) -> bool` (compares all fields except `id`/`taken_at`).

- [ ] **Step 1: Write failing tests** (`cargo test --features pocketbase`):
  - `stats_from_player_flattens_records`: build a `PlayerStats` with `records.species_captured = Some(257)`; the JSON has `"species_captured":257` and no `pal_capture_by_species`.
  - `session_source_serializes_lowercase`: `"source":"log"`.
  - `snapshot_differs`: identical → false; level changed → true; only `taken_at` changed → false.
- [ ] **Step 2: Run `cargo test --features pocketbase pocketbase`**. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Write `README.md`** (French, matching the project's audience). It covers:
  - what the crate provides;
  - the Otternel integration loop from spec §8, with a code sample using `RestClient`, `diff_presence`, `parse_log_line` and `tokio::task::spawn_blocking(move || parse_world(&level, players))`;
  - the fetch caveat: download `<save_path>/Level.sav` and `<save_path>/Players` **separately**. `DockerFetcher::fetch_files_by_extension` keys by bare file name, so fetching the whole world folder would mix in `backup/world/*/Level.sav`;
  - the `PlayerUid ↔ PlatformId` link: it is only known from REST/logs while the player is online, and Otternel must persist it;
  - the PocketBase collections table from spec §7;
  - the stats that are not available (deaths, kills, distance, eggs hatched).
- [ ] **Step 5: Run `cargo test --all-features --release && cargo clippy --all-features -- -D warnings && cargo doc --no-deps --all-features`**. Expected: tests pass, no clippy warnings, docs build.
- [ ] **Step 6: Commit** `feat: PocketBase models and README`
