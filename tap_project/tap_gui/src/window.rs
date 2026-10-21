//! The window layout: which panel shows what. The pieces come from `view.rs`.

use eframe::egui;

use crate::app::TapApp;
use crate::view::{
    push_refresh, show_character, show_counters, show_dialogue, show_group, show_inventory,
    show_room,
};

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

        // Panels are added from the outside in: top, bottom, left and right
        // first, then the central panel takes the space that is left.
        egui::Panel::top("top").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("TAP");
                ui.separator();
                ui.label("Name:");
                ui.text_edit_singleline(&mut self.name);
                if ui.button("Connect").clicked() {
                    commands.push(format!("CONNECT {}", self.name.trim()));
                    push_refresh(&mut commands);
                    commands.push("STATUS".to_string());
                    commands.push("QUESTS".to_string());
                }
            });
            show_counters(ui, self.room.as_ref(), self.server_players);
        });

        // Bottom: the actions that do not depend on the room.
        egui::Panel::bottom("actions").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for command in ["LOOK", "INVENTORY", "STATUS", "QUESTS", "WHO", "QUIT"] {
                    if ui.button(command).clicked() {
                        commands.push(command.to_string());
                    }
                }
            });
            show_group(ui, &mut self.group_target, &mut commands);
            ui.add_space(4.0);
        });

        // Left: the player (health, last attack, quests, inventory).
        egui::Panel::left("player")
            .default_size(260.0)
            .show(ui, |ui| {
                ui.heading("Player");
                show_character(
                    ui,
                    self.health.as_ref(),
                    self.last_attack.as_ref(),
                    &self.quests,
                );
                ui.separator();
                show_inventory(ui, self.inventory.as_deref(), &mut commands);
            });

        // Right: every line sent and received, as before.
        egui::Panel::right("log")
            .default_size(340.0)
            .show(ui, |ui| {
                ui.heading("Log");
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        for line in &self.log {
                            ui.label(line);
                        }
                    });
            });

        // Center: the room and the last NPC dialogue.
        egui::CentralPanel::default().show(ui, |ui| match &self.room {
            Some(look) => {
                show_room(ui, look, &mut commands);
                if let Some((npc, text)) = &self.dialogue {
                    show_dialogue(ui, npc, text);
                }
            }
            None => {
                ui.label("Type a name and click Connect to enter the world.");
            }
        });

        for command in commands {
            self.send(&command);
        }
    }
}
