//! What each `EVT` line changes in the window, while we play: other players
//! coming and going, the player count, our group, the chat. The line is read in
//! `events.rs`.

use crate::app::TapApp;
use crate::chat::ChatLine;
use crate::events::Event;

impl TapApp {
    /// Updates the window from one event of the server.
    pub fn handle_event(&mut self, event: Event) {
        match event {
            Event::Enter(name) => {
                if let Some(look) = &mut self.room
                    && !look.players.contains(&name)
                {
                    look.players.push(name);
                }
            }
            Event::Leave(name) => {
                if let Some(look) = &mut self.room {
                    look.players.retain(|player| *player != name);
                }
            }
            Event::Players(count) => self.server_players = Some(count),
            Event::GroupJoin(name) => self.group_news = Some(format!("{name} joined")),
            Event::GroupLeave(name) => self.group_news = Some(format!("{name} left")),
            Event::Chat { scope, from, text } => self.chat.push(ChatLine { scope, from, text }),
            // Invitations get their own place at the next step.
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A window in the entrance with alice, as after a LOOK reply.
    fn app_in_entrance() -> TapApp {
        let mut app = TapApp::empty();
        app.pending.push_back("LOOK".to_string());
        app.receive(
            r#"OK {"room":{"id":"entrance","name":"Dungeon Entrance","exits":{}},"players":["alice"],"items":[],"npcs":[]}"#
                .to_string(),
        );
        app
    }

    fn players(app: &TapApp) -> Vec<String> {
        app.room
            .as_ref()
            .map(|look| look.players.clone())
            .unwrap_or_default()
    }

    #[test]
    fn players_come_and_go_in_the_room() {
        let mut app = app_in_entrance();
        app.receive("EVT ROOM PRESENCE ENTER bob".to_string());
        assert_eq!(players(&app), vec!["alice", "bob"]);
        // Twice the same name: bob is listed once.
        app.receive("EVT ROOM PRESENCE ENTER bob".to_string());
        assert_eq!(players(&app), vec!["alice", "bob"]);
        app.receive("EVT ROOM PRESENCE LEAVE bob".to_string());
        assert_eq!(players(&app), vec!["alice"]);
        // Events answer no command: the queue of commands is untouched.
        assert!(app.pending.is_empty());
    }

    #[test]
    fn stats_and_group_events_update_the_window() {
        let mut app = TapApp::empty();
        app.receive("EVT STATS players=4".to_string());
        assert_eq!(app.server_players, Some(4));
        app.receive("EVT GROUP JOIN bob".to_string());
        assert_eq!(app.group_news.as_deref(), Some("bob joined"));
        app.receive("EVT GROUP LEAVE bob".to_string());
        assert_eq!(app.group_news.as_deref(), Some("bob left"));
        // Every event still goes to the log.
        assert_eq!(app.log.len(), 3);
    }
}
