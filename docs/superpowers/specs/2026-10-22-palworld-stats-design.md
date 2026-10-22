# palworld-stats — Document de conception

Date : 2026-10-22
Statut : en attente d'approbation

## 1. Intention

L'Antre des Loutres affiche des classements de joueurs par serveur pour Minecraft
(https://antredesloutres.fr/classements/minecraft/) : temps de jeu, morts, kills, blocs…
On veut la même chose pour les serveurs Palworld : un classement par serveur, un total
global, un historique (temps de jeu, niveau…) et un classement par guilde.

Les données Minecraft sont collectées par **Otternel** (service Rust, tokio, PocketBase,
https://github.com/Corentin-cott/Otternel). Palworld doit s'y brancher de la même façon.
Ce projet livre la **crate Rust `palworld-stats`** qu'Otternel appelle comme une
bibliothèque. Otternel garde la responsabilité du planning, du stockage PocketBase et de
l'accès Docker.

## 2. Objectifs, non-objectifs, anti-objectifs

**Objectifs**
- Lire l'API REST officielle du serveur dédié Palworld (joueurs en ligne, infos, métriques).
- Parser les sauvegardes Palworld (`Level.sav`, `Players/*.sav`) en Rust pur, au format
  actuel (`PlM`/Oodle, UE 5.1), et en extraire toutes les statistiques disponibles (§6).
- Permettre un temps de jeu à ±1 min : parser les lignes de log connexion/déconnexion et
  fournir un diff de présence basé sur le sondage REST.
- Identifier les joueurs par leur ID plateforme (`steam_…`, `xbox_…`).
- Fournir une proposition de schéma PocketBase et les structs `serde` correspondantes.
- Livrer tout le périmètre d'un coup (pas de découpage v1/v2).

**Non-objectifs**
- Morts, kills, distance parcourue, œufs éclos : **absents des sauvegardes**, écartés.
- Stocker quoi que ce soit, planifier, accéder à Docker ou au disque : c'est le rôle d'Otternel.
- Fournir un binaire ou une API HTTP.
- Écrire ou modifier des sauvegardes.
- Une fiche par Pal capturé (comme `captured_pokemons` pour Cobblemon) : hors périmètre.
- Fusionner automatiquement un joueur Steam et son compte Xbox (lien manuel côté PocketBase).
- Serveurs moddés.

**Anti-objectifs** (ce qui serait un échec même si ça « marche »)
- Bloquer le runtime tokio d'Otternel pendant le parsing d'un `Level.sav` (~92 Mo décompressé).
- Planter ou renvoyer une erreur globale parce qu'une mise à jour de Palworld a ajouté ou
  renommé une propriété : la crate doit dégrader proprement (champ `None`, avertissement).
- Dépendre d'une bibliothèque propriétaire (Oodle officiel), de Python ou d'un outil externe.
- Exposer des données sensibles des joueurs (IP renvoyée par l'API REST) dans les types de sortie.

## 3. Contexte et contraintes

- **Hébergement** : VPS Linux, un conteneur Docker par serveur Palworld,
  image `thijsvanloef/palworld-server-docker:latest`, ex. `/opt/docker/palworld/palworld_vanilla/`,
  volume `./data:/palworld`. Sauvegardes dans le conteneur :
  `/palworld/Pal/Saved/SaveGames/0/<worldId>/{Level.sav, LevelMeta.sav, Players/*.sav}`.
- **Version de Palworld** : la dernière (UE 5.1.1, sauvegardes `PlM1`). Le format évolue
  avec les mises à jour du jeu.
- **API REST** : port 8212/tcp, activée via le `.env` (`REST_API_ENABLED=true`),
  authentification Basic `admin:<ADMIN_PASSWORD>`. Otternel sera sur le même réseau Docker
  et joindra `http://<container>:8212`.
- **Otternel** : Rust edition 2024, tokio, `anyhow`/`thiserror`, `serde`, `chrono`, `log`,
  `bollard`. Il récupère déjà des fichiers depuis les conteneurs via
  `DockerFetcher::fetch_files_by_extension(container, path, ext)`. Il surveille déjà des logs
  (`serverlog/`, `triggers.toml`). Il connaît les serveurs via la collection PocketBase
  `servers`, qui contiendra les serveurs Palworld.
- **Collections PocketBase existantes utiles** : `servers`, `active_servers` (paramètres RCON),
  `players` (account id, nom, plateforme, première/dernière connexion),
  `player_connections_logs`, `players_stats`.
- **Rythme** : le parsing des sauvegardes suit le rythme d'Otternel pour Minecraft (tout est
  collecté en même temps).
- **Livraison** : dépôt Git séparé, consommé par Otternel comme dépendance Git. Licence MIT
  (choix du concepteur, voir §10).

## 4. Approches envisagées

Le seul vrai risque technique est la lecture des `.sav` : compression Oodle propriétaire
(`PlM`) puis format GVAS d'Unreal avec des blocs binaires propres à Palworld.

| Option | Principe | Pour | Contre |
|---|---|---|---|
| **A. 100 % Rust** | `oozextract` (décodeur Oodle libre, MIT) + parseur GVAS/Palworld écrit dans la crate | Un seul binaire, aucune dépendance externe, cohérent avec Otternel | Parseur GVAS à écrire ; `oozextract` est un petit projet |
| B. Rust + Oodle officiel | Crate `oodle` qui charge `oo2core.so` | Décompression garantie fidèle | Bibliothèque propriétaire à distribuer (licence), dépendance native |
| C. palworld-save-tools en sous-processus | Appel Python | Rapide à mettre en place | Python dans le conteneur, contraire au choix du Rust natif |

**Choix : option A**, retenue par l'utilisateur.

**Validé par un essai** sur la sauvegarde réelle du serveur vanilla : tous les fichiers sont
`PlM1`. `oozextract` 0.5.5 les décompresse sans erreur (`Level.sav` : 4,9 Mo → 91,6 Mo en
34 ms ; joueur : < 1 ms). Un parseur GVAS minimal lit l'en-tête (GVAS v3, UE 5.1.1,
85 versions personnalisées) et toutes les propriétés de premier et deuxième niveau.

## 5. Architecture de la crate

Crate unique `palworld-stats`, **sans état, sans I/O disque, sans base de données**.
Elle prend des octets ou des chaînes et renvoie des structs.

```
palworld-stats/
  src/
    lib.rs
    error.rs        // Error (thiserror), Result
    ids.rs          // PlatformId, PlayerUid, GuildId
    rest/           // client API REST (reqwest, async)
    presence.rs     // diff de présence entre deux sondages
    logs.rs         // parsing des lignes de log join/left
    save/
      container.rs  // en-tête .sav, décompression PlM (oozextract) / PlZ (flate2)
      gvas/         // lecteur GVAS générique (propriétés UE)
      rawdata/      // décodeurs des blocs RawData Palworld (personnage, groupe…)
      player.rs     // Players/<uid>.sav → PlayerSave
      level.rs      // Level.sav → LevelSave
      world.rs      // fusion → WorldStats
    pocketbase.rs   // structs serde des collections proposées (feature "pocketbase")
  tests/            // tests d'intégration sur fixtures
```

### 5.1 Identifiants (`ids.rs`)
- `PlatformId` : enum `Steam(String)`, `Xbox(String)` et `Other(String)`, construit depuis le
  `userId` REST ou le log (`steam_7656…`, `xbox_…`). `Display` restitue la chaîne d'origine.
  C'est la clé `account` dans `players`.
- `PlayerUid` : UID interne Palworld (GUID). Il apparaît comme `playerId` dans l'API REST
  et comme nom de fichier `Players/<UID>.sav`. Les deux formes sont normalisées en une seule
  représentation.
- **Point clé** : les sauvegardes ne contiennent **pas** l'ID plateforme. Le lien
  `PlayerUid ↔ PlatformId` n'est connu que lorsque le joueur est en ligne (API REST
  `players` : `playerId` + `userId`). La crate fournit le lien à chaque sondage, et
  Otternel doit le **persister** (champ `player` de `palworld_characters`). Un personnage
  vu uniquement dans les sauvegardes (joueur jamais vu en ligne depuis la mise en service)
  reste sans lien jusqu'à sa prochaine connexion.

