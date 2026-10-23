# palworld-stats — Guide d'utilisation

## Démarrage rapide

`palworld-stats` lit les statistiques d'un serveur Palworld dédié depuis trois sources : l'API REST, les logs et les sauvegardes `.sav`. C'est une bibliothèque sans état : elle ne lit aucun fichier et n'écrit nulle part, vous lui passez des octets ou des chaînes.

**1. Ajouter la dépendance** (Rust 1.88 minimum, édition 2024) :

```toml
[dependencies]
palworld-stats = { git = "https://github.com/matheo-1712/palworld-stats-crate" }
tokio = { version = "1", features = ["full"] }
```

Ajoutez `features = ["pocketbase"]` si vous voulez les structs prêtes pour PocketBase.

**2. Lire un monde et ses joueurs connectés** :

```rust
use palworld_stats::{parse_world, RestClient};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Sauvegardes : Level.sav + chaque fichier de Players/
    let world = Path::new("/palworld/Pal/Saved/SaveGames/0/86150974C2A144B79C51A0354CCBE1FD");
    let level = std::fs::read(world.join("Level.sav"))?;
    let mut files = Vec::new();
    for e in std::fs::read_dir(world.join("Players"))? {
        let path = e?.path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        files.push((name, std::fs::read(&path)?));
    }
    let stats = tokio::task::spawn_blocking(move || parse_world(&level, files)).await??;
    for p in &stats.players {
        println!("{:?} niveau {:?}, {} Pals", p.name, p.level, p.owned_pals);
    }

    // API REST : joueurs en ligne
    let rest = RestClient::new("http://palworld-vanilla:8212", "mot-de-passe-admin")?;
    for p in rest.players().await? {
        println!("en ligne : {} ({})", p.name, p.platform_id);
    }
    Ok(())
}
```

Tout l'API utile est réexporté à la racine du crate : `use palworld_stats::{...}`.

## Concepts

**Trois identifiants.** Un joueur Palworld a deux identités qui ne se croisent jamais dans les sauvegardes : son UID interne et son compte de plateforme.

| Type | Exemple | Source | Méthodes |
| --- | --- | --- | --- |
| `PlayerUid` | `3C36EC75000000000000000000000000` | sauvegardes (nom de `Players/<UID>.sav`), REST `playerId` | `parse(&str)`, `from_guid(&[u8;16])`, `as_str()`, `is_nil()`, `Display` |
| `PlatformId` | `Steam("76561198…")`, `Xbox("2535…")`, `Other("psn_1")` | REST `userId`, logs « User id » | `From<&str>`, `Display` → `steam_…` / `xbox_…` |
| `GuildId` | 32 caractères hexadécimaux | `Level.sav` | `from_guid`, `as_str()`, `Display` |

`PlayerUid::parse` accepte majuscules ou minuscules, tirets, et la forme courte (`3c36ec75`) : tout est normalisé en 32 caractères majuscules. Deux écritures du même UID sont donc égales. Les sauvegardes ne contiennent **jamais** le Steam ID : le lien `PlayerUid` ↔ `PlatformId` s'apprend quand le joueur est en ligne, et c'est à vous de le conserver.

**Valeurs absentes = valeur par défaut.** Unreal n'écrit pas une propriété restée à sa valeur par défaut. Quand un compteur manque dans une sauvegarde, la crate renvoie `Some(0)` (et `Some(1)` pour le niveau). `None` signifie autre chose : la donnée est vraiment inconnue (fichier joueur absent, type inattendu).

**Erreurs ou avertissements.** Une fonction renvoie `Err` seulement quand rien n'est exploitable : serveur injoignable, `Level.sav` illisible. Un problème local (un fichier joueur corrompu, un champ au type inattendu) devient un `ParseWarning { path, message }` dans `WorldStats.warnings`, et le reste est quand même renvoyé.

**Synchrone ou async.** Le client REST est async (tokio). Le parsing des sauvegardes est synchrone et utilise le CPU environ 0,2 s par monde : appelez-le dans `tokio::task::spawn_blocking`.

## Client REST

`RestClient` interroge l'API officielle du serveur dédié (`/v1/api/info`, `/players`, `/metrics`) avec l'authentification HTTP Basic `admin:<ADMIN_PASSWORD>`. Il est `Clone + Send + Sync` : créez-en un par serveur et partagez-le.

