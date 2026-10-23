//! Reading the server replies (RFC 42TAP): JSON bodies and `key=value` lines.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::de::DeserializeOwned;

/// Room part of a LOOK reply (RFC 5.1.2).
/// `#[serde(default)]` lets a server from another group leave a field out:
/// it becomes empty instead of making the whole reply unreadable.
#[derive(Deserialize)]
pub struct RoomInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Direction -> room id. A BTreeMap keeps the directions sorted.
    #[serde(default)]
    pub exits: BTreeMap<String, String>,
}

/// Whole JSON of a LOOK reply (RFC 5.1.2).
#[derive(Deserialize)]
pub struct Look {
    pub room: RoomInfo,
    #[serde(default)]
    pub players: Vec<String>,
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(default)]
    pub npcs: Vec<String>,
}

/// Reads a line like `OK {"room":...}`. Returns None if it is not a LOOK reply.
pub fn parse_look(line: &str) -> Option<Look> {
    let json = line.strip_prefix("OK ")?;
    serde_json::from_str(json).ok()
}

/// Reads a line like `OK ["item.a","item.b"]`. Returns None if it is not an INVENTORY reply.
pub fn parse_inventory(line: &str) -> Option<Vec<String>> {
    let json = line.strip_prefix("OK ")?;
    serde_json::from_str(json).ok()
}

/// Player health from STATUS (RFC 5.4.6).
#[derive(Deserialize)]
pub struct Health {
    pub hp: i64,
    pub max_hp: i64,
    #[serde(default)]
    pub status: String,
}

/// Result of one ATTACK (RFC 5.4.5).
#[derive(Deserialize)]
pub struct AttackResult {
    pub damage: i64,
    pub target_hp: i64,
    #[serde(default)]
    pub status: String,
}

/// One quest from QUEST or QUESTS (RFC 5.4.7 / 5.4.8).
/// QUESTS sends no description or reward, so they are optional.
#[derive(Deserialize)]
pub struct QuestInfo {
    pub quest_id: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub reward: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub progress: String,
}

/// Reads the JSON after `OK ` into any type. Returns None if it does not fit.
pub fn parse_json<T: DeserializeOwned>(line: &str) -> Option<T> {
    let json = line.strip_prefix("OK ")?;
    serde_json::from_str(json).ok()
}

/// Puts the quests from a QUESTS reply in `quests`, keeping the description
/// and reward already known from QUEST (QUESTS does not send them), and the
/// last progress when a finished quest comes without one.
pub fn merge_quests(quests: &mut Vec<QuestInfo>, new_list: Vec<QuestInfo>) {
    let mut merged = Vec::new();
    for mut quest in new_list {
        if let Some(old) = quests.iter().find(|q| q.quest_id == quest.quest_id) {
            if quest.description.is_empty() {
                quest.description = old.description.clone();
            }
            if quest.reward.is_empty() {
                quest.reward = old.reward.clone();
            }
            if quest.progress.is_empty() {
                quest.progress = old.progress.clone();
            }
        }
        merged.push(quest);
    }
    *quests = merged;
}

/// Reads a WHO reply like `OK players=3`. Returns None for any other line.
pub fn parse_who(line: &str) -> Option<usize> {
    line.strip_prefix("OK players=")?.trim().parse().ok()
}

/// True for a reply to a command (`OK ...` or `ERR ...`).
/// The greeting `OK hello` and the `EVT` lines answer no command.
pub fn is_reply(line: &str) -> bool {
    let is_ok = line == "OK" || line.starts_with("OK ");
    (is_ok && !line.starts_with("OK hello")) || line.starts_with("ERR ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_look_reads_rfc_json() {
        let line = r#"OK {"room":{"id":"entrance","name":"Dungeon Entrance","description":"A cold stone archway.","exits":{"north":"corridor","east":"guard_room"}},"players":["Alice"],"items":[],"npcs":["merchant"]}"#;
        let look = parse_look(line).expect("valid LOOK reply");
        assert_eq!(look.room.name, "Dungeon Entrance");
        assert_eq!(
            look.room.exits.get("north").map(String::as_str),
            Some("corridor")
        );
        assert_eq!(look.players, vec!["Alice"]);
        assert!(look.items.is_empty());
    }

    #[test]
    fn parse_look_ignores_other_lines() {
        assert!(parse_look("OK hello proto=1").is_none());
        assert!(parse_look("ERR 301 NO_EXIT").is_none());
    }

    #[test]
    fn parse_inventory_reads_rfc_list() {
        let items = parse_inventory(r#"OK ["item.herbs", "item.loaf_bread"]"#);
        assert_eq!(
            items,
            Some(vec![
                "item.herbs".to_string(),
                "item.loaf_bread".to_string()
            ])
        );
        assert_eq!(parse_inventory("OK []"), Some(vec![]));
        assert!(parse_inventory("OK taken=item.herbs").is_none());
    }

    #[test]
    fn parse_look_accepts_other_worlds() {
        // Another group's server: unknown extra field, no npcs, no description.
        let line = r#"OK {"room":{"id":"loc.bakery","name":"Old Bakery","exits":{}},"players":["bob"],"items":["item.loaf_bread"],"weather":"rain"}"#;
        let look = parse_look(line).expect("reply from another world");
        assert_eq!(look.room.name, "Old Bakery");
        assert!(look.room.description.is_empty());
        assert!(look.npcs.is_empty());
        assert_eq!(look.items, vec!["item.loaf_bread"]);
    }

    #[test]
    fn is_reply_skips_greeting_and_events() {
        assert!(is_reply("OK"));
        assert!(is_reply("OK taken=torch"));
        assert!(is_reply("ERR 404 ITEM_NOT_FOUND"));
        assert!(!is_reply("OK hello proto=1"));
        assert!(!is_reply("EVT ROOM PRESENCE ENTER bob"));
    }

    #[test]
    fn parse_who_reads_player_count() {
        assert_eq!(parse_who("OK players=3"), Some(3));
        assert_eq!(parse_who("OK players=0"), Some(0));
        assert!(parse_who("OK players=abc").is_none());
        assert!(parse_who("OK taken=torch").is_none());
    }
}