### 5.2 Client REST (`rest/`)
- `RestClient::new(base_url, admin_password)`, async, `reqwest` (rustls), timeout configurable.
- `info()` : version, nom du serveur, description, world GUID.
- `players()` : `Vec<OnlinePlayer>` (nom, nom de compte, `PlayerUid`, `PlatformId`, niveau,
  ping, position, nombre de constructions). **L'IP est volontairement omise.**
- `metrics()` : FPS serveur, joueurs connectés et max, uptime, jour en jeu, nombre de bases.
- Erreurs distinctes : serveur injoignable, authentification refusée, réponse inattendue.

### 5.3 Présence et temps de jeu (`presence.rs`, `logs.rs`)
- `diff_presence(previous: &[OnlinePlayer], current: &[OnlinePlayer]) -> Vec<PresenceEvent>`
  (`Joined` / `Left`, avec `PlayerUid`, `PlatformId`, nom). Pure, sans horloge : Otternel
  horodate.
- `parse_log_line(&str) -> Option<LogEvent>` reconnaît les lignes de connexion et de
  déconnexion du serveur (« … joined the server. (User id: steam_…) », « … left the server … »).
  Otternel l'appelle depuis son `log_watcher` (ou via des règles `triggers.toml`).
  **À vérifier pendant l'implémentation** sur les logs réels du conteneur (`docker logs`) : le
  format exact n'a pas encore été observé.
