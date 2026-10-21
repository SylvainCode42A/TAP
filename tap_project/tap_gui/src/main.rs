use std::collections::{BTreeMap, VecDeque};
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use eframe::egui;
use serde::Deserialize;
use serde::de::DeserializeOwned;

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 7878;

/// Reads `[host] [port]` from the command line, with defaults.
fn parse_args() -> Result<(String, u16), String> {
    let args: Vec<String> = env::args().collect();

    if args.len() > 3 {
        return Err("too many arguments".to_string());
    }

    let host = match args.get(1) {
        Some(h) => h.clone(),
        None => DEFAULT_HOST.to_string(),
    };

    let port = match args.get(2) {
        Some(p) => match p.parse::<u16>() {
            Ok(n) => n,
            Err(_) => return Err(format!("invalid port: {p}")),
        },
        None => DEFAULT_PORT,
    };

    Ok((host, port))
}

/// Starts a thread that forwards every line received from the server to the window.
/// `ctx` is used to wake the window up as soon as a line arrives.
fn spawn_reader(stream: TcpStream, ctx: egui::Context) -> Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            let text = match line {
                Ok(t) => t,
                Err(e) => {
                    let _ = sender.send(format!("Read error: {e}"));
                    break;
                }
            };
            // An error here means the window is closed: nobody is listening anymore.
            if sender.send(text).is_err() {
                return;
            }
            ctx.request_repaint();
        }
        let _ = sender.send("Connection closed".to_string());
        ctx.request_repaint();
    });
    receiver
}

/// Room part of a LOOK reply (RFC 5.1.2).
/// `#[serde(default)]` lets a server from another group leave a field out:
/// it becomes empty instead of making the whole reply unreadable.
#[derive(Deserialize)]
struct RoomInfo {
    id: String,
    name: String,
    #[serde(default)]
    description: String,
    /// Direction -> room id. A BTreeMap keeps the directions sorted.
    #[serde(default)]
    exits: BTreeMap<String, String>,
}

/// Whole JSON of a LOOK reply (RFC 5.1.2).
#[derive(Deserialize)]
struct Look {
    room: RoomInfo,
    #[serde(default)]
    players: Vec<String>,
    #[serde(default)]
    items: Vec<String>,
    #[serde(default)]
    npcs: Vec<String>,
}

/// Reads a line like `OK {"room":...}`. Returns None if it is not a LOOK reply.
fn parse_look(line: &str) -> Option<Look> {
    let json = line.strip_prefix("OK ")?;
    serde_json::from_str(json).ok()
}

/// Reads a line like `OK ["item.a","item.b"]`. Returns None if it is not an INVENTORY reply.
fn parse_inventory(line: &str) -> Option<Vec<String>> {
    let json = line.strip_prefix("OK ")?;
    serde_json::from_str(json).ok()
}

/// Player health from STATUS (RFC 5.4.6).
#[derive(Deserialize)]
struct Health {
    hp: i64,
    max_hp: i64,
    #[serde(default)]
    status: String,
}

/// Result of one ATTACK (RFC 5.4.5).
#[derive(Deserialize)]
struct AttackResult {
    damage: i64,
    target_hp: i64,
    #[serde(default)]
    status: String,
}

/// One quest from QUEST or QUESTS (RFC 5.4.7 / 5.4.8).
/// QUESTS sends no description or reward, so they are optional.
#[derive(Deserialize)]
struct QuestInfo {
    quest_id: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    reward: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    progress: String,
}

/// Reads the JSON after `OK ` into any type. Returns None if it does not fit.
fn parse_json<T: DeserializeOwned>(line: &str) -> Option<T> {
    let json = line.strip_prefix("OK ")?;
    serde_json::from_str(json).ok()
}

/// Puts the quests from a QUESTS reply in `quests`, keeping the description
/// and reward already known from QUEST (QUESTS does not send them).
fn merge_quests(quests: &mut Vec<QuestInfo>, new_list: Vec<QuestInfo>) {
    let mut merged = Vec::new();
    for mut quest in new_list {
        if let Some(old) = quests.iter().find(|q| q.quest_id == quest.quest_id) {
            if quest.description.is_empty() {
                quest.description = old.description.clone();
            }
            if quest.reward.is_empty() {
                quest.reward = old.reward.clone();
            }
        }
        merged.push(quest);
    }
    *quests = merged;
}

/// Reads a WHO reply like `OK players=3`. Returns None for any other line.
fn parse_who(line: &str) -> Option<usize> {
    line.strip_prefix("OK players=")?.trim().parse().ok()
}

