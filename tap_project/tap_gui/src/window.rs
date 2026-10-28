//! The window layout: which panel shows what. The pieces come from `view.rs`.

use eframe::egui;

use crate::app::TapApp;
use crate::chat::show_chat;
use crate::group_view::{show_group, show_group_name, show_invite};
use crate::item_view::show_item_field;
use crate::log_panel::show_log;
use crate::map::show_map;
use crate::view::{
    push_refresh, show_character, show_counters, show_dialogue, show_inventory, show_room,
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

        // Left: the player (health, last attack, quests, group, map, inventory,
        // item field).
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
                show_group_name(ui, self.group.as_deref(), self.group_news.as_deref());
                show_invite(ui, &mut self.invite, &mut commands);
                ui.separator();
                ui.strong("Map");
                show_map(ui, &self.map);
                ui.separator();
                show_inventory(ui, self.inventory.as_deref(), &mut commands);
                show_item_field(ui, &mut self.item_input, &mut commands);
            });

        // Right: the chat, then every line sent and received, coloured by kind.
        egui::Panel::right("log")
            .default_size(340.0)
            .show(ui, |ui| {
                ui.heading("Chat");
                show_chat(
                    ui,
                    &self.chat,
                    &mut self.chat_scope,
                    &mut self.chat_input,
                    &mut commands,
                );
                ui.separator();
                ui.heading("Log");
                show_log(ui, &self.log);
            });

        // Center: the room and the last NPC dialogue.
        egui::CentralPanel::default().show(ui, |ui| {
            // The picture makes the room taller: on a small window it scrolls.
            egui::ScrollArea::vertical().show(ui, |ui| match &self.room {
                Some(look) => {
                    // A new room fades in; egui redraws until it is done.
                    let now = ui.input(|i| i.time);
                    let opacity = self.fade.opacity(&look.room.id, now);
                    if opacity < 1.0 {
                        ui.ctx().request_repaint();
                    }
                    ui.set_opacity(opacity);
                    show_room(ui, look, &mut commands);
                    if let Some((npc, text)) = &self.dialogue {
                        show_dialogue(ui, npc, text);
                    }
                }
                None => {
                    ui.label("Type a name and click Connect to enter the world.");
                }
            });
        });

        for command in commands {
            self.send(&command);
        }
    }
}
