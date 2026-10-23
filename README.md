# palworld-stats

Crate Rust (bibliothèque uniquement, licence MIT) qui lit les statistiques d'un serveur
Palworld dédié. Elle est consommée par le service « Otternel », qui stocke les données dans
PocketBase. La crate est **sans état** : pas d'I/O disque, pas de Docker, pas de base de
données. Elle prend des octets ou des chaînes et renvoie des structs.

Guide complet d'utilisation : [docs/GUIDE.md](docs/GUIDE.md).

Elle fournit :

- `RestClient` : client async de l'API REST officielle (`info`, `players`, `metrics`). L'IP
  des joueurs n'est jamais exposée.
- `diff_presence` : diff de présence entre deux sondages (`Joined` / `Left`), sans horloge.
- `parse_log_line` : reconnaît les lignes de connexion / déconnexion des logs du serveur.
- `parse_world` : lit `Level.sav` + `Players/*.sav` (formats `PlM`/Oodle via `oozextract`, et
  `PlZ`) et renvoie un `WorldStats` (joueurs, guildes, avertissements).
- `pal_name` / `pal_species_name` / `passive_name` : noms affichés en jeu (anglais, français) des
  identifiants internes des sauvegardes (`SheepBall` → « Lamball »).
- Feature `pocketbase` : structs `serde` des collections proposées.

## Installation

```toml
[dependencies]
palworld-stats = { git = "https://github.com/matheo-1712/palworld-stats-crate", features = ["pocketbase"] }
```

## Boucle d'intégration côté Otternel

```rust
use palworld_stats::{
    LogEventKind, PresenceEvent, RestClient, diff_presence, parse_log_line,
    parse_world,
};

let rest = RestClient::new("http://127.0.0.1:8212", admin_password)?;

// 1. Toutes les ~60 s : sondage REST -> sessions (source "poll") + lien UID <-> plateforme.
let current = rest.players().await?;
for event in diff_presence(&previous, &current) {
    match event {
        PresenceEvent::Joined(p) => { /* ouvrir une session, persister p.uid <-> p.platform_id */ }
        PresenceEvent::Left(p) => { /* fermer la session */ }
    }
}
previous = current;

// 2. log_watcher : lignes de logs du conteneur -> sessions (source "log").
if let Some(ev) = parse_log_line(&line) {
    match ev.kind {
        LogEventKind::Joined => { /* ev.name, ev.platform_id, ev.timestamp */ }
        LogEventKind::Left => {}
    }
}

// 3. Au rythme du cycle Minecraft : télécharger les sauvegardes, puis parser.
//    `parse_world` est synchrone et gourmand en CPU : toujours via spawn_blocking.
let world = tokio::task::spawn_blocking(move || parse_world(&level_bytes, player_files))
    .await??; // player_files: Vec<(String /* stem du fichier */, Vec<u8>)>
for p in &world.players {
    // upsert palworld_characters / palworld_stats, snapshot si `differs_from`
}
for g in &world.guilds { /* upsert palworld_guilds */ }
for w in &world.warnings { /* journaliser */ }
```

## Points d'attention

- **Récupération des fichiers** : télécharger `<save_path>/Level.sav` et
  `<save_path>/Players` **séparément**. `DockerFetcher::fetch_files_by_extension` indexe par
  nom de fichier nu ; récupérer tout le dossier du monde mélangerait les
  `backup/world/*/Level.sav` avec le vrai `Level.sav`.
- **Lien `PlayerUid` <-> `PlatformId`** : les sauvegardes ne contiennent pas l'ID plateforme.
  Le lien n'est connu que lorsque le joueur est en ligne (REST `players` ou logs). Otternel
  doit donc le **persister** (champ `player` de `palworld_characters`). Un personnage jamais
  vu en ligne reste sans lien jusqu'à sa prochaine connexion.
- **Valeurs absentes** : Unreal omet les propriétés égales à leur valeur par défaut. Une
  propriété absente de la sauvegarde vaut donc la valeur par défaut du jeu (0 ; niveau 1), sans
  avertissement. Seul un type inattendu produit un `ParseWarning`. Un fichier joueur
  illisible devient un avertissement, pas une erreur ; les champs issus de ce fichier sont
  alors `None` (colonnes `null` côté PocketBase).
- **Format des logs non vérifié** : le format des lignes « joined / left the server » n'a pas
  encore été confronté à de vrais logs du serveur. Le test `log_real_lines` est ignoré en
  attendant des lignes réelles.
- **Horodatage des logs** : `LogEvent::timestamp` est une heure locale du conteneur, sans
  fuseau (naïve). `LogEvent` ne porte pas de `PlayerUid` : une session issue des logs est
  rattachée à un personnage via le lien `PlatformId` <-> `PlayerUid` persisté lors des
  sondages REST.