```rust
use palworld_stats::RestClient;
use std::time::Duration;

let rest = RestClient::new("http://palworld-vanilla:8212", admin_password)?   // slash final accepté
    .with_timeout(Duration::from_secs(5))?;                                   // 10 s par défaut

let info = rest.info().await?;
let players = rest.players().await?;
let metrics = rest.metrics().await?;
```

Le `Debug` du client masque le mot de passe (`<redacted>`), et les messages d'erreur ne contiennent jamais l'URL.

**`ServerInfo`** — `info()`

| Champ | Type | Contenu |
| --- | --- | --- |
| `version` | `String` | version du serveur |
| `server_name` | `String` | nom du serveur |
| `description` | `String` | description du serveur |
| `world_guid` | `String` | identifiant du monde (nom du dossier de sauvegarde) |

**`OnlinePlayer`** — un élément de `players()`

| Champ | Type | Contenu |
| --- | --- | --- |
| `name` | `String` | nom du personnage |
| `account_name` | `String` | nom du compte de plateforme |
| `uid` | `PlayerUid` | UID interne, identique à celui des sauvegardes |
| `platform_id` | `PlatformId` | Steam ou Xbox |
| `level` | `Option<i32>` | niveau actuel |
| `ping` | `Option<f64>` | ping en ms |
| `location_x`, `location_y` | `Option<f64>` | position sur la carte |
| `building_count` | `Option<i32>` | constructions posées |

L'IP du joueur n'est jamais lue. Les joueurs encore en chargement (UID nul) et ceux dont l'UID est invalide sont ignorés.

**`ServerMetrics`** — `metrics()`

| Champ | Type | Contenu |
| --- | --- | --- |
| `server_fps` | `Option<f64>` | images par seconde du serveur |
| `server_frame_time` | `Option<f64>` | durée d'une image en ms |
| `current_players` / `max_players` | `u32` | joueurs connectés / maximum |
| `uptime_seconds` | `u64` | temps depuis le démarrage |
| `days` | `Option<u32>` | jour en jeu (versions récentes du serveur) |
| `basecamp_count` | `Option<u32>` | nombre de bases |

## Présence et logs

Deux fonctions pures servent à construire des sessions de jeu ; aucune ne lit l'horloge, c'est vous qui horodatez.

**`diff_presence(previous, current) -> Vec<PresenceEvent>`** compare deux résultats de `players()` par `uid`. Elle renvoie d'abord les `Joined(OnlinePlayer)` (ordre de `current`), puis les `Left(OnlinePlayer)` (ordre de `previous`). Un joueur qui change de nom sans changer d'UID ne produit aucun événement.

```rust
use palworld_stats::{diff_presence, PresenceEvent};

let current = rest.players().await?;
for ev in diff_presence(&previous, &current) {
    match ev {
        PresenceEvent::Joined(p) => println!("{} arrive ({})", p.name, p.platform_id),
        PresenceEvent::Left(p) => println!("{} part", p.name),
    }
}
previous = current;
```

**`parse_log_line(&str) -> Option<LogEvent>`** reconnaît les lignes de connexion et de déconnexion du serveur et renvoie `None` pour toute autre ligne. Elle tolère les préfixes Docker, les codes couleur et le tag `[LOG]`, et ne panique sur aucune entrée.

```rust
use palworld_stats::{parse_log_line, LogEventKind};

let line = "[2026-10-22 21:03:11] [LOG] RereBleue joined the server. (User id: steam_76561198000000000)";
let ev = parse_log_line(line).unwrap();
assert_eq!(ev.kind, LogEventKind::Joined);
assert_eq!(ev.name, "RereBleue");
assert_eq!(ev.platform_id.to_string(), "steam_76561198000000000");
```

| Champ de `LogEvent` | Type | Contenu |
| --- | --- | --- |
| `kind` | `LogEventKind` | `Joined` ou `Left` |
| `name` | `String` | nom du personnage |
| `platform_id` | `PlatformId` | compte Steam ou Xbox |
| `timestamp` | `Option<NaiveDateTime>` | heure de la ligne, **locale au conteneur et sans fuseau** ; `None` si absente ou invalide |

