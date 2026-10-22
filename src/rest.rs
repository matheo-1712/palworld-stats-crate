//! Async client for the official Palworld dedicated-server REST API.

use crate::{Error, PlatformId, PlayerUid, Result};
use serde::{Deserialize, Deserializer, Serialize};
use std::time::Duration;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Client for `{base}/v1/api/*`, authenticated with HTTP Basic (`admin`).
#[derive(Clone)]
pub struct RestClient {
    http: reqwest::Client,
    base_url: String,
    password: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ServerInfo {
    pub version: String,
    pub server_name: String,
    pub description: String,
    pub world_guid: String,
}

/// A currently connected player. The player's IP is never captured.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OnlinePlayer {
    pub name: String,
    pub account_name: String,
    pub uid: PlayerUid,
    pub platform_id: PlatformId,
    pub level: Option<i32>,
    pub ping: Option<f64>,
    pub location_x: Option<f64>,
    pub location_y: Option<f64>,
    pub building_count: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ServerMetrics {
    pub server_fps: Option<f64>,
    pub current_players: u32,
    pub server_frame_time: Option<f64>,
    pub max_players: u32,
    pub uptime_seconds: u64,
    pub days: Option<u32>,
    pub basecamp_count: Option<u32>,
}

/// Lenient number: accepts ints and floats, converts to f64.
fn opt_f64<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Option<f64>, D::Error> {
    Option::<f64>::deserialize(d)
}

fn opt_int<'de, D, T>(d: D) -> std::result::Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: TryFrom<i64>,
{
    Ok(Option::<f64>::deserialize(d)?.and_then(|f| {
        if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.0e15 {
            T::try_from(f as i64).ok()
        } else {
            None
        }
    }))
}

fn int_or_zero<'de, D, T>(d: D) -> std::result::Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: TryFrom<i64> + Default,
{
    Ok(opt_int::<D, T>(d)?.unwrap_or_default())
}

#[derive(Deserialize)]
struct InfoDto {
    #[serde(default)]
    version: String,
    #[serde(default, rename = "servername")]
    server_name: String,
    #[serde(default)]
    description: String,
    #[serde(default, rename = "worldguid")]
    world_guid: String,
}

#[derive(Deserialize)]
struct PlayersDto {
    #[serde(default)]
    players: Vec<PlayerDto>,
}

// `ip` is deliberately not declared: serde ignores it.
#[derive(Deserialize)]
struct PlayerDto {
    #[serde(default)]
    name: String,
    #[serde(default, rename = "accountName")]
    account_name: String,
    #[serde(default, rename = "playerId")]
    player_id: String,
    #[serde(default, rename = "userId")]
    user_id: String,
    #[serde(default, deserialize_with = "opt_f64")]
    ping: Option<f64>,
    #[serde(default, deserialize_with = "opt_f64")]
    location_x: Option<f64>,
    #[serde(default, deserialize_with = "opt_f64")]
    location_y: Option<f64>,
    #[serde(default, deserialize_with = "opt_int")]
    level: Option<i32>,
    #[serde(default, deserialize_with = "opt_int")]
    building_count: Option<i32>,
}

#[derive(Deserialize)]
struct MetricsDto {
    #[serde(default, deserialize_with = "opt_f64")]
    serverfps: Option<f64>,
    #[serde(default, deserialize_with = "int_or_zero")]
    currentplayernum: u32,
    #[serde(default, deserialize_with = "opt_f64")]
    serverframetime: Option<f64>,
    #[serde(default, deserialize_with = "int_or_zero")]
    maxplayernum: u32,
    #[serde(default, deserialize_with = "int_or_zero")]
    uptime: u64,
    #[serde(default, deserialize_with = "opt_int")]
    days: Option<u32>,
    #[serde(default, deserialize_with = "opt_int")]
    basecampnum: Option<u32>,
}

impl std::fmt::Debug for RestClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RestClient")
            .field("base_url", &self.base_url)
            .field("password", &"<redacted>")
            .finish()
    }
}

fn build_http(timeout: Duration) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| Error::Http(e.without_url().to_string()))
}

impl RestClient {
    /// `base_url` may have a trailing slash. Default timeout is 10 s.
    pub fn new(base_url: impl Into<String>, admin_password: impl Into<String>) -> Result<Self> {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        Ok(Self {
            http: build_http(DEFAULT_TIMEOUT)?,
            base_url,
            password: admin_password.into(),
        })
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Result<Self> {
        self.http = build_http(timeout)?;
        Ok(self)
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, endpoint: &str) -> Result<T> {
        let url = format!("{}/v1/api/{}", self.base_url, endpoint);
        let resp = self
            .http
            .get(&url)
            .basic_auth("admin", Some(&self.password))
            .send()
            .await
            .map_err(|e| Error::Http(e.without_url().to_string()))?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(Error::Unauthorized);
        }
        if !status.is_success() {
            return Err(Error::Status(status.as_u16()));
        }
        resp.json::<T>()
            .await
            .map_err(|e| Error::UnexpectedResponse(e.without_url().to_string()))
    }

    pub async fn info(&self) -> Result<ServerInfo> {
        let d: InfoDto = self.get("info").await?;
        Ok(ServerInfo {
            version: d.version,
            server_name: d.server_name,
            description: d.description,
            world_guid: d.world_guid,
        })
    }

    pub async fn players(&self) -> Result<Vec<OnlinePlayer>> {
        let d: PlayersDto = self.get("players").await?;
        let mut out = Vec::with_capacity(d.players.len());
        for p in d.players {
            let uid = match PlayerUid::parse(&p.player_id) {
                Ok(u) => u,
                Err(e) => {
                    log::warn!("dropping player {:?} with invalid playerId: {e}", p.name);
                    continue;
                }
            };
            if uid.is_nil() {
                log::debug!(
                    "dropping player {:?} with nil playerId (still loading)",
                    p.name
                );
                continue;
            }
            out.push(OnlinePlayer {
                name: p.name,
                account_name: p.account_name,
                uid,
                platform_id: PlatformId::from(p.user_id.as_str()),
                level: p.level,
                ping: p.ping,
                location_x: p.location_x,
                location_y: p.location_y,
                building_count: p.building_count,
            });
        }
        Ok(out)
    }

    pub async fn metrics(&self) -> Result<ServerMetrics> {
        let d: MetricsDto = self.get("metrics").await?;
        Ok(ServerMetrics {
            server_fps: d.serverfps,
            current_players: d.currentplayernum,
            server_frame_time: d.serverframetime,
            max_players: d.maxplayernum,
            uptime_seconds: d.uptime,
            days: d.days,
            basecamp_count: d.basecampnum,
        })
    }
}
