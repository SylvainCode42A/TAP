//! The item field: TAKE or DROP an item typed by its id or its display
//! name (subject V.4, RFC 8.3). The buttons of the room and the inventory
//! always send the id; this field lets the player type either one.

use eframe::egui;

use crate::view::push_refresh;

/// Builds `TAKE <item>` or `DROP <item>` from what the player typed, or None
/// if the field is empty. Every word is kept, so `Cell Key` works (RFC 8.4).
pub fn item_command(verb: &str, typed: &str) -> Option<String> {
    let item = typed.split_whitespace().collect::<Vec<_>>().join(" ");
    if item.is_empty() {
        None
    } else {
        Some(format!("{verb} {item}"))
    }
}

/// Draws the field and its TAKE and DROP buttons, greyed out while it is empty.
/// After a click the field is emptied and the room and inventory are asked again.
pub fn show_item_field(ui: &mut egui::Ui, input: &mut String, commands: &mut Vec<String>) {
    ui.horizontal_wrapped(|ui| {
        ui.add(
            egui::TextEdit::singleline(input)
                .hint_text("item name or id")
                .desired_width(120.0),
        );
        for verb in ["TAKE", "DROP"] {
            let command = item_command(verb, input);
            let button = ui.add_enabled(command.is_some(), egui::Button::new(verb));
            if let (true, Some(command)) = (button.clicked(), command) {
                commands.push(command);
                push_refresh(commands);
                input.clear();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_command_keeps_the_name_or_id_typed() {
        assert_eq!(
            item_command("TAKE", "cell_key"),
            Some("TAKE cell_key".to_string())
        );
        assert_eq!(
            item_command("TAKE", "Sword"),
            Some("TAKE Sword".to_string())
        );
        assert_eq!(
            item_command("DROP", "  Cell   Key "),
            Some("DROP Cell Key".to_string())
        );
    }

    #[test]
    fn empty_field_sends_nothing() {
        assert_eq!(item_command("TAKE", ""), None);
        assert_eq!(item_command("DROP", "   "), None);
    }
}