`LogEvent` ne contient pas d'UID : retrouvez le personnage grâce au lien `PlatformId` → `PlayerUid` appris par `players()`. Le format exact des lignes n'a pas encore été vérifié sur de vrais logs (test `log_real_lines` en attente).

## Sauvegardes : `parse_world`

```rust
pub fn parse_world<I>(level: &[u8], players: I) -> Result<WorldStats>
where I: IntoIterator<Item = (String, Vec<u8>)>
```

`level` est le contenu brut de `Level.sav`. `players` est la liste (nom du fichier, contenu) du dossier `Players/`, avec ou sans l'extension `.sav`. Les fichiers `*_dps.sav` (stockage de Pals), `Level` et `LevelMeta` sont ignorés ; tout autre nom qui n'est pas un UID produit un avertissement. Formats acceptés : `PlM` (Oodle, sauvegardes actuelles) et `PlZ` (zlib, anciennes).

**`WorldStats`** : `players: Vec<PlayerStats>` (triés par UID), `guilds: Vec<GuildInfo>`, `warnings: Vec<ParseWarning>`. Un joueur présent dans `Level.sav` sans fichier joueur, ou l'inverse, est quand même listé avec les champs manquants à `None`.

**`PlayerStats`** — un joueur, fusion de `Level.sav` et de son fichier

| Champ | Type | Contenu |
| --- | --- | --- |
| `uid` | `PlayerUid` | UID du joueur |
| `name` | `Option<String>` | nom du personnage |
| `level` | `Option<u8>` | niveau (1 si absent de la sauvegarde) |
| `exp` | `Option<i64>` | expérience totale |
| `arena_rank_point` | `Option<i32>` | points de classement d'arène |
| `guild_id` / `guild_role` | `Option<GuildId>` / `Option<GuildRole>` | guilde et rôle (`Admin` ou `Member`) |
| `owned_pals` | `u32` | Pals dont le joueur est propriétaire dans Level.sav |
| `pals` | `Vec<OwnedPal>` | détail de chaque Pal possédé (voir plus bas) |
| `pal_summary` | `PalSummary` | agrégats sur ces Pals, prêts pour un classement |
| `status_points` / `ex_status_points` | `BTreeMap<String, i32>` | points de statut attribués par stat (noms internes japonais du jeu, ex. `最大HP`) |
| `record` | `Option<PlayerRecord>` | données du fichier joueur, `None` s'il manque |

**`PlayerRecord`** — contenu de `Players/<UID>.sav`

