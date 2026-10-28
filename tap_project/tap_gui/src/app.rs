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
    /// it answers, an event updates the window, then the line goes to the log.
    pub fn receive(&mut self, line: String) {
        if line == CONNECTION_CLOSED {
            // Nothing can be sent anymore: buttons now log "Not connected".
            self.writer = None;
            self.pending.clear();
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
}
