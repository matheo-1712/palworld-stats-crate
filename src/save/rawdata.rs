//! Decoders for the Palworld `RawData` byte blobs (characters and groups).

use serde::Serialize;

use crate::Result;
use crate::ids::{GuildId, PlayerUid};
use crate::save::gvas::{ParseOptions, Props, Reader};

/// A decoded character `RawData` blob.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterRaw {
    /// Contains `"SaveParameter"`.
    pub props: Props,
    pub group_id: [u8; 16],
}

/// Properties until `"None"`, 4 unknown bytes, a 16-byte group guid, an optional
/// 4-byte zero trailer (present in current saves), then EOF.
pub fn decode_character(bytes: &[u8]) -> Result<CharacterRaw> {
    let mut r = Reader::new(bytes);
    let opts = ParseOptions {
        struct_keys: &[],
        skip: &|_| false,
    };
    let props = r.properties("", &opts)?;
    r.bytes(4)?;
    let group_id = r.guid()?;
    if r.remaining() == 4 {
        r.bytes(4)?;
    }
    if !r.eof() {
        return Err(r.err(format!("{} trailing bytes after character", r.remaining())));
    }
    Ok(CharacterRaw { props, group_id })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GuildMember {
    pub uid: PlayerUid,
    pub name: String,
    pub last_online_ticks: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GroupRaw {
    Guild {
        id: GuildId,
        name: String,
        base_camp_level: i32,
        base_count: u32,
        admin: PlayerUid,
        members: Vec<GuildMember>,
    },
    /// Neutral, IndependentGuild, Organization: not reported.
    Other,
}

/// Decode a group `RawData` blob; `group_type` looks like `"EPalGroupType::Guild"`.
pub fn decode_group(group_type: &str, bytes: &[u8]) -> Result<GroupRaw> {
    // Only guilds are reported; other group layouts are opaque (never validated).
    if group_type != "EPalGroupType::Guild" {
        return Ok(GroupRaw::Other);
    }

    let mut r = Reader::new(bytes);
    // 1. Common header: id, name, character handles (guid pairs).
    let group_id = r.guid()?;
    let group_name = r.fstring()?;
    skip_guids(&mut r, 2)?;

    // 2. Organisation: type byte, 4 unknown bytes, base ids, 4 unknown bytes.
    r.bytes(5)?;
    let base_count = skip_guids(&mut r, 1)?;
    r.bytes(4)?;

    // 3. Guild: camp level, a guid list, guild name.
    let base_camp_level = r.i32()?;
    skip_guids(&mut r, 1)?;
    let guild_name = r.fstring()?;

    // 4. Guild: last name modifier (guid) and 14 further bytes (fixed in current
    //    saves), then admin and members.
    r.bytes(30)?;
    let (admin, members) = {
        let admin = PlayerUid::from_guid(&r.guid()?);
        let count = r.i32()?;
        let count =
            usize::try_from(count).map_err(|_| r.err(format!("negative member count {count}")))?;
        // Each member needs at least 16 + 8 + 4 + 1 bytes.
        r.check_count(count.saturating_mul(29))?;
        let mut members = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            let uid = PlayerUid::from_guid(&r.guid()?);
            let last_online_ticks = r.i64()?;
            let name = r.fstring()?;
            // One extra per-member byte in current saves.
            r.u8()?;
            members.push(GuildMember {
                uid,
                name,
                last_online_ticks,
            });
        }
        (admin, members)
    };

    // 5. Per-guild trailing data (unmodelled) is ignored.
    Ok(GroupRaw::Guild {
        id: GuildId::from_guid(&group_id),
        name: if guild_name.is_empty() {
            group_name
        } else {
            guild_name
        },
        base_camp_level,
        base_count,
        admin,
        members,
    })
}

/// Read a `u32` count then that many entries of `per` guids, bounded by the remaining bytes.
fn skip_guids(r: &mut Reader, per: usize) -> Result<u32> {
    let n = r.u32()?;
    let total = (n as usize).saturating_mul(16 * per);
    r.check_count(total)?;
    r.bytes(total)?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::gvas::build::*;

    fn guid_hdr(st: &str) -> Vec<u8> {
        let mut h = fs(st);
        h.extend_from_slice(&[0u8; 16]);
        h
    }

    fn character_bytes(trailer: &[u8]) -> Vec<u8> {
        let mut inner = Vec::new();
        tag(&mut inner, "Level", "ByteProperty", &fs("None"), &[80]);
        tag(
            &mut inner,
            "Exp",
            "Int64Property",
            &[],
            &45859908i64.to_le_bytes(),
        );
        tag(&mut inner, "NickName", "StrProperty", &[], &fs("RereBleue"));
        tag(&mut inner, "IsPlayer", "BoolProperty", &[1], &[]);
        none(&mut inner);
        let mut b = Vec::new();
        tag(
            &mut b,
            "SaveParameter",
            "StructProperty",
            &guid_hdr("PalIndividualCharacterSaveParameter"),
            &inner,
        );
        none(&mut b);
        b.extend_from_slice(&[0, 0, 0, 0]);
        b.extend_from_slice(&[0xAA; 16]);
        b.extend_from_slice(trailer);
        b
    }

    #[test]
    fn character_roundtrip() {
        let c = decode_character(&character_bytes(&[])).unwrap();
        assert_eq!(c.group_id, [0xAA; 16]);
        let sp = c.props.struct_props("SaveParameter").unwrap();
        assert_eq!(sp.int("Level"), Some(80));
        assert_eq!(sp.int("Exp"), Some(45859908));
        assert_eq!(sp.string("NickName"), Some("RereBleue"));
        assert_eq!(sp.bool("IsPlayer"), Some(true));
    }

    #[test]
    fn character_with_four_byte_trailer_is_ok() {
        let c = decode_character(&character_bytes(&[0; 4])).unwrap();
        assert_eq!(c.group_id, [0xAA; 16]);
    }

    #[test]
    fn character_trailing_garbage_is_err() {
        assert!(decode_character(&character_bytes(&[1])).is_err());
    }

    #[test]
    fn character_truncated_is_err_not_panic() {
        let data = character_bytes(&[]);
        for len in 0..data.len() {
            assert!(decode_character(&data[..len]).is_err());
        }
    }

    fn g(n: u8) -> [u8; 16] {
        [n; 16]
    }

    fn part1(name: &str) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&g(1));
        b.extend(fs(name));
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&g(2));
        b.extend_from_slice(&g(3));
        b
    }

    fn guild_bytes(guild_name: &str) -> Vec<u8> {
        let mut b = part1("grp");
        b.push(7);
        b.extend_from_slice(&[0; 4]);
        b.extend_from_slice(&3u32.to_le_bytes());
        for i in 10..13 {
            b.extend_from_slice(&g(i));
        }
        b.extend_from_slice(&[0; 4]);
        b.extend_from_slice(&12i32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&g(20));
        b.extend(fs(guild_name));
        b.extend_from_slice(&[0; 30]);
        b.extend_from_slice(&g(30));
        b.extend_from_slice(&2i32.to_le_bytes());
        for (i, n) in [(30u8, "Alice"), (31, "Bob")] {
            b.extend_from_slice(&g(i));
            b.extend_from_slice(&(i as i64 * 1000).to_le_bytes());
            b.extend(fs(n));
            b.push(1);
        }
        b
    }

    #[test]
    fn guild_roundtrip() {
        let r = decode_group("EPalGroupType::Guild", &guild_bytes("The Guild")).unwrap();
        let GroupRaw::Guild {
            id,
            name,
            base_camp_level,
            base_count,
            admin,
            members,
        } = r
        else {
            panic!("not a guild")
        };
        assert_eq!(id, GuildId::from_guid(&g(1)));
        assert_eq!(name, "The Guild");
        assert_eq!(base_camp_level, 12);
        assert_eq!(base_count, 3);
        assert_eq!(admin, PlayerUid::from_guid(&g(30)));
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].name, "Alice");
        assert_eq!(members[0].uid, PlayerUid::from_guid(&g(30)));
        assert_eq!(members[0].last_online_ticks, 30_000);
        assert_eq!(members[1].name, "Bob");
    }

    #[test]
    fn guild_name_falls_back_to_group_name() {
        let r = decode_group("EPalGroupType::Guild", &guild_bytes("")).unwrap();
        let GroupRaw::Guild { name, .. } = r else {
            panic!()
        };
        assert_eq!(name, "grp");
    }

    #[test]
    fn guild_truncated_is_err_and_trailing_ignored() {
        let mut data = guild_bytes("G");
        let ty = "EPalGroupType::Guild";
        assert!(decode_group(ty, &data).is_ok());
        for len in 0..data.len() {
            assert!(decode_group(ty, &data[..len]).is_err(), "len {len}");
        }
        data.push(0);
        assert!(decode_group(ty, &data).is_ok());
    }

    #[test]
    fn hostile_counts_are_err() {
        let mut b = Vec::new();
        b.extend_from_slice(&g(1));
        b.extend(fs("x"));
        b.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_group("EPalGroupType::Guild", &b).is_err());
    }

    #[test]
    fn non_guild_is_other() {
        let b = part1("neutral");
        assert_eq!(
            decode_group("EPalGroupType::Neutral", &b).unwrap(),
            GroupRaw::Other
        );
        // Opaque: trailing bytes are not validated for non-guild types.
        let mut extra = b.clone();
        extra.push(0);
        assert_eq!(
            decode_group("EPalGroupType::Neutral", &extra).unwrap(),
            GroupRaw::Other
        );
    }

    #[test]
    fn independent_guild_with_tail_is_other() {
        let mut b = part1("ind");
        b.push(1);
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&1i32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend(fs("n"));
        // group.py IndependentGuild tail.
        b.extend_from_slice(&g(40));
        b.extend(fs("n2"));
        b.extend_from_slice(&5i64.to_le_bytes());
        b.extend(fs("player"));
        assert_eq!(
            decode_group("EPalGroupType::IndependentGuild", &b).unwrap(),
            GroupRaw::Other
        );
    }

    #[test]
    fn organization_garbage_is_other() {
        assert_eq!(
            decode_group("EPalGroupType::Organization", &[1, 2, 3]).unwrap(),
            GroupRaw::Other
        );
    }
}
