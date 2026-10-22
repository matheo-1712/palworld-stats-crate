use crate::ids::PlatformId;
use chrono::NaiveDateTime;
use regex::Regex;
use serde::Serialize;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LogEventKind {
    Joined,
    Left,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LogEvent {
    pub kind: LogEventKind,
    pub name: String,
    pub platform_id: PlatformId,
    pub timestamp: Option<NaiveDateTime>,
}

// The event part is not anchored, so docker prefixes and ANSI codes are tolerated.
// An optional `[LOG]` tag (and anything before it) is skipped so it never ends up in the name.
// The timestamp is searched separately so it survives arbitrary prefixes.
static EVENT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:.*?\[LOG\])?\s*(.+?) (joined|left) the server\.? \(User id: ([^)]+)\)")
        .expect("valid regex")
});
static TIMESTAMP_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})\]").expect("valid regex")
});

/// Parses a server join/leave log line. Returns `None` for unrelated lines.
/// A missing or invalid timestamp yields `timestamp: None`.
pub fn parse_log_line(line: &str) -> Option<LogEvent> {
    let caps = EVENT_RE.captures(line)?;
    let kind = if &caps[2] == "joined" {
        LogEventKind::Joined
    } else {
        LogEventKind::Left
    };
    let name = caps[1].trim().to_string();
    let platform_id = PlatformId::from(caps[3].trim());
    let timestamp = TIMESTAMP_RE
        .captures(line)
        .and_then(|c| NaiveDateTime::parse_from_str(&c[1], "%Y-%m-%d %H:%M:%S").ok());
    Some(LogEvent {
        kind,
        name,
        platform_id,
        timestamp,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(s: &str) -> chrono::NaiveDateTime {
        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").unwrap()
    }

    #[test]
    fn log_join_with_timestamp() {
        let e = parse_log_line(
            "[2026-10-22 21:03:11] [LOG] RereBleue joined the server. (User id: steam_76561198000000000)",
        )
        .unwrap();
        assert_eq!(e.kind, LogEventKind::Joined);
        assert_eq!(e.name, "RereBleue");
        assert_eq!(e.platform_id, PlatformId::Steam("76561198000000000".into()));
        assert_eq!(e.timestamp, Some(ts("2026-10-22T21:03:11")));
    }

    #[test]
    fn log_left_xbox_no_timestamp() {
        let e =
            parse_log_line("[LOG] Loutre Grise left the server. (User id: xbox_2535400000000000)")
                .unwrap();
        assert_eq!(e.kind, LogEventKind::Left);
        assert_eq!(e.name, "Loutre Grise");
        assert_eq!(e.platform_id, PlatformId::Xbox("2535400000000000".into()));
        assert_eq!(e.timestamp, None);
    }

    #[test]
    fn log_unrelated_line_is_none() {
        assert!(parse_log_line("[LOG] Server saved").is_none());
    }

    #[test]
    fn log_non_ascii_and_edge_inputs_do_not_panic() {
        let e = parse_log_line(
            "[2026-10-22 21:03:11] [LOG] Lóutre✓ joined the server. (User id: steam_1)",
        )
        .unwrap();
        assert_eq!(e.name, "Lóutre✓");
        assert!(parse_log_line("").is_none());
        assert!(parse_log_line(&"é".repeat(100_000)).is_none());
        let tricky =
            parse_log_line("[LOG] Bob joined the server left the server. (User id: steam_2)")
                .unwrap();
        assert_eq!(tricky.kind, LogEventKind::Left);
        assert_eq!(tricky.name, "Bob joined the server");
    }

    #[test]
    fn log_invalid_timestamp_yields_none_timestamp() {
        let e =
            parse_log_line("[2026-13-45 99:99:99] [LOG] Bob joined the server. (User id: steam_2)")
                .unwrap();
        assert_eq!(e.timestamp, None);
        assert_eq!(e.name, "Bob");
    }

    #[test]
    fn log_tolerates_docker_and_ansi_prefix() {
        let e = parse_log_line(
            "2026-10-22T21:03:11.123Z \u{1b}[0m[2026-10-22 21:03:11] [LOG] Bob joined the server. (User id: steam_2)",
        )
        .unwrap();
        assert_eq!(e.name, "Bob");
        assert_eq!(e.timestamp, Some(ts("2026-10-22T21:03:11")));
    }

    // TODO: replace placeholder lines with real `docker logs palworld-vanilla` lines
    // (one join, one leave) once obtained.
    #[test]
    #[ignore = "needs real log lines"]
    fn log_real_lines() {
        let join = parse_log_line(
            "[2026-10-22 21:03:11] [LOG] RereBleue joined the server. (User id: steam_76561198000000000)",
        )
        .unwrap();
        assert_eq!(join.kind, LogEventKind::Joined);
        let leave = parse_log_line(
            "[2026-10-22 21:40:02] [LOG] RereBleue left the server. (User id: steam_76561198000000000)",
        )
        .unwrap();
        assert_eq!(leave.kind, LogEventKind::Left);
    }
}
