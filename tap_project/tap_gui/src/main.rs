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
#[derive(Deserialize)]
struct RoomInfo {
    id: String,
    name: String,
    description: String,
    /// Direction -> room id. A BTreeMap keeps the directions sorted.
    exits: BTreeMap<String, String>,
}

/// Whole JSON of a LOOK reply (RFC 5.1.2).
#[derive(Deserialize)]
struct Look {
    room: RoomInfo,
    players: Vec<String>,
    items: Vec<String>,
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

    /// Asks the server for the room and the inventory again.
    fn refresh(&mut self) {
        self.send("LOOK");
        self.send("INVENTORY");
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
                }
                self.log.push(line);
            }
        }

        // Buttons only record the click here: commands are sent after drawing,
        // because the drawing code already borrows `self.name` and `self.log`.
        let mut connect_clicked = false;
        let mut look_clicked = false;
        let mut inventory_clicked = false;
        let mut take_item: Option<String> = None;
        let mut drop_item: Option<String> = None;

        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("TAP");
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut self.name);
                connect_clicked = ui.button("Connect").clicked();
            });
            ui.separator();
            if let Some(look) = &self.room {
                take_item = show_room(ui, look);
                ui.separator();
            }
            ui.horizontal(|ui| {
                look_clicked = ui.button("Look").clicked();
                inventory_clicked = ui.button("Inventory").clicked();
                if let Some(items) = &self.inventory {
                    if items.is_empty() {
                        ui.label("-");
                    }
                    for item in items {
                        if ui.button(format!("Drop {item}")).clicked() {
                            drop_item = Some(item.clone());
                        }
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &self.log {
                        ui.label(line);
                    }
                });
        });

        if connect_clicked {
            let name = self.name.trim().to_string();
            self.send(&format!("CONNECT {name}"));
            self.refresh();
        }
        if look_clicked {
            self.send("LOOK");
        }
        if inventory_clicked {
            self.send("INVENTORY");
        }
        if let Some(item) = take_item {
            self.send(&format!("TAKE {item}"));
            self.refresh();
        }
        if let Some(item) = drop_item {
            self.send(&format!("DROP {item}"));
            self.refresh();
        }
    }
}

/// Draws the room panel from the last LOOK reply.
/// Returns the item whose Take button was clicked, if any.
fn show_room(ui: &mut egui::Ui, look: &Look) -> Option<String> {
    ui.horizontal(|ui| {
        ui.heading(&look.room.name);
        ui.weak(&look.room.id);
    });
    ui.label(&look.room.description);

    let exits: Vec<String> = look
        .room
        .exits
        .iter()
        .map(|(direction, room)| format!("{direction} -> {room}"))
        .collect();
    ui.label(format!("Exits: {}", join_or_dash(&exits)));
    ui.label(format!("Players: {}", join_or_dash(&look.players)));
    let mut take_item = None;
    ui.horizontal(|ui| {
        ui.label("Items:");
        if look.items.is_empty() {
            ui.label("-");
        }
        for item in &look.items {
            if ui.button(format!("Take {item}")).clicked() {
                take_item = Some(item.clone());
            }
        }
    });
    ui.label(format!("NPCs: {}", join_or_dash(&look.npcs)));
    take_item
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
}
