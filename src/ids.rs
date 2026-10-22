use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{Error, Result};

/// Formats a 16-byte UE FGuid as four little-endian `u32` words, each `{:08X}`.
pub fn guid_to_string(bytes: &[u8; 16]) -> String {
    let mut out = String::with_capacity(32);
    for i in 0..4 {
        let word = u32::from_le_bytes([bytes[i * 4], bytes[i * 4 + 1], bytes[i * 4 + 2], bytes[i * 4 + 3]]);
        out.push_str(&format!("{word:08X}"));
    }
    out
}

/// Player identifier: 32 uppercase hex characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerUid(String);

impl PlayerUid {
    /// Parses a UID: dashes are stripped, 1 to 32 hex chars required,
    /// uppercased and right-padded with `'0'` to 32.
    pub fn parse(s: &str) -> Result<PlayerUid> {
        let mut cleaned: String = s.chars().filter(|c| *c != '-').collect();
        if cleaned.is_empty()
            || cleaned.len() > 32
            || !cleaned.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Err(Error::InvalidId(s.to_string()));
        }
        cleaned.make_ascii_uppercase();
        while cleaned.len() < 32 {
            cleaned.push('0');
        }
        Ok(PlayerUid(cleaned))
    }

    pub fn from_guid(bytes: &[u8; 16]) -> PlayerUid {
        PlayerUid(guid_to_string(bytes))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_nil(&self) -> bool {
        self.0.chars().all(|c| c == '0')
    }
}

impl fmt::Display for PlayerUid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Guild identifier: 32 uppercase hex characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GuildId(String);

impl GuildId {
    pub fn from_guid(bytes: &[u8; 16]) -> GuildId {
        GuildId(guid_to_string(bytes))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GuildId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Platform account id. Inner values exclude the prefix, except `Other`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PlatformId {
    Steam(String),
    Xbox(String),
    Other(String),
}

impl From<&str> for PlatformId {
    fn from(s: &str) -> Self {
        if let Some(rest) = s.strip_prefix("steam_") {
            PlatformId::Steam(rest.to_string())
        } else if let Some(rest) = s.strip_prefix("xbox_") {
            PlatformId::Xbox(rest.to_string())
        } else {
            PlatformId::Other(s.to_string())
        }
    }
}

impl fmt::Display for PlatformId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlatformId::Steam(id) => write!(f, "steam_{id}"),
            PlatformId::Xbox(id) => write!(f, "xbox_{id}"),
            PlatformId::Other(s) => f.write_str(s),
        }
    }
}

impl Serialize for PlatformId {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for PlatformId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(PlatformId::from(s.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let c = PlayerUid::parse("3c36ec75").unwrap();
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
}
