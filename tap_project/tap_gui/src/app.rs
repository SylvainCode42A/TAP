//! Window state: commands sent to the server and the replies they get.
//! What each reply changes in the window is in `replies.rs`.

use std::collections::VecDeque;
use std::io::Write;
use std::net::TcpStream;
use std::sync::mpsc::Receiver;

use eframe::egui;

use crate::chat::ChatLine;
use crate::events::parse_event;
use crate::fade::RoomFade;
use crate::map::MiniMap;
use crate::network::{CONNECTION_CLOSED, spawn_reader};
use crate::protocol::{AttackResult, Health, Look, QuestInfo, is_reply};

/// State of the GUI window.
pub struct TapApp {
    /// Lines coming from the network thread, None if the connection failed.
    pub receiver: Option<Receiver<String>>,
    /// Write side of the connection, None if the connection failed.
    pub writer: Option<TcpStream>,
    /// Server address given on the command line, kept to connect again.
    pub host: String,
    /// Server port given on the command line.
    pub port: u16,
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
    /// Player or leader name typed for GROUP INVITE / GROUP JOIN.
    pub group_target: String,
    /// Id of our group, from the last GROUP CREATE / JOIN / LEAVE reply.
    pub group: Option<String>,
    /// Last change in our group made by someone else, from EVT GROUP.
    pub group_news: Option<String>,
    /// Leader of the group we are invited to, from EVT GROUP INVITE.
    pub invite: Option<String>,
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
    /// Rooms seen so far, for the mini-map.
    pub map: MiniMap,
    /// Fade-in of the room panel when the room changes.
    pub fade: RoomFade,
    /// Chat messages, oldest first.
    pub chat: Vec<ChatLine>,
    /// Scope chosen for the next message: GLOBAL, ROOM or GROUP.
    pub chat_scope: String,
    /// Message typed in the chat field.
    pub chat_input: String,
    /// Item name or id typed in the item field, for TAKE / DROP.
    pub item_input: String,
}

impl TapApp {
    /// A window with no connection and nothing received yet.
    pub fn empty() -> TapApp {
        TapApp {
            receiver: None,
            writer: None,
            host: String::new(),
            port: 0,
            log: vec![],
            name: String::new(),
            room: None,
            inventory: None,
            server_players: None,
            group_target: String::new(),
            group: None,
            group_news: None,
            invite: None,
            pending: VecDeque::new(),
            dialogue: None,
            health: None,
            last_attack: None,
            quests: vec![],
            map: MiniMap::default(),
            fade: RoomFade::default(),
            chat: vec![],
            chat_scope: "GLOBAL".to_string(),
            chat_input: String::new(),
            item_input: String::new(),
        }
    }

    /// Opens the window and connects to the server at `host:port`.
    pub fn new(ctx: &egui::Context, host: &str, port: u16) -> TapApp {
        let mut app = TapApp::empty();
        app.host = host.to_string();
        app.port = port;
        app.open_connection(ctx);
        app
    }

    /// Connects to the server and starts the network thread. Called when the
    /// window opens, and again by Connect after the connection was closed.
    pub fn open_connection(&mut self, ctx: &egui::Context) {
        let (host, port) = (self.host.clone(), self.port);
        let stream = match TcpStream::connect((host.as_str(), port)) {
            Ok(s) => s,
            Err(e) => {
                self.log
                    .push(format!("Connection to {host}:{port} failed: {e}"));
                return;
            }
        };
        match stream.try_clone() {
            Ok(reader_stream) => {
                self.receiver = Some(spawn_reader(reader_stream, ctx.clone()));
                self.writer = Some(stream);
                self.log.push(format!("Connected to {host}:{port}"));
            }
            Err(e) => self.log.push(format!("Cannot clone connection: {e}")),
        }
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

    /// Back to the start screen, as if the window had just opened without a
    /// connection: the room, map, inventory, quests, group and chat are
    /// forgotten. Only the log, the typed name and the server address are kept.
    pub fn reset_world(&mut self) {
        let log = std::mem::take(&mut self.log);
        let name = std::mem::take(&mut self.name);
        let host = std::mem::take(&mut self.host);
        *self = TapApp {
            log,
            name,
            host,
            port: self.port,
            ..TapApp::empty()
        };
    }

    /// Handles one line from the server: a reply is matched with the command
    /// it answers, an event updates the window, then the line goes to the log.
    pub fn receive(&mut self, line: String) {
        if line == CONNECTION_CLOSED {
            // Nothing can be sent anymore: buttons now log "Not connected",
            // and the window goes back to its start screen.
            self.reset_world();
        }
        let command = if is_reply(&line) {
            self.pending.pop_front()
        } else {
            None
        };
        if let Some(command) = command {
            self.handle_reply(&command, &line);
        }
        if let Some(event) = parse_event(&line) {
            self.handle_event(event);
        }
        self.log.push(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_connection_stops_sending() {
        let mut app = TapApp::empty();
        app.pending.push_back("LOOK".to_string());
        app.receive(CONNECTION_CLOSED.to_string());
        assert!(app.writer.is_none());
        assert!(app.pending.is_empty());

        app.send("LOOK");
        assert_eq!(app.log.last().map(String::as_str), Some("Not connected"));
    }

    #[test]
    fn closed_connection_goes_back_to_the_start_screen() {
        let mut app = TapApp::empty();
        app.name = "alice".to_string();
        app.host = "127.0.0.1".to_string();
        app.port = 7878;
        app.inventory = Some(vec!["sword".to_string()]);
        app.group = Some("g1".to_string());
        app.log.push("OK bye".to_string());
        app.receive(CONNECTION_CLOSED.to_string());

        assert!(app.room.is_none());
        assert!(app.inventory.is_none());
        assert!(app.group.is_none());
        assert_eq!(app.name, "alice");
        assert_eq!((app.host.as_str(), app.port), ("127.0.0.1", 7878));
        assert_eq!(app.log, vec!["OK bye", CONNECTION_CLOSED]);
    }
}
