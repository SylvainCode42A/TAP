//! Window state: commands sent to the server and the replies they get.

use std::collections::VecDeque;
use std::io::Write;
use std::net::TcpStream;
use std::sync::mpsc::Receiver;

use eframe::egui;

use crate::network::spawn_reader;
use crate::protocol::{
    AttackResult, Health, Look, QuestInfo, is_reply, merge_quests, parse_inventory, parse_json,
    parse_look, parse_who,
};

/// State of the GUI window.
pub struct TapApp {
    /// Lines coming from the network thread, None if the connection failed.
    pub receiver: Option<Receiver<String>>,
    /// Write side of the connection, None if the connection failed.
    pub writer: Option<TcpStream>,
    /// Every line shown in the log, oldest first.
    pub log: Vec<String>,
    /// Player name typed in the name field.
    pub name: String,
    /// Last room received with LOOK, None before the first one.
    pub room: Option<Look>,
    /// Items held by the player, None before the first INVENTORY reply.
    pub inventory: Option<Vec<String>>,
    /// Players on the whole server, from the last WHO reply.
    pub server_players: Option<usize>,
    /// Player name or group id typed for GROUP INVITE / GROUP JOIN.
    pub group_target: String,
    /// Commands sent and still waiting for their reply, oldest first.
    /// The server answers in order, so the next `OK` / `ERR` answers the front one.
    pub pending: VecDeque<String>,
    /// Last TALK answer: (NPC id, what the NPC said).
    pub dialogue: Option<(String, String)>,
    /// Player health, from the last STATUS reply.
    pub health: Option<Health>,
    /// Last ATTACK: (NPC id, result).
    pub last_attack: Option<(String, AttackResult)>,
    /// Known quests, from QUEST and QUESTS.
    pub quests: Vec<QuestInfo>,
}

impl TapApp {
    /// A window with no connection and nothing received yet.
    fn empty() -> TapApp {
        TapApp {
            receiver: None,
            writer: None,
            log: vec![],
            name: String::new(),
            room: None,
            inventory: None,
            server_players: None,
            group_target: String::new(),
            pending: VecDeque::new(),
            dialogue: None,
            health: None,
            last_attack: None,
            quests: vec![],
        }
    }

    /// Connects to the server and starts the network thread.
    pub fn new(ctx: &egui::Context, host: &str, port: u16) -> TapApp {
        let mut app = TapApp::empty();
        let stream = match TcpStream::connect((host, port)) {
            Ok(s) => s,
            Err(e) => {
                app.log
                    .push(format!("Connection to {host}:{port} failed: {e}"));
                return app;
            }
        };
        match stream.try_clone() {
            Ok(reader_stream) => {
                app.receiver = Some(spawn_reader(reader_stream, ctx.clone()));
                app.writer = Some(stream);
                app.log.push(format!("Connected to {host}:{port}"));
            }
            Err(e) => app.log.push(format!("Cannot clone connection: {e}")),
        }
        app
    }

    /// Sends one command to the server, ending with `\n`, and shows it in the log.
    pub fn send(&mut self, command: &str) {
        let Some(writer) = &mut self.writer else {
            self.log.push("Not connected".to_string());
            return;
        };
        match writer.write_all(format!("{command}\n").as_bytes()) {
            Ok(()) => {
                self.pending.push_back(command.to_string());
                self.log.push(format!("> {command}"));
            }
            Err(e) => self.log.push(format!("Send error: {e}")),
        }
    }

    /// Handles one line from the server: a reply is matched with the command
    /// it answers, then the line goes to the log.
    pub fn receive(&mut self, line: String) {
        let command = if is_reply(&line) {
            self.pending.pop_front()
        } else {
            None
        };
        if let Some(command) = command {
            self.handle_reply(&command, &line);
        }
        self.log.push(line);
    }

    /// Updates the window from the reply `line` to `command`.
    fn handle_reply(&mut self, command: &str, line: &str) {
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
                if let Some(quest) = parse_json::<QuestInfo>(line) {
                    self.quests.retain(|q| q.quest_id != quest.quest_id);
                    self.quests.push(quest);
                }
            }
            "QUESTS" => {
                if let Some(list) = parse_json(line) {
                    merge_quests(&mut self.quests, list);
                }
            }
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
}
