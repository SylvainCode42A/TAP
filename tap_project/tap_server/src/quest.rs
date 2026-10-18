use crate::combat::{self, MAX_HP};
use crate::{GameState, Player, Quest, Response};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum QuestStatus {
    Active,
    Completed,
}

// (conditions met, conditions needed) for this player.
fn progress(player: &Player, quest: &Quest) -> (usize, usize) {
    let mut done = 0;
    let mut needed = 0;
    if let Some(item) = &quest.requires_item {
        needed += 1;
        if player.inventory.contains(item) {
            done += 1;
        }
    }
    if let Some(npc) = &quest.requires_kill {
        needed += 1;
        if player.kills.contains(npc) {
            done += 1;
        }
    }
    (done, needed)
}

fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn quest_json(id: &str, quest: &Quest, status: &str) -> String {
    format!(
        "{{\"quest_id\": {}, \"description\": {}, \"reward\": {}, \"status\": \"{}\"}}",
        json_string(id),
        json_string(&quest.description),
        json_string(&quest.rewards.join(", ")),
        status
    )
}

// First call accepts the NPC's next quest, a later call turns it in once its conditions hold.
pub fn quest(gs: &mut GameState, username: &str, npc_query: &str) -> Response {
    let room = match gs.players.get(username) {
        None => return Response::Err { code: 404, message: "Player not CONNECTED".to_string() },
        Some(player) => player.current_room.clone(),
    };
    let npc_id = match combat::find_npc_in_room(gs, &room, npc_query) {
        None => return Response::Err { code: 404, message: "NPC_NOT_FOUND".to_string() },
        Some(id) => id,
    };
    let mut quest_ids: Vec<&String> = gs.world.world.quests.iter()
        .filter(|(_, quest)| quest.giver == npc_id)
        .map(|(id, _)| id)
        .collect();
    quest_ids.sort();

    let Some(player) = gs.players.get_mut(username) else {
        return Response::Err { code: 404, message: "Player not CONNECTED".to_string() };
    };

    if let Some(id) = quest_ids.iter().find(|id| player.quests.get(**id) == Some(&QuestStatus::Active)) {
        let quest = &gs.world.world.quests[*id];
        let (done, needed) = progress(player, quest);
        if done < needed {
            return Response::Ok(quest_json(id, quest, "active"));
        }
        player.quests.insert((*id).clone(), QuestStatus::Completed);
        player.hp = MAX_HP;
        return Response::Ok(quest_json(id, quest, "completed"));
    }

    if let Some(id) = quest_ids.iter().find(|id| !player.quests.contains_key(**id)) {
        player.quests.insert((*id).clone(), QuestStatus::Active);
        return Response::Ok(quest_json(id, &gs.world.world.quests[*id], "active"));
    }

    Response::Err { code: 406, message: "NO_QUEST_AVAILABLE".to_string() }
}

pub fn quests(gs: &GameState, username: &str) -> Response {
    let Some(player) = gs.players.get(username) else {
        return Response::Err { code: 404, message: "Player not CONNECTED".to_string() };
    };
    let mut ids: Vec<&String> = player.quests.keys().collect();
    ids.sort();
    let entries: Vec<String> = ids.into_iter()
        .filter_map(|id| {
            let quest = gs.world.world.quests.get(id)?;
            Some(match player.quests[id] {
                QuestStatus::Active => {
                    let (done, needed) = progress(player, quest);
                    format!(
                        "{{\"quest_id\": {}, \"status\": \"active\", \"progress\": \"{}/{}\"}}",
                        json_string(id), done, needed
                    )
                }
                QuestStatus::Completed => {
                    format!("{{\"quest_id\": {}, \"status\": \"completed\"}}", json_string(id))
                }
            })
        })
        .collect();
    Response::Ok(format!("[{}]", entries.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{game_with_player, ok_body};

    #[test]
    fn quest_from_absent_npc_is_404() {
        let mut gs = game_with_player("alice", "entrance");
        assert!(matches!(quest(&mut gs, "alice", "prisoner"), Response::Err { code: 404, .. }));
    }

    #[test]
    fn free_prisoner_accept_then_complete() {
        let mut gs = game_with_player("alice", "secret_cell");
        assert!(ok_body(quest(&mut gs, "alice", "Prisoner")).contains("\"status\": \"active\""));
        assert!(ok_body(quest(&mut gs, "alice", "prisoner")).contains("\"status\": \"active\""));
        assert!(ok_body(quests(&gs, "alice")).contains("\"progress\": \"0/1\""));

        gs.players.get_mut("alice").unwrap().inventory.push("cell_key".to_string());
        assert!(ok_body(quest(&mut gs, "alice", "prisoner")).contains("\"status\": \"completed\""));
        assert!(matches!(quest(&mut gs, "alice", "prisoner"), Response::Err { code: 406, .. }));
        assert!(ok_body(quests(&gs, "alice")).contains("\"status\": \"completed\""));
    }

    #[test]
    fn treasure_quest_needs_kill_and_item() {
        let mut gs = game_with_player("alice", "entrance");
        quest(&mut gs, "alice", "merchant");
        let player = gs.players.get_mut("alice").unwrap();
        player.kills.push("monster".to_string());
        assert!(ok_body(quests(&gs, "alice")).contains("\"progress\": \"1/2\""));
        gs.players.get_mut("alice").unwrap().inventory.push("treasure".to_string());
        assert!(ok_body(quest(&mut gs, "alice", "merchant")).contains("\"status\": \"completed\""));
    }
}