- **Oodle et `panic`** : la décompression Oodle est protégée par un garde `catch_unwind`, qui
  exige `panic = "unwind"`. Ne pas compiler Otternel avec `panic = "abort"`.
- **Noms de fichiers** : `parse_world` accepte les noms de fichiers joueur avec ou sans
  `.sav` (insensible à la casse). `*_dps`, `Level` et `LevelMeta` sont ignorés sans bruit ;
  tout autre nom invalide produit un `ParseWarning` (`Players/<nom>`).
- **Erreurs REST** : `Error::Http` (serveur injoignable), `Unauthorized` (401), `Status(n)`
  (autre statut non 2xx) et `UnexpectedResponse` (corps JSON illisible) sont distincts.
- **Dernière connexion** : `PlayerRecord::last_online` vient du fichier joueur. Les ticks de
  `GuildMember::last_online_ticks` sont relatifs au serveur et ne doivent jamais être
  convertis en date.
- **Fixtures** : `fixtures/` (vraies sauvegardes) est ignoré par git ; les tests qui en
  dépendent sont sautés si le dossier est absent. Lancer `cargo test --release`.

## Collections PocketBase (feature `pocketbase`)

| Collection | Struct | Contenu |
|---|---|---|
| `servers`, `players` | (existantes) | `players.account` = `steam_…` / `xbox_…` |
| `palworld_characters` | `PalworldCharacter` | un personnage par serveur : `server`, `player` (vide tant que le lien est inconnu), `player_uid`, `name`, `level`, `exp`, `guild`, `last_online`, `first_seen`. Unique (`server`, `player_uid`) |
| `palworld_stats` | `PalworldStats` | valeurs courantes, 1:1 avec le personnage : tous les compteurs ci-dessous, `playtime_seconds`, `updated_at` |
| `palworld_sessions` | `PalworldSession` | `server`, `character`, `started_at`, `ended_at`, `duration_seconds`, `source` (`log` ou `poll`) |
| `palworld_stats_snapshots` | `PalworldStatsSnapshot` | historique : `taken_at`, `playtime_seconds`, `level`, `exp`, `pal_capture_count`, `paldeck_unique`, `boss_defeated` (boss normaux + boss de tour). Un snapshot seulement si `differs_from` |
| `palworld_pals` | `PalworldPal` | une ligne par Pal possédé : `server`, `character`, espèce, niveau, rang, talents, passifs (json), lucky, alpha… |
| `palworld_guilds` | `PalworldGuild` | `server`, `guild_id`, `name`, `admin`, `member_count`, `base_count`, `base_level`, `updated_at`. Unique (`server`, `guild_id`) |

IDs et relations sont des `String`, les dates du RFC 3339. Les valeurs inconnues sont des
`Option` sérialisées en `null`. Le temps de jeu est calculé par Otternel (somme des sessions).
Les détails par clé (`capture_by_species`, `fishing_by_species`, `craft_by_item`, `skins`, `status_points`…) sont des champs `json` de `palworld_stats`.

## Statistiques disponibles

- **REST (en ligne seulement)** : nom, plateforme, niveau, ping, position, constructions ;
  serveur : FPS, joueurs, uptime, jour en jeu, bases.
- **Identité / progression** : nom, `PlayerUid`, dernière connexion, niveau, XP, rang d'arène,
  points de technologie (normaux et boss), technologies débloquées, quêtes terminées.
- **Pals** : captures (total et par espèce), espèces capturées, Paldeck, éveils, rangs,
  mutations, Pals abattus ; **chaque Pal possédé** (espèce, alpha, lucky, niveau, rang, âmes,
  talents, passifs, éveil, skin, amitié, date d'obtention, **équipe / Palbox**) et son résumé
  (`PalSummary`) ; `PlayerStats::party()` donne l'équipe dans l'ordre.
- **Combat** : boss vaincus, boss de tour, raids, prédateurs, camps conquis, arène solo,
  plateformes pétrolières.
- **Exploration / artisanat** : donjons (normaux et fixes), trésors, reliques, pêche, objets
  fabriqués, voyages rapides débloqués, jeu terminé.
- **Exploration / collection** : zones découvertes, barrières, cartes, notes, reliques et objets
  ramassés, conversations et récompenses PNJ, emotes, skins, quêtes en cours.
- **Détails par clé** : par espèce (captures, bonus, abattus), par poisson, par objet fabriqué,
  par boss (tour, raid, spéciaux), par rang (arène, condensation), par type de relique.
- **Personnage** : points de statut attribués ; **monde** (`parse_level_meta`) : nom, jour en jeu.
- **Guilde** : guilde, rôle (admin ou membre), membres, bases, niveau des bases.

**Non disponibles** (absentes des sauvegardes) : morts, victimes (kills), distance parcourue,
œufs éclos.
