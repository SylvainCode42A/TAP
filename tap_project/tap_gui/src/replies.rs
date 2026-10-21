//! What each server reply changes in the window. The reply is matched with
//! the command it answers in `app.rs`.

use crate::app::TapApp;
use crate::protocol::{
    QuestInfo, merge_quests, parse_inventory, parse_json, parse_look, parse_who,
};

impl TapApp {
    /// Updates the window from the reply `line` to `command`.
    pub fn handle_reply(&mut self, command: &str, line: &str) {
        let (verb, arg) = command.split_once(' ').unwrap_or((command, ""));
        match verb.to_uppercase().as_str() {
            "LOOK" => {
                if let Some(look) = parse_look(line) {
                    // A new room: the last dialogue belongs to the old one.
                    if self.room.as_ref().map(|r| &r.room.id) != Some(&look.room.id) {
                        self.dialogue = None;
                    }
                    self.room = Some(look);
                }
            }
            "INVENTORY" => {
                if let Some(items) = parse_inventory(line) {
                    self.inventory = Some(items);
                }
            }
            "WHO" => {
                if let Some(count) = parse_who(line) {
                    self.server_players = Some(count);
                }
            }
            "TALK" => {
                if let Some(text) = line.strip_prefix("OK ") {
                    self.dialogue = Some((arg.to_string(), text.to_string()));
                }
            }
            "STATUS" => {
                if let Some(health) = parse_json(line) {
                    self.health = Some(health);
                }
            }
            "ATTACK" => {
                if let Some(result) = parse_json(line) {
                    self.last_attack = Some((arg.to_string(), result));
                }
            }
            "QUEST" => {
                if let Some(mut quest) = parse_json::<QuestInfo>(line) {
                    match self
                        .quests
                        .iter_mut()
                        .find(|q| q.quest_id == quest.quest_id)
                    {
                        // Same place in the list. QUEST sends no progress:
                        // keep the one from the last QUESTS.
                        Some(old) => {
                            if quest.progress.is_empty() {
                                quest.progress = std::mem::take(&mut old.progress);
                            }
                            *old = quest;
                        }
                        None => self.quests.push(quest),
                    }
                }
            }
            "QUESTS" => {
                if let Some(list) = parse_json(line) {
                    merge_quests(&mut self.quests, list);
                }
            }
            "GROUP" => self.handle_group(arg, line),
            _ => {}
        }
    }

    /// GROUP CREATE and GROUP JOIN answer `OK group=<id>`, GROUP LEAVE answers `OK`.
    /// INVITE changes nothing for us. Errors keep the group as it was.
    fn handle_group(&mut self, arg: &str, line: &str) {
        let action = arg.split_whitespace().next().unwrap_or("").to_uppercase();
        match action.as_str() {
            "CREATE" | "JOIN" => {
                if let Some(id) = line.strip_prefix("OK group=") {
                    self.group = Some(id.trim().to_string());
                }
            }
            "LEAVE" if line.starts_with("OK") => self.group = None,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_go_to_the_command_they_answer() {
        let mut app = TapApp::empty();
        app.pending.push_back("QUESTS".to_string());
        app.pending.push_back("INVENTORY".to_string());
        app.pending.push_back("TALK merchant".to_string());

        app.receive("OK hello proto=1".to_string());
        // `OK []` answers QUESTS here: it must not empty the inventory.
        app.receive("OK []".to_string());
        assert!(app.inventory.is_none());
        app.receive(r#"OK ["torch"]"#.to_string());
        assert_eq!(app.inventory, Some(vec!["torch".to_string()]));
        app.receive("OK Take a sword!".to_string());
        assert_eq!(
            app.dialogue,
            Some(("merchant".to_string(), "Take a sword!".to_string()))
        );
        assert!(app.pending.is_empty());
        assert_eq!(app.log.len(), 4);
    }

    #[test]
    fn status_attack_and_quests_are_read() {
        let mut app = TapApp::empty();
        for command in ["STATUS", "ATTACK monster", "QUEST merchant", "QUESTS"] {
            app.pending.push_back(command.to_string());
        }
        app.receive(r#"OK {"hp": 80, "max_hp": 100, "status": "combat"}"#.to_string());
        app.receive(
            r#"OK {"attacker_hp": 80, "target_hp": 15, "damage": 10, "status": "combat"}"#
                .to_string(),
        );
        app.receive(r#"OK {"quest_id": "treasure_quest", "description": "Defeat the monster.", "reward": "victory", "status": "active"}"#.to_string());
        app.receive(
            r#"OK [{"quest_id": "treasure_quest", "status": "active", "progress": "0/1"}]"#
                .to_string(),
        );

        let health = app.health.expect("STATUS read");
        assert_eq!((health.hp, health.max_hp), (80, 100));
        let (npc, attack) = app.last_attack.expect("ATTACK read");
        assert_eq!((npc.as_str(), attack.target_hp), ("monster", 15));
        assert_eq!(app.quests.len(), 1);
        // QUESTS gave the progress, QUEST had given the description: both are kept.
        assert_eq!(app.quests[0].progress, "0/1");
        assert_eq!(app.quests[0].description, "Defeat the monster.");
    }

    #[test]
    fn quest_turned_in_keeps_its_place_and_progress() {
        let mut app = TapApp::empty();
        app.pending.push_back("QUESTS".to_string());
        app.pending.push_back("QUEST merchant".to_string());
        app.pending.push_back("QUESTS".to_string());
        app.receive(
            r#"OK [{"quest_id": "treasure_quest", "status": "active", "progress": "2/2"}, {"quest_id": "free_prisoner", "status": "active", "progress": "0/1"}]"#
                .to_string(),
        );
        app.receive(r#"OK {"quest_id": "treasure_quest", "description": "Defeat the monster.", "reward": "victory", "status": "completed"}"#.to_string());
        // The server sends no progress for a completed quest.
        app.receive(
            r#"OK [{"quest_id": "treasure_quest", "status": "completed"}, {"quest_id": "free_prisoner", "status": "active", "progress": "0/1"}]"#
                .to_string(),
        );

        assert_eq!(app.quests.len(), 2);
        assert_eq!(app.quests[0].quest_id, "treasure_quest");
        assert_eq!(app.quests[0].status, "completed");
        assert_eq!(app.quests[0].progress, "2/2");
    }

    #[test]
    fn group_replies_set_and_clear_the_group() {
        let mut app = TapApp::empty();
        for command in [
            "GROUP CREATE",
            "GROUP JOIN bob",
            "GROUP LEAVE",
            "GROUP LEAVE",
        ] {
            app.pending.push_back(command.to_string());
        }
        app.receive("OK group=g1".to_string());
        assert_eq!(app.group.as_deref(), Some("g1"));
        // Already in a group: the error keeps it.
        app.receive("ERR 402 ALREADY_IN_GROUP".to_string());
        assert_eq!(app.group.as_deref(), Some("g1"));
        app.receive("OK".to_string());
        assert!(app.group.is_none());
        app.receive("ERR 401 NOT_IN_GROUP".to_string());
        assert!(app.group.is_none());
    }
}
