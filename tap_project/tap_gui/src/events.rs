//! Reading the `EVT` lines of the server (RFC 4.4 and 6.2): events that
//! answer no command and can arrive at any time.

/// One event from the server.
#[derive(Debug, PartialEq)]
pub enum Event {
    /// `EVT ROOM PRESENCE ENTER <name>`: a player came into our room.
    Enter(String),
    /// `EVT ROOM PRESENCE LEAVE <name>`: a player left our room.
    Leave(String),
    /// `EVT <scope> CHAT <name> <message>`, scope ROOM, GLOBAL or GROUP.
    Chat {
        scope: String,
        from: String,
        text: String,
    },
    /// `EVT GROUP INVITE <leader>`: we are invited to the group of `leader`.
    GroupInvite(String),
    /// `EVT GROUP JOIN <name>`: a player joined our group.
    GroupJoin(String),
    /// `EVT GROUP LEAVE <name>`: a player left our group.
    GroupLeave(String),
    /// `EVT STATS players=<n>`: players on the whole server.
    Players(usize),
    /// Any other event, like our team's `EVT ROOM COMBAT` or an event from
    /// another group's server: it only goes to the log.
    Other,
}

/// Reads a line like `EVT ROOM CHAT alice hello`. Returns None if the line
/// is not an event.
pub fn parse_event(line: &str) -> Option<Event> {
    let rest = line.strip_prefix("EVT ")?;
    // At most 4 parts: the chat message keeps its spaces in the last one.
    let words: Vec<&str> = rest.splitn(4, ' ').collect();
    let event = match words.as_slice() {
        ["ROOM", "PRESENCE", "ENTER", name] => Event::Enter(name.to_string()),
        ["ROOM", "PRESENCE", "LEAVE", name] => Event::Leave(name.to_string()),
        [scope @ ("ROOM" | "GLOBAL" | "GROUP"), "CHAT", from, text] => Event::Chat {
            scope: scope.to_string(),
            from: from.to_string(),
            text: text.to_string(),
        },
        ["GROUP", "INVITE", leader] => Event::GroupInvite(leader.to_string()),
        ["GROUP", "JOIN", name] => Event::GroupJoin(name.to_string()),
        ["GROUP", "LEAVE", name] => Event::GroupLeave(name.to_string()),
        ["STATS", data] => match data.strip_prefix("players=").and_then(|n| n.parse().ok()) {
            Some(count) => Event::Players(count),
            None => Event::Other,
        },
        _ => Event::Other,
    };
    Some(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_and_stats_are_read() {
        assert_eq!(
            parse_event("EVT ROOM PRESENCE ENTER bob"),
            Some(Event::Enter("bob".to_string()))
        );
        assert_eq!(
            parse_event("EVT ROOM PRESENCE LEAVE bob"),
            Some(Event::Leave("bob".to_string()))
        );
        assert_eq!(parse_event("EVT STATS players=3"), Some(Event::Players(3)));
    }

    #[test]
    fn chat_keeps_the_spaces_of_the_message() {
        assert_eq!(
            parse_event("EVT GLOBAL CHAT alice hello all"),
            Some(Event::Chat {
                scope: "GLOBAL".to_string(),
                from: "alice".to_string(),
                text: "hello all".to_string(),
            })
        );
    }

    #[test]
    fn group_events_are_read() {
        assert_eq!(
            parse_event("EVT GROUP INVITE alice"),
            Some(Event::GroupInvite("alice".to_string()))
        );
        assert_eq!(
            parse_event("EVT GROUP JOIN bob"),
            Some(Event::GroupJoin("bob".to_string()))
        );
        assert_eq!(
            parse_event("EVT GROUP LEAVE bob"),
            Some(Event::GroupLeave("bob".to_string()))
        );
    }

    #[test]
    fn unknown_events_and_other_lines() {
        let combat = "EVT ROOM COMBAT alice monster damage=5 target_hp=15 status=combat";
        assert_eq!(parse_event(combat), Some(Event::Other));
        assert_eq!(parse_event("EVT STATS players=abc"), Some(Event::Other));
        assert_eq!(parse_event("EVT WEATHER rain"), Some(Event::Other));
        assert!(parse_event("OK players=3").is_none());
    }
}