/// Turns an id into a name for people: `item.loaf_bread` -> `Loaf Bread`.
/// Commands still send the id, which every server accepts (RFC 8.3).
fn display_name(id: &str) -> String {
    let last_part = id.rsplit('.').next().unwrap_or(id);
    last_part
        .split('_')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

/// Joins a list with commas, or gives `-` when it is empty.
fn join_or_dash(list: &[String]) -> String {
    if list.is_empty() {
        "-".to_string()
    } else {
        list.join(", ")
    }
}

/// State of the GUI window.
struct TapApp {
    /// Lines coming from the network thread, None if the connection failed.
    receiver: Option<Receiver<String>>,
    /// Write side of the connection, None if the connection failed.
    writer: Option<TcpStream>,
    /// Every line shown in the log, oldest first.
    log: Vec<String>,
    /// Player name typed in the name field.
    name: String,
    /// Last room received with LOOK, None before the first one.
    room: Option<Look>,
    /// Items held by the player, None before the first INVENTORY reply.
    inventory: Option<Vec<String>>,
    /// Players on the whole server, from the last WHO reply.
    server_players: Option<usize>,
    /// Player name or group id typed for GROUP INVITE / GROUP JOIN.
    group_target: String,
    /// Commands sent and still waiting for their reply, oldest first.
    /// The server answers in order, so the next `OK` / `ERR` answers the front one.
    pending: VecDeque<String>,
    /// Last TALK answer: (NPC id, what the NPC said).
    dialogue: Option<(String, String)>,
    /// Player health, from the last STATUS reply.
    health: Option<Health>,
    /// Last ATTACK: (NPC id, result).
    last_attack: Option<(String, AttackResult)>,
    /// Known quests, from QUEST and QUESTS.
    quests: Vec<QuestInfo>,
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
    fn new(ctx: &egui::Context, host: &str, port: u16) -> TapApp {
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
    fn send(&mut self, command: &str) {
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
    fn receive(&mut self, line: String) {
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

/// True for a reply to a command (`OK ...` or `ERR ...`).
/// The greeting `OK hello` and the `EVT` lines answer no command.
fn is_reply(line: &str) -> bool {
    let is_ok = line == "OK" || line.starts_with("OK ");
    (is_ok && !line.starts_with("OK hello")) || line.starts_with("ERR ")
}

impl eframe::App for TapApp {
    /// Called on every frame: draws the whole window from the current state.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Handle every line received since the last frame. They are collected
        // first because `receive` needs `self` while `receiver` borrows it.
        let lines: Vec<String> = match &self.receiver {
            Some(receiver) => receiver.try_iter().collect(),
            None => vec![],
        };
        for line in lines {
            self.receive(line);
        }

        // Buttons only add commands to this list: they are sent after drawing,
        // because the drawing code already borrows parts of `self`.
        let mut commands: Vec<String> = Vec::new();

        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("TAP");
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut self.name);
                if ui.button("Connect").clicked() {
                    commands.push(format!("CONNECT {}", self.name.trim()));
                    push_refresh(&mut commands);
                    commands.push("STATUS".to_string());
                    commands.push("QUESTS".to_string());
                }
            });
            ui.separator();
            show_counters(ui, self.room.as_ref(), self.server_players);
            if let Some(look) = &self.room {
                show_room(ui, look, &mut commands);
                if let Some((npc, text)) = &self.dialogue {
                    show_dialogue(ui, npc, text);
                }
                ui.separator();
            }
            ui.horizontal(|ui| {
                ui.label("Inventory:");
                if let Some(items) = &self.inventory {
                    if items.is_empty() {
                        ui.label("-");
                    }
                    for item in items {
                        let button = ui.button(format!("Drop {}", display_name(item)));
                        if button.on_hover_text(item).clicked() {
                            commands.push(format!("DROP {item}"));
                            push_refresh(&mut commands);
                        }
                    }
                }
            });
            show_character(
                ui,
                self.health.as_ref(),
                self.last_attack.as_ref(),
                &self.quests,
            );
            ui.horizontal_wrapped(|ui| {
                for command in ["LOOK", "INVENTORY", "STATUS", "QUESTS", "WHO", "QUIT"] {
                    if ui.button(command).clicked() {
                        commands.push(command.to_string());
                    }
                }
            });
            show_group(ui, &mut self.group_target, &mut commands);
            ui.separator();
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &self.log {
                        ui.label(line);
                    }
                });
        });

        for command in commands {
            self.send(&command);
        }
    }
}

/// Adds LOOK, INVENTORY and WHO, so the room, the inventory and the counters
/// are shown again.
fn push_refresh(commands: &mut Vec<String>) {
    commands.push("LOOK".to_string());
    commands.push("INVENTORY".to_string());
    commands.push("WHO".to_string());
}

/// Draws the player counters: in this room (from LOOK) and on the server (from WHO).
/// Shows `?` while the reply has not arrived yet.
fn show_counters(ui: &mut egui::Ui, look: Option<&Look>, server_players: Option<usize>) {
    let in_room = look.map_or("?".to_string(), |l| l.players.len().to_string());
    let on_server = server_players.map_or("?".to_string(), |n| n.to_string());
    ui.label(format!(
        "Players in this room: {in_room}   |   Players on the server: {on_server}"
    ));
}

