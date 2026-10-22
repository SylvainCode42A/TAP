//! Drawing the window: room, player, counters and buttons.

use eframe::egui;

use crate::app::TapApp;
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

/// Adds LOOK, INVENTORY and WHO, so the room, the inventory and the counters
/// are shown again.
fn push_refresh(commands: &mut Vec<String>) {
    commands.push("LOOK".to_string());
    commands.push("INVENTORY".to_string());
    commands.push("WHO".to_string());
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
}