- Stratégie de temps de jeu : le **log** est la source principale (horodatage exact). Le
  **sondage REST** sert de filet : il ferme les sessions orphelines (redémarrage du serveur,
  ligne de log manquée) et en ouvre si une connexion a été manquée. Une session = (début,
  fin, source). Le temps de jeu cumulé est la somme des sessions.

### 5.4 Sauvegardes (`save/`)
- `decompress(&[u8]) -> Result<Vec<u8>>` : en-tête de 12 octets (taille décompressée u32,
  taille compressée u32, magic `PlM`/`PlZ`, type `'1'`/`'2'`). `PlM` → `oozextract`.
  `PlZ` → zlib (double zlib si type `'2'`), pour les anciennes sauvegardes.
- Lecteur GVAS générique : en-tête, puis propriétés (Int, Int64, UInt32, Float, Str, Name,
  Bool, Byte, Enum, Struct, Array, Set, Map). Les propriétés inconnues ou non nécessaires
  sont **sautées grâce à leur taille**, sans être décodées. C'est indispensable pour
  ignorer `MapObjectSaveData` (44 Mo) et `FoliageGridSaveDataMap`.
- `parse_player_save(&[u8]) -> PlayerSave` (`Players/<uid>.sav`, `SaveData`) :
  `PlayerUId`, `RecordData` (§6), `UnlockedRecipeTechnologyNames`, `TechnologyPoint`,
  `bossTechnologyPoint`, `CompletedQuestArray_FullRelease`, `LastOnlineDateTime`,
  `PlayerPlatform`. Les fichiers `*_dps.sav` (stockage de Pals) sont ignorés.
- `parse_level_save(&[u8]) -> LevelSave` (`worldSaveData`) :
  - `CharacterSaveParameterMap` : décodage du `RawData` des personnages **joueurs**
    uniquement (nom, niveau, XP, UID propriétaire). Les Pals possédés sont seulement comptés
    par joueur, sans être décodés en détail.
  - `GroupSaveDataMap` : décodage du `RawData` des groupes de type guilde (ID, nom, admin,
    membres, bases).
  - `BaseCampSaveData` : nombre de bases et niveau par guilde.
  - `GameTimeSaveData` : jour en jeu (facultatif).
  - Référence pour les formats RawData : `palworld-save-tools` (cheahjs), modules `rawdata/`.
