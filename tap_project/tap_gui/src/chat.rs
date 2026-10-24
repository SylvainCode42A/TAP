//! The chat panel: messages of CHAT GLOBAL / ROOM / GROUP (RFC 5.2.1 and
//! 6.2), apart from the log, and the field to write one.

use eframe::egui;

/// Scopes of CHAT, in the order of their buttons.
const SCOPES: [&str; 3] = ["GLOBAL", "ROOM", "GROUP"];

/// One chat message: from an `EVT ... CHAT` line, or our own once the
/// server accepted it.
pub struct ChatLine {
    pub scope: String,
    pub from: String,
    pub text: String,
}

impl ChatLine {
    /// The line as shown: `[ROOM] bob: hello`.
    fn shown(&self) -> String {
        format!("[{}] {}: {}", self.scope, self.from, self.text)
    }
}

/// The command for the message typed in the field, or None when it is empty.
fn chat_command(scope: &str, input: &str) -> Option<String> {
    let text = input.trim();
    if text.is_empty() {
        return None;
    }
    Some(format!("CHAT {scope} {text}"))
}

/// Draws the messages, newest at the bottom, then the scope buttons and the
/// field. Enter or Send adds the CHAT command and empties the field.
pub fn show_chat(
    ui: &mut egui::Ui,
    chat: &[ChatLine],
    scope: &mut String,
    input: &mut String,
    commands: &mut Vec<String>,
) {
    // Its own id: the log below has a scroll area too.
    egui::ScrollArea::vertical()
        .id_salt("chat")
        .stick_to_bottom(true)
        .auto_shrink(false)
        .max_height(220.0)
        .show(ui, |ui| {
            if chat.is_empty() {
                ui.weak("No message yet.");
            }
            for line in chat {
                ui.label(line.shown());
            }
        });
    ui.horizontal(|ui| {
        for name in SCOPES {
            ui.selectable_value(scope, name.to_string(), name);
        }
    });
    ui.horizontal(|ui| {
        let field = ui.add(egui::TextEdit::singleline(input).hint_text("message"));
        // Enter in the field sends too: egui then takes the focus away from it.
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (ui.button("Send").clicked() || enter)
            && let Some(command) = chat_command(scope, input)
        {
            commands.push(command);
            input.clear();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::TapApp;

    #[test]
    fn empty_messages_are_not_sent() {
        assert_eq!(
            chat_command("ROOM", "  hello all "),
            Some("CHAT ROOM hello all".to_string())
        );
        assert_eq!(chat_command("GLOBAL", "   "), None);
    }

    #[test]
    fn chat_comes_from_events_and_from_our_accepted_messages() {
        let mut app = TapApp::empty();
        app.receive("EVT ROOM CHAT bob hi there".to_string());
        app.pending.push_back("CHAT GROUP hello".to_string());
        app.pending.push_back("CHAT GLOBAL hello all".to_string());
        // Not in a group: our message was refused, it is not in the chat.
        app.receive("ERR 401 NOT_IN_GROUP".to_string());
        app.receive("OK".to_string());

        let shown: Vec<String> = app.chat.iter().map(ChatLine::shown).collect();
        assert_eq!(shown, ["[ROOM] bob: hi there", "[GLOBAL] You: hello all"]);
    }
}
