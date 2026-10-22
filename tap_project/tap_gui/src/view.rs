//! Drawing helpers: room, player, counters and buttons.
//! `window.rs` places them in the panels of the window.

use eframe::egui;

use crate::images::{icon, room_image};
use crate::protocol::{AttackResult, Health, Look, QuestInfo};

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

/// Draws the small picture of an item or NPC, when our world has one.
fn show_icon(ui: &mut egui::Ui, id: &str, size: f32) {
    if let Some(image) = icon(id) {
        ui.add(
            egui::Image::new(image)
                .fit_to_exact_size(egui::vec2(size, size))
                .corner_radius(4.0),
        );
    }
}

/// Joins a list with commas, or gives `-` when it is empty.
fn join_or_dash(list: &[String]) -> String {
    if list.is_empty() {
        "-".to_string()
    } else {
        list.join(", ")
    }
}

/// Draws the player counters: in this room (from LOOK) and on the server (from WHO).
/// Shows `?` while the reply has not arrived yet.
pub fn show_counters(ui: &mut egui::Ui, look: Option<&Look>, server_players: Option<usize>) {
    let in_room = look.map_or("?".to_string(), |l| l.players.len().to_string());
    let on_server = server_players.map_or("?".to_string(), |n| n.to_string());
    ui.label(format!(
        "Players in this room: {in_room}   |   Players on the server: {on_server}"
    ));
}

/// Draws the room panel from the last LOOK reply, with one button per action.
pub fn show_room(ui: &mut egui::Ui, look: &Look, commands: &mut Vec<String>) {
    ui.horizontal(|ui| {
        ui.heading(&look.room.name);
        ui.weak(&look.room.id);
    });
    ui.label(&look.room.description);
    if let Some(image) = room_image(&look.room.id) {
        // As wide as the panel, but never taller than 320 points.
        ui.add(egui::Image::new(image).max_height(320.0).corner_radius(6.0));
    }

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
            show_icon(ui, item, 32.0);
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
            show_icon(ui, npc, 48.0);
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
                    if action == "QUEST" {
                        // A finished quest may heal, and its progress changed.
                        commands.push("STATUS".to_string());
                        commands.push("QUESTS".to_string());
                    }
                }
            }
        });
    }
}

/// Draws the inventory, one Drop button per item.
/// Shows `?` before the first INVENTORY reply and `-` when it is empty.
pub fn show_inventory(ui: &mut egui::Ui, inventory: Option<&[String]>, commands: &mut Vec<String>) {
    ui.strong("Inventory");
    match inventory {
        None => {
            ui.label("?");
        }
        Some([]) => {
            ui.label("-");
        }
        Some(items) => {
            for item in items {
                ui.horizontal(|ui| {
                    show_icon(ui, item, 32.0);
                    let button = ui.button(format!("Drop {}", display_name(item)));
                    if button.on_hover_text(item).clicked() {
                        commands.push(format!("DROP {item}"));
                        push_refresh(commands);
                    }
                });
            }
        }
    }
}

/// Draws the last thing an NPC said, in a framed box.
pub fn show_dialogue(ui: &mut egui::Ui, npc: &str, text: &str) {
    ui.group(|ui| {
        ui.strong(format!("{} says:", display_name(npc)));
        ui.label(format!("\u{201c}{text}\u{201d}"));
    });
}

/// Draws the player panel: health bar, last attack and known quests.
pub fn show_character(
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
                        .fill(health_color(fraction))
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

/// Colour of the health bar: green above 60 %, orange above 30 %, red below.
fn health_color(fraction: f32) -> egui::Color32 {
    if fraction > 0.6 {
        egui::Color32::from_rgb(76, 175, 80)
    } else if fraction > 0.3 {
        egui::Color32::from_rgb(245, 166, 35)
    } else {
        egui::Color32::from_rgb(229, 57, 53)
    }
}

/// Draws the group we are in, or `-`. Known from our own GROUP replies only:
/// the server sends no event yet when the group changes (EVT GROUP).
pub fn show_group_name(ui: &mut egui::Ui, group: Option<&str>) {
    ui.label(format!("Group: {}", group.unwrap_or("-")));
}

/// Draws the GROUP buttons. INVITE takes a player name, JOIN the group leader's
/// name (RFC 5.3.3).
pub fn show_group(ui: &mut egui::Ui, target: &mut String, commands: &mut Vec<String>) {
    ui.horizontal(|ui| {
        ui.label("Group:");
        if ui.button("CREATE").clicked() {
            commands.push("GROUP CREATE".to_string());
        }
        if ui.button("LEAVE").clicked() {
            commands.push("GROUP LEAVE".to_string());
        }
        ui.add(egui::TextEdit::singleline(target).hint_text("player or leader name"));
        let target = target.trim();
        if ui.button("INVITE").clicked() {
            commands.push(format!("GROUP INVITE {target}"));
        }
        if ui.button("JOIN").clicked() {
            commands.push(format!("GROUP JOIN {target}"));
        }
    });
}

/// Adds LOOK, INVENTORY and WHO, so the room, the inventory and the counters
/// are shown again.
pub fn push_refresh(commands: &mut Vec<String>) {
    commands.push("LOOK".to_string());
    commands.push("INVENTORY".to_string());
    commands.push("WHO".to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_makes_ids_readable() {
        assert_eq!(display_name("item.loaf_bread"), "Loaf Bread");
        assert_eq!(display_name("torch"), "Torch");
        assert_eq!(display_name("cell_key"), "Cell Key");
        assert_eq!(display_name("npc.old_merchant"), "Old Merchant");
    }

    #[test]
    fn health_bar_changes_colour() {
        assert_eq!(health_color(1.0), egui::Color32::from_rgb(76, 175, 80));
        assert_eq!(health_color(0.5), egui::Color32::from_rgb(245, 166, 35));
        assert_eq!(health_color(0.1), egui::Color32::from_rgb(229, 57, 53));
    }
}