| Champ | Type | Contenu |
| --- | --- | --- |
| `last_online` | `Option<DateTime<Utc>>` | dernière connexion |
| `platform_hint` | `Option<String>` | plateforme déclarée (sans l'identifiant) |
| `technology_points` / `boss_technology_points` | `Option<i32>` | points de technologie disponibles |
| `unlocked_technologies` | `Option<u32>` | recettes débloquées |
| `unlocked_technology_names` | `Vec<String>` | noms des recettes débloquées |
| `completed_quests` | `Option<u32>` | quêtes terminées |
| `quests_in_progress` | `Option<u32>` | quêtes en cours |
| `skins` | `BTreeMap<String, i32>` | skins possédés (nom → quantité) |
| `records` | `RecordStats` | compteurs ci-dessous |

**`RecordStats`** — compteurs de progression (propriété Palworld entre parenthèses)

| Champ | Contenu |
| --- | --- |
| `pal_capture_total` | Pals capturés au total (somme de `PalCaptureCount`) |
| `pal_capture_by_species` | captures par espèce (`PalCaptureCount`) |
| `species_captured` | espèces différentes capturées (`TribeCaptureCount`) |
| `paldeck_unlocked` | entrées du Paldeck débloquées (`PaldeckUnlockFlag`) |
| `normal_boss_defeated` | boss de terrain vaincus (`NormalBossDefeatFlag`) |
| `tower_boss_defeated` / `tower_boss_defeat_total` | boss de tour différents / victoires au total |
| `raid_boss_defeat_total` | boss de raid vaincus (`RaidBossDefeatCount`) |
| `predator_defeated` | Pals prédateurs vaincus (`PredatorDefeatCount`) |
| `pal_butcher_total` | Pals abattus (`PalButcherCount`) |
| `pal_rankup_total` | améliorations de rang (`PalRankupCount`) |
| `mutation_count` / `awakening_count` | mutations / éveils |
| `camp_conquered` | camps ennemis conquis |
| `normal_dungeon_clear` / `fixed_dungeon_clear` | donjons aléatoires / fixes terminés |
| `oilrig_clear` | plateformes pétrolières terminées |
| `found_treasure` | trésors trouvés |
| `relic_possess` | reliques possédées |
| `arena_solo_clear_total` | victoires en arène solo |
| `fishing_total` | poissons pêchés |
| `craft_item_total` | objets produits (inclut sans doute la production des bases) |
| `fast_travel_unlocked` | points de voyage rapide débloqués |
| `game_cleared` | jeu terminé (`bIsGameCleared`) |
| `areas_discovered` | zones de la carte découvertes (`FindAreaFlagMap`) |
| `area_barriers_unlocked` | barrières de zone déverrouillées |
| `world_maps_unlocked` | morceaux de carte du monde débloqués |
| `notes_found` | notes de journal trouvées |
| `relics_picked_up` | reliques ramassées sur la carte |
| `items_picked_up` | objets uniques ramassés sur la carte |
| `npc_talks_total` | conversations avec des PNJ |
| `npc_achievements` | récompenses de PNJ obtenues |
| `emotes_learned` | emotes apprises auprès de PNJ |
| `specific_boss_defeat_total` | boss spéciaux vaincus (`SpecificBossDefeatFlag`) |

Tous ces champs sont des `Option` et valent `Some(0)` quand la propriété est absente.

**Détails par clé** (`BTreeMap<String, i32>`, vides quand la propriété est absente) :

| Champ | Clé | Contenu |
| --- | --- | --- |
| `pal_capture_by_species` | espèce | captures |
| `capture_bonus_by_species` | espèce | captures ayant donné un bonus d'XP |
| `butcher_by_species` | espèce | Pals abattus |
| `rankup_by_rank` | rang atteint (`"2"` à `"5"`) | condensations |
| `fishing_by_species` | poisson | prises |
| `craft_by_item` | objet | quantités produites |
| `tower_boss_defeats` / `raid_boss_defeats` | boss | victoires |
| `arena_solo_clears` | rang d'arène | victoires |
| `relics_by_type` | type de relique (`CapturePower`, …) | reliques possédées |
| `specific_boss_defeats` | boss spécial | victoires |

**`OwnedPal`** — un Pal possédé (lu dans `Level.sav`)

| Champ | Type | Contenu |
| --- | --- | --- |
| `species` | `String` | identifiant d'espèce (`CharacterID`), ex. `WoolFox` ; un alpha garde son préfixe `BOSS_` |
| `is_alpha` / `is_lucky` | `bool` | alpha capturé / Pal lucky (shiny, `IsRarePal`) |
| `level` / `exp` | `u8` / `i64` | niveau (1 si absent) et expérience |
| `nickname` / `gender` | `Option<String>` | surnom, `Male` ou `Female` |
| `rank` | `u8` | rang de condensation, 1 (base) à 5 (4 étoiles) |
| `soul_hp` / `soul_attack` / `soul_defense` / `soul_craft_speed` | `u8` | améliorations par les âmes |
| `talent_hp` / `talent_attack` / `talent_defense` | `u8` | talents (IV), 0 à 100 |
| `passives` | `Vec<String>` | compétences passives |
| `skills_learned` | `u32` | compétences actives apprises |
| `awakened` | `bool` | Pal éveillé |
| `skin` | `Option<String>` | skin appliqué |
| `friendship` | `i32` | points d'amitié |
| `obtained_at` | `Option<DateTime<Utc>>` | date d'obtention |
| `location` | `Option<PalLocation>` | `Party` (équipe), `Box` (Palbox) ou `Other` ; `None` si le fichier joueur manque |
| `container_id` / `slot_index` | `Option<String>` / `Option<i32>` | conteneur et emplacement ; ordre de l'équipe pour les Pals de l'équipe |

**Équipe du joueur.** `PlayerStats::party()` renvoie les Pals de l'équipe dans l'ordre (5 au maximum), tels qu'au moment de la sauvegarde. L'emplacement est déduit des conteneurs `OtomoCharacterContainerId` (équipe) et `PalStorageContainerId` (Palbox) du fichier joueur.

**`PalSummary`** — agrégats par joueur : `total`, `unique_species` (un alpha compte comme son espèce de base), `lucky`, `alpha`, `max_level`, `average_level` (`None` sans Pal), `max_rank` (Pals au rang 5), `awakened`, `perfect_talents` (100 dans les trois talents), `four_passives`, `in_party`, `in_box`.

**`parse_level_meta(&[u8]) -> Result<LevelMeta>`** lit `LevelMeta.sav` : `world_name` (nom du monde) et `in_game_day` (jour en jeu).

**`GuildInfo`** : `id`, `name`, `admin: PlayerUid`, `members: Vec<GuildMember>`, `base_count`, `base_camp_level`. **`GuildMember`** : `uid`, `name`, `last_online_ticks` — ces ticks sont relatifs au serveur, ne les convertissez pas en date (utilisez `PlayerRecord::last_online`).

Non disponibles dans les sauvegardes : morts, kills, distance parcourue, œufs éclos, temps de jeu.

## Fonctions bas niveau

`parse_world` suffit dans presque tous les cas. Ces fonctions du module `palworld_stats::save` servent à traiter un seul fichier ou à lire une propriété que la crate n'expose pas encore.

| Fonction | Entrée | Sortie |
| --- | --- | --- |
| `save::decompress(&[u8])` | fichier `.sav` brut | `Result<Vec<u8>>` : données GVAS décompressées |
| `save::parse_player_save(&[u8])` | `Players/<UID>.sav` brut | `Result<(PlayerRecord, Vec<ParseWarning>)>` |
| `save::parse_level_save(&[u8])` | `Level.sav` brut | `Result<(LevelSave, Vec<ParseWarning>)>` |
| `save::gvas::parse(&[u8], &ParseOptions)` | données décompressées | `Result<(GvasHeader, Props, Vec<ParseWarning>)>` |

`LevelSave` contient `players: Vec<CharacterInfo>` (uid, name, level, exp, arena_rank_point, group_id), `owned_pals: BTreeMap<PlayerUid, u32>` et `guilds: Vec<GuildInfo>`.

**Lire une propriété brute.** Le lecteur GVAS rend un arbre de `Value` (`Int`, `Str`, `Bool`, `Struct { props }`, `Map`, `Array`, `Bytes`, `Raw`…). `Props` offre des accesseurs typés : `get`, `struct_props`, `int`, `string`, `bool`, `guid`, `datetime`.

```rust
use palworld_stats::save::{decompress, gvas::{self, ParseOptions}};

let data = decompress(&std::fs::read("Players/3C36EC75000000000000000000000000.sav")?)?;
let opts = ParseOptions { struct_keys: &[], skip: &|_| false };
let (_header, top, _warnings) = gvas::parse(&data, &opts)?;
let save = top.struct_props("SaveData").unwrap();
let record = save.struct_props("RecordData").unwrap();
println!("espèces capturées : {:?}", record.int("TribeCaptureCount"));
```

`skip` reçoit le chemin de chaque propriété (`.worldSaveData.MapObjectSaveData`…) : renvoyez `true` pour la sauter sans la décoder (elle devient un `Raw` vide). C'est ainsi que `parse_level_save` ignore les 85 Mo de données de carte. Le lecteur est protégé contre les fichiers hostiles : tailles vérifiées, compteurs bornés, profondeur limitée à 128.

## Feature `pocketbase`

Avec `features = ["pocketbase"]`, le module `palworld_stats::pocketbase` fournit une struct `serde` par collection. IDs et relations sont des `String`, les dates du RFC 3339, et `id` n'est sérialisé que s'il est renseigné.

| Struct | Collection | Constructeur |
| --- | --- | --- |
| `PalworldCharacter` | `palworld_characters` | à remplir à la main (server, player, player_uid, name, level, exp, guild, last_online, first_seen) |
| `PalworldStats` | `palworld_stats` | `from_player(server, character, &PlayerStats, playtime_seconds, now)` |
| `PalworldSession` | `palworld_sessions` | à remplir à la main ; `source: SessionSource::{Log, Poll}` sérialisé `"log"` / `"poll"` |
| `PalworldStatsSnapshot` | `palworld_stats_snapshots` | `from_player(server, character, &PlayerStats, playtime_seconds, now)` |
| `PalworldGuild` | `palworld_guilds` | `from_guild(server, &GuildInfo, admin_character, now)` ; le niveau sort sous le nom `base_level` |
| `PalworldPal` | `palworld_pals` | `from_pal(server, character, &OwnedPal, now)` : une ligne par Pal possédé, avec `location` (`party`, `box`, `other`) et `slot_index` |

`PalworldStats` reprend tous les compteurs de `RecordStats`, plus `level`, `exp`, `owned_pals`, `arena_rank_point`, les points de technologie, les quêtes, `skins_owned` et le résumé des Pals (`pals_unique_species`, `pals_lucky`, `pals_alpha`, `pals_max_level`, `pals_average_level`, `pals_max_rank`, `pals_awakened`, `pals_perfect_talents`, `pals_four_passives`). Les détails par clé (`capture_by_species`, `fishing_by_species`, `craft_by_item`, `skins`, `status_points`, …) sont des objets JSON : déclarez-les comme champs `json` dans PocketBase. `PalworldStatsSnapshot` garde l'essentiel pour les courbes : `playtime_seconds`, `level`, `exp`, `pal_capture_count`, `paldeck_unique`, `boss_defeated` (boss de terrain + boss de tour).

```rust
use chrono::Utc;
use palworld_stats::pocketbase::{PalworldStats, PalworldStatsSnapshot};

let now = Utc::now();
for p in &world.players {
    let character_id = /* id PocketBase du personnage */;
    let playtime = /* somme de ses sessions */;
    let stats = PalworldStats::from_player(server_id, character_id, p, playtime, now);
    // upsert de stats dans palworld_stats

    let snap = PalworldStatsSnapshot::from_player(server_id, character_id, p, playtime, now);
    if last_snapshot.map_or(true, |last| snap.differs_from(&last)) {
        // insert de snap dans palworld_stats_snapshots
    }
}
```

`differs_from` compare tous les champs sauf `id` et `taken_at` : on n'écrit un point d'historique que si une valeur a changé.

## Export JSON en ligne de commande

L'exemple `export_stats` produit, sans écrire une ligne de code, un JSON de toutes les stats d'un monde au format PocketBase. Il sert à tester la crate sur une vraie sauvegarde ou à alimenter un site statique.

```bash
cargo run --release --example export_stats --features pocketbase -- <dossier du monde> [nom du serveur] [--ids <correspondance.json>] > stats.json
```

- `<dossier du monde>` : `SaveGames/0/<worldId>`, qui contient `Level.sav` et `Players/`.
- `[nom du serveur]` : valeur du champ `server` (défaut `palworld`).
- `--ids` : fichier JSON UID → identifiant de plateforme, tenu à jour par votre logiciel à partir des sondages REST. L'UID peut s'écrire sous n'importe quelle forme.

```json
{
  "3C36EC75000000000000000000000000": "steam_76561198000000001",
  "357a1272": "xbox_2535400000000002"
}
```

Forme de la sortie :

```json
{
  "server": "palworld_vanilla",
  "generated_at": "2026-10-23T08:00:00+00:00",
  "world": { "world_name": "Autosave_W", "in_game_day": 5081 },
  "players": [
    {
      "player_uid": "3C36EC75000000000000000000000000",
      "steam_id": "76561198000000001",
      "platform_id": "steam_76561198000000001",
      "name": "RereBleue",
      "guild": "Les Loutres Mouillées",
      "guild_role": "Member",
      "last_online": "2026-08-06T22:15:34+00:00",
      "stats": { "level": 80, "exp": 45859908, "owned_pals": 869, "species_captured": 257, "fishing_by_species": { "...": 0 }, "...": "..." },
      "party": [ { "species": "JetDragon", "level": 80, "location": "Party", "slot_index": 0, "...": "..." } ],
      "pal_summary": { "total": 869, "unique_species": 82, "lucky": 1, "alpha": 221, "max_level": 80, "...": "..." },
      "pals": [ { "species": "WoolFox", "level": 37, "rank": 5, "is_lucky": false, "passives": ["..."], "...": "..." } ]
    }
  ],
  "guilds": [ { "name": "Les Loutres Mouillées", "member_count": 3, "base_count": 3, "base_level": 35, "...": "..." } ],
  "warnings": []
}
```

Les joueurs sont triés par niveau puis par XP. `world` vient de `LevelMeta.sav` s'il est présent dans le dossier. Avec la liste complète des Pals, le fichier pèse environ 2 Mo pour 6 joueurs. `steam_id` vaut `null` sans `--ids`, pour un joueur absent du fichier ou pour un joueur Xbox (qui garde son `platform_id`). Dans `stats`, `character` contient l'UID et `playtime_seconds` vaut 0, car ces deux valeurs n'existent qu'une fois les données en base.

## Erreurs et avertissements

Toutes les fonctions renvoient `palworld_stats::Result<T>`. L'enum `Error` est `#[non_exhaustive]` : gardez toujours un bras `_ =>`.

| Variante | Quand | Que faire |
| --- | --- | --- |
| `Http(String)` | serveur injoignable, timeout, erreur réseau | considérer le serveur arrêté : fermer les sessions ouvertes, réessayer au cycle suivant |
| `Unauthorized` | l'API répond 401 | vérifier `ADMIN_PASSWORD`, alerter |
| `Status(u16)` | autre réponse HTTP non 2xx | journaliser, réessayer |
| `UnexpectedResponse(String)` | réponse JSON illisible | l'API a probablement changé : alerter, mettre la crate à jour |
| `InvalidHeader(String)` | fichier qui n'est pas une sauvegarde `PlM`/`PlZ`, ou en-tête corrompu | vérifier le chemin du fichier |
| `Decompress(String)` | contenu tronqué ou corrompu (souvent : lu pendant l'écriture) | ignorer ce cycle, réessayer au suivant |
| `Gvas { offset, msg }` | structure de sauvegarde illisible au niveau supérieur | ignorer ce cycle ; si ça persiste après une mise à jour du jeu, mettre la crate à jour |
| `InvalidId(String)` | `PlayerUid::parse` sur une chaîne qui n'est pas un UID | corriger l'entrée |

**Avertissements.** `ParseWarning { path, message }` signale un problème local sans bloquer le reste. `path` situe le problème : `Players/<fichier>` pour un fichier joueur illisible ou mal nommé, `.worldSaveData.…` ou `Players/<fichier>/.SaveData.RecordData.<Nom>` pour une propriété au type inattendu. Journalisez-les : sur une sauvegarde saine, la liste est vide. Des avertissements qui apparaissent après une mise à jour de Palworld signalent un changement de format.

**Robustesse.** Aucune entrée ne fait paniquer la crate : tailles vérifiées, compteurs bornés, profondeur limitée, panique du décodeur Oodle interceptée. Cette dernière protection exige `panic = "unwind"` (valeur par défaut) dans le profil de compilation.

## Tests, fixtures et limites

**Lancer les tests.** `cargo test --release --all-features` exécute 81 tests ; le mode release est conseillé car la décompression Oodle est lente en debug.

- Les tests unitaires (format GVAS, décodeurs, REST simulé avec wiremock, logs, PocketBase) n'ont besoin de rien.
- Les tests sur vraies sauvegardes lisent `fixtures/86150974C2A144B79C51A0354CCBE1FD/`. Ce dossier est ignoré par Git (environ 100 Mo, noms de vrais joueurs) ; sans lui, ces tests sont sautés avec un message.
- Pour tester une nouvelle version de Palworld : copiez un dossier `SaveGames/0/<worldId>` dans `fixtures/`, adaptez le chemin et relancez. Zéro avertissement attendu.

**Limites connues.**

| Limite | Effet |
| --- | --- |
| Pas de Steam ID dans les sauvegardes | le lien UID ↔ plateforme doit venir de l'API REST ou des logs |
| Pas de morts, kills, distance, œufs éclos, temps de jeu | ces stats n'existent pas dans les fichiers Palworld |
| Champ renommé par une mise à jour du jeu | lu comme absent, donc 0, sans avertissement : surveillez les classements après chaque patch |
| Format des logs non vérifié sur un vrai serveur | test `log_real_lines` ignoré en attendant deux vraies lignes |
| `craft_item_total` | compte probablement aussi la production automatique des bases |
| Mémoire | environ 100 Mo de pic pour un `Level.sav` de 92 Mo décompressé |
| Serveurs moddés | non testés |

**Pour aller plus loin.** Le [README](../README.md) décrit la boucle d'intégration complète côté service : sondage, sessions, stockage PocketBase et points d'attention.