/// Draws the room panel from the last LOOK reply, with one button per action.
fn show_room(ui: &mut egui::Ui, look: &Look, commands: &mut Vec<String>) {
    ui.horizontal(|ui| {
        ui.heading(&look.room.name);
        ui.weak(&look.room.id);
    });
    ui.label(&look.room.description);

    ui.horizontal_wrapped(|ui| {
        ui.label("Exits:");
        if look.room.exits.is_empty() {
            ui.label("-");
        }
        for (direction, room) in &look.room.exits {
            if ui.button(format!("{direction} -> {room}")).clicked() {
                commands.push(format!("MOVE {direction}"));
                push_refresh(commands);
            }
        }
    });
    ui.label(format!("Players: {}", join_or_dash(&look.players)));
    ui.horizontal_wrapped(|ui| {
        ui.label("Items:");
        if look.items.is_empty() {
            ui.label("-");
        }
        for item in &look.items {
            let button = ui.button(format!("Take {}", display_name(item)));
            if button.on_hover_text(item).clicked() {
                commands.push(format!("TAKE {item}"));
                push_refresh(commands);
            }
        }
    });
    if look.npcs.is_empty() {
        ui.label("NPCs: -");
    }
    for npc in &look.npcs {
        ui.horizontal(|ui| {
            ui.label(format!("NPC {}:", display_name(npc)))
                .on_hover_text(npc);
            for action in ["TALK", "ATTACK", "QUEST"] {
                if ui.button(action).clicked() {
                    commands.push(format!("{action} {npc}"));
                    if action == "ATTACK" {
                        // New health, and the NPC may be gone from the room.
                        commands.push("STATUS".to_string());
                        push_refresh(commands);
                    }
                }
            }
        });
    }
}

/// Draws the last thing an NPC said, in a framed box.
fn show_dialogue(ui: &mut egui::Ui, npc: &str, text: &str) {
    ui.group(|ui| {
        ui.strong(format!("{} says:", display_name(npc)));
        ui.label(format!("\u{201c}{text}\u{201d}"));
    });
}

/// Draws the player panel: health bar, last attack and known quests.
fn show_character(
    ui: &mut egui::Ui,
    health: Option<&Health>,
    last_attack: Option<&(String, AttackResult)>,
    quests: &[QuestInfo],
) {
    ui.horizontal(|ui| {
        ui.label("HP:");
        match health {
            Some(h) if h.max_hp > 0 => {
                let fraction = (h.hp as f32 / h.max_hp as f32).clamp(0.0, 1.0);
                let text = format!("{} / {} ({})", h.hp, h.max_hp, h.status);
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .text(text)
                        .desired_width(220.0),
                );
            }
            _ => {
                ui.label("?");
            }
        }
    });
    if let Some((npc, result)) = last_attack {
        ui.label(format!(
            "You hit {} for {} damage: {} HP left ({}).",
            display_name(npc),
            result.damage,
            result.target_hp,
            result.status
        ));
    }
    if quests.is_empty() {
        ui.label("Quests: -");
    }
    for quest in quests {
        let mut text = format!("Quest {}: {}", display_name(&quest.quest_id), quest.status);
        if !quest.progress.is_empty() {
            text.push_str(&format!(" ({})", quest.progress));
        }
        if !quest.description.is_empty() {
            text.push_str(&format!(" - {}", quest.description));
        }
        if !quest.reward.is_empty() {
            text.push_str(&format!(" Reward: {}.", quest.reward));
        }
        ui.label(text);
    }
}

/// Draws the GROUP buttons. INVITE takes a player name, JOIN a group id.
fn show_group(ui: &mut egui::Ui, target: &mut String, commands: &mut Vec<String>) {
    ui.horizontal(|ui| {
        ui.label("Group:");
        if ui.button("CREATE").clicked() {
            commands.push("GROUP CREATE".to_string());
        }
        if ui.button("LEAVE").clicked() {
            commands.push("GROUP LEAVE".to_string());
        }
        ui.add(egui::TextEdit::singleline(target).hint_text("player or group id"));
        let target = target.trim();
        if ui.button("INVITE").clicked() {
            commands.push(format!("GROUP INVITE {target}"));
        }
        if ui.button("JOIN").clicked() {
            commands.push(format!("GROUP JOIN {target}"));
        }
    });
}

fn main() -> ExitCode {
    let (host, port) = match parse_args() {
        Ok(hp) => hp,
        Err(e) => {
            eprintln!("Error: {e}");
            eprintln!("Usage: tap_gui [host] [port]");
            return ExitCode::FAILURE;
        }
    };

    let options = eframe::NativeOptions::default();
    let result = eframe::run_native(
        "TAP",
        options,
        Box::new(move |cc| Ok(Box::new(TapApp::new(&cc.egui_ctx, &host, port)))),
    );

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("GUI error: {e}");
            ExitCode::FAILURE
        }
    }
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
    fn display_name_makes_ids_readable() {
        assert_eq!(display_name("item.loaf_bread"), "Loaf Bread");
        assert_eq!(display_name("torch"), "Torch");
        assert_eq!(display_name("cell_key"), "Cell Key");
        assert_eq!(display_name("npc.old_merchant"), "Old Merchant");
    }

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