- `parse_world(level: &[u8], players: impl IntoIterator<Item = (String, Vec<u8>)>) -> WorldStats`
  : point d'entrée principal. Il fusionne `LevelSave` et chaque `PlayerSave` par `PlayerUid`,
  et renvoie les joueurs, les guildes et les avertissements de parsing.
- **Asynchronisme** : le parsing est synchrone (CPU). La doc et l'exemple indiquent de
  l'appeler via `tokio::task::spawn_blocking`. Mémoire crête attendue : environ 2 × 92 Mo
  pour `Level.sav`.
- **Robustesse** : chaque champ de stat est `Option`. Une propriété absente ou de type
  inattendu produit un `ParseWarning` dans `WorldStats.warnings`, pas une erreur. Seuls une
  décompression impossible et un GVAS illisible sont des erreurs.

## 6. Statistiques fournies (par joueur et par serveur)

Source REST (en ligne seulement) : nom, plateforme, niveau, ping, nombre de constructions.

Source sauvegardes (tous les joueurs ayant déjà joué) :

| Catégorie | Champs (propriétés Palworld) |
|---|---|
| Identité | nom, `PlayerUid`, plateforme (`PlayerPlatform`), dernière connexion (`LastOnlineDateTime`) |
| Progression | niveau, XP, points de technologie (normaux et boss), nombre de technologies débloquées, quêtes terminées |
| Pals | Pals capturés (`PalCaptureCount`), captures par espèce (`TribeCaptureCount`), espèces uniques et complétion du Paldeck (`PaldeckUnlockFlag`), Pals possédés (comptés dans `Level.sav`), éveils (`AwakeningCount`), rangs (`PalRankupCount`), mutations (`MutationCount`), Pals abattus (`PalButcherCount`) |
| Combat | boss vaincus (`NormalBossDefeatFlag`), boss de tour (`TowerBossDefeatCount`), raids (`RaidBossDefeatCount`), prédateurs (`PredatorDefeatCount`), camps conquis (`CampConqueredCount`), arène solo (`ArenaSoloClearCount`), plateformes pétrolières (`OilrigClearCount`) |
| Exploration | donjons (`NormalDungeonClearCount`, `FixedDungeonClearCount`), trésors (`FoundTreasureCount`), reliques (`RelicPossessNum`), pêche (`FishingCountMap`, total) |
| Artisanat | objets fabriqués (`CraftItemCount`, total) |
| Guilde | guilde, rôle (admin ou membre) |
| Temps de jeu | calculé par Otternel à partir des sessions (§5.3) |

Par guilde : nom, admin, membres, nombre de bases, niveau des bases, et les agrégats
calculés par Otternel ou le site (somme des temps de jeu, captures…).

Les noms exacts des propriétés ont été relevés sur la sauvegarde réelle. Les compteurs de
`RecordData` stockés sous forme de map (par espèce, par poisson…) sont exposés à la fois en
**total** et en **map détaillée**.

## 7. Proposition de schéma PocketBase

Proposition à valider. Otternel l'implémente, et la crate fournit les structs `serde`
derrière la feature `pocketbase`. Conventions d'Otternel : ID et relations en `String`
(15 caractères), dates en RFC 3339.

