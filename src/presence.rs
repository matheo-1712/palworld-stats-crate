use crate::ids::PlayerUid;
use crate::rest::OnlinePlayer;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum PresenceEvent {
    Joined(OnlinePlayer),
    Left(OnlinePlayer),
}

/// Keyed by uid. Joined events in `current` order, then Left events in `previous` order.
/// No clock: the caller timestamps.
pub fn diff_presence(previous: &[OnlinePlayer], current: &[OnlinePlayer]) -> Vec<PresenceEvent> {
    let prev: HashSet<&PlayerUid> = previous.iter().map(|p| &p.uid).collect();
    let curr: HashSet<&PlayerUid> = current.iter().map(|p| &p.uid).collect();
    let joined = current
        .iter()
        .filter(|p| !prev.contains(&p.uid))
        .map(|p| PresenceEvent::Joined(p.clone()));
    let left = previous
        .iter()
        .filter(|p| !curr.contains(&p.uid))
        .map(|p| PresenceEvent::Left(p.clone()));
    joined.chain(left).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{PlatformId, PlayerUid};

    fn p(uid: &str, name: &str) -> OnlinePlayer {
        OnlinePlayer {
            name: name.to_string(),
            account_name: name.to_string(),
            uid: PlayerUid::parse(uid).unwrap(),
            platform_id: PlatformId::from("steam_1"),
            level: None,
            ping: None,
            location_x: None,
            location_y: None,
            building_count: None,
        }
    }

    #[test]
    fn presence_join_and_leave() {
        let (a, b, c) = (p("A1", "A"), p("B2", "B"), p("C3", "C"));
        let ev = diff_presence(&[a.clone(), b.clone()], &[b, c.clone()]);
        assert_eq!(ev, vec![PresenceEvent::Joined(c), PresenceEvent::Left(a)]);
    }

    #[test]
    fn presence_no_change_is_empty() {
        let (a, b) = (p("A1", "A"), p("B2", "B"));
        assert!(diff_presence(&[a.clone(), b.clone()], &[b, a]).is_empty());
        assert!(diff_presence(&[], &[]).is_empty());
    }
}
