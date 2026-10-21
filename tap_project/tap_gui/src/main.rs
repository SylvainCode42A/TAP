use std::collections::BTreeMap;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use eframe::egui;
use serde::Deserialize;

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
}

impl TapApp {
    /// Connects to the server and starts the network thread.
    fn new(ctx: &egui::Context, host: &str, port: u16) -> TapApp {
        let mut app = TapApp {
            receiver: None,
            writer: None,
            log: vec![],
            name: String::new(),
            room: None,
            inventory: None,
            server_players: None,
            group_target: String::new(),
        };
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
            Ok(()) => self.log.push(format!("> {command}")),
            Err(e) => self.log.push(format!("Send error: {e}")),
        }
    }
}

impl eframe::App for TapApp {
    /// Called on every frame: draws the whole window from the current state.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Move every line received since the last frame into the log.
        if let Some(receiver) = &self.receiver {
            while let Ok(line) = receiver.try_recv() {
                if let Some(look) = parse_look(&line) {
                    self.room = Some(look);
                } else if let Some(items) = parse_inventory(&line) {
                    self.inventory = Some(items);
                } else if let Some(count) = parse_who(&line) {
                    self.server_players = Some(count);
                }
                self.log.push(line);
            }
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
                }
            });
            ui.separator();
            show_counters(ui, self.room.as_ref(), self.server_players);
            if let Some(look) = &self.room {
                show_room(ui, look, &mut commands);
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
                }
            }
        });
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
    fn parse_who_reads_player_count() {
        assert_eq!(parse_who("OK players=3"), Some(3));
        assert_eq!(parse_who("OK players=0"), Some(0));
        assert!(parse_who("OK players=abc").is_none());
        assert!(parse_who("OK taken=torch").is_none());
    }
}