- **`servers`** (existante) : on ajoute pour les serveurs Palworld `rest_url`,
  `rest_password` (ou réutilisation d'`active_servers`), `container_name` et `save_path`
  (chemin du dossier `SaveGames/0/<worldId>` dans le conteneur).
- **`players`** (existante) : un enregistrement par compte plateforme, `account` =
  `steam_…`/`xbox_…`, `platform` = relation vers la plateforme Palworld Steam ou Xbox. Le lien
  vers un compte ADL et le rattachement Steam/Xbox d'une même personne se font comme pour
  Minecraft.
- **`palworld_characters`** (nouvelle) : un personnage par serveur.
  `server` (rel), `player` (rel `players`, vide tant que le lien UID↔plateforme est inconnu),
  `player_uid`, `name`, `level`, `exp`, `guild` (rel), `last_online`, `first_seen`.
  Clé unique (`server`, `player_uid`).
- **`palworld_stats`** (nouvelle) : valeurs courantes, 1:1 avec `palworld_characters`.
  Tous les compteurs du §6 en colonnes, plus `playtime_seconds` (cumul) et `updated_at`.
- **`palworld_sessions`** (nouvelle) : `server`, `character`, `started_at`, `ended_at`
  (vide si en cours), `duration_seconds`, `source` (`log` | `poll`). Source du temps de jeu
  et de l'historique fin. (Alternative : étendre `player_connections_logs`, mais elle ne
  porte qu'un horodatage.)
- **`palworld_stats_snapshots`** (nouvelle) : historique pour les graphiques. `character`,
  `server`, `taken_at`, `playtime_seconds`, `level`, `exp`, `pal_capture_count`,
  `paldeck_unique`, `boss_defeated`. Un snapshot par personnage et par cycle de parsing,
  seulement si une valeur a changé.
- **`palworld_guilds`** (nouvelle) : `server`, `guild_id`, `name`, `admin` (rel
  `palworld_characters`), `member_count`, `base_count`, `base_level`, `updated_at`.
  Clé unique (`server`, `guild_id`).

Total global multi-serveurs : agrégation par `players` sur toutes les lignes de
`palworld_stats`, comme pour Minecraft. Les serveurs désactivés gardent leurs lignes.
Masquage de joueurs : côté Otternel ou site (champ `hidden`), la crate renvoie tout.

## 8. Intégration côté Otternel (hors crate, pour référence)

1. Toutes les ~60 s, par serveur Palworld : `RestClient::players()` → `diff_presence` →
   ouvrir/fermer les `palworld_sessions` (source `poll`) et persister le lien
   `PlayerUid ↔ PlatformId`.
2. `log_watcher` : `parse_log_line` sur les logs du conteneur → sessions (source `log`).
3. Au rythme du cycle Minecraft : `fetch_files_by_extension(container, save_path, "sav")`
   → `spawn_blocking(parse_world)` → upsert de `palworld_characters`, `palworld_stats` et
   `palworld_guilds`, plus un snapshot.

## 9. Tests

- Tests unitaires : en-tête et décompression, lecteur GVAS (propriétés synthétiques),
  décodeurs RawData, `diff_presence`, `parse_log_line`, désérialisation REST (JSON d'exemple).
- Tests d'intégration sur les **vraies sauvegardes** dans `fixtures/` : décompression de
  tous les fichiers, et `parse_world` qui renvoie les 6 joueurs de la fixture avec des valeurs plausibles
  (niveau > 0, nom non vide, compteurs ≥ 0) sans avertissement.
- Les fixtures contiennent des données réelles de joueurs (noms) et pèsent ~100 Mo : elles
  sont **exclues du dépôt** (`.gitignore`), et les tests d'intégration sont ignorés si elles
  sont absentes. Les tests unitaires restent autonomes.
- Client REST testé contre un serveur HTTP simulé (`wiremock`).

## 10. Décisions prises par le concepteur (à vérifier)

- Licence MIT. Pas de publication sur crates.io.
- Le secours Oodle officiel (feature `oodle-native`) n'est **pas** implémenté : l'essai le
  rend inutile. Il reste documenté comme piste si une future version casse `oozextract`.
- Pas de fiche par Pal capturé. Les `*_dps.sav` sont ignorés.
- IP des joueurs non exposée.
- Fixtures hors dépôt.
- `palworld_sessions` séparée plutôt qu'une extension de `player_connections_logs`.
- Snapshots d'historique uniquement quand une valeur change.

## 11. Laissé au constructeur

- Structure interne exacte des modules, noms de types secondaires.
- Format exact des lignes de log, à relever sur les logs réels du conteneur.
- Détail des décodeurs RawData, d'après palworld-save-tools.
- Gestion fine des versions de sauvegarde, tant que la règle « avertir, ne pas casser »
  est respectée.
