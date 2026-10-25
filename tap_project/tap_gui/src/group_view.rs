//! The group zone: our group and its last news, the invitation we got,
//! and the GROUP buttons (RFC 5.3).

use eframe::egui;

/// Draws the group we are in, or `-`, from our own GROUP replies, and the
/// last change made by someone else (EVT GROUP JOIN / LEAVE).
pub fn show_group_name(ui: &mut egui::Ui, group: Option<&str>, news: Option<&str>) {
    let mut text = format!("Group: {}", group.unwrap_or("-"));
    if let Some(news) = news {
        text.push_str(&format!(" ({news})"));
    }
    ui.label(text);
}

/// Draws the invitation to a group, if any: Join sends GROUP JOIN with the
/// leader's name (RFC 5.3.3), Ignore only hides it. Both close it.
pub fn show_invite(ui: &mut egui::Ui, invite: &mut Option<String>, commands: &mut Vec<String>) {
    let Some(leader) = invite.clone() else {
        return;
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("{leader} invited you to a group."));
        if ui.button(format!("Join {leader}")).clicked() {
            commands.push(format!("GROUP JOIN {leader}"));
            *invite = None;
        }
        if ui.button("Ignore").clicked() {
            *invite = None;
        }
    });
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
