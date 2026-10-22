//! Quests: the `QUEST` and `QUESTS` commands.
//!
//! A quest is given by one NPC (`giver` in `world.yaml`) and is done when its
//! conditions hold: an item in the inventory (`requires_item`) and/or an NPC
//! beaten (`requires_kill`). The server checks them, never the client.
//!
//! Life of a quest, per player:
//!
//! ```text
//! (not taken) --QUEST npc--> active --QUEST npc, conditions met--> completed
//! ```
//!
//! - Owns: per-player quest state, progress, the JSON of `QUEST` and
//!   `QUESTS`.
//! - Does NOT own: NPC lookup ([`combat::find_npc_in_room`]), parsing,
//!   sending the reply.

use crate::combat::{self, MAX_HP};
use crate::{GameState, Player, Quest, Response};

// MARK: - Quest state

/// Where a player stands on one quest. A quest never taken has no entry.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum QuestStatus {
    /// Accepted, conditions not yet turned in.
    Active,
    /// Turned in and rewarded. Final: never goes back to `Active`.
    Completed,
}

// MARK: - Progress

/// Counts the conditions of `quest` that `player` meets.
///
/// Returns `(met, needed)`. A quest with no condition is `(0, 0)`: done as
/// soon as it is taken.
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

// MARK: - JSON helpers

/// Quotes `text` as a JSON string.
///
/// Escapes what JSON forbids raw inside a string: double quote, backslash,
/// control characters. Descriptions come from `world.yaml` and may contain
/// any of them.
fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Builds the reply body of `QUEST`, RFC 5.4.7:
/// `{"quest_id", "description", "reward", "status"}`.
fn quest_json(id: &str, quest: &Quest, status: &str) -> String {
    format!(
        "{{\"quest_id\": {}, \"description\": {}, \
         \"reward\": {}, \"status\": \"{}\"}}",
        json_string(id),
        json_string(&quest.description),
        json_string(&quest.rewards.join(", ")),
        status
    )
}

// MARK: - QUEST

/// Accepts or turns in the next quest of the NPC that `npc_query` names,
/// RFC 5.4.7.
///
/// One command does both, so no extra command is needed:
///
/// 1. an active quest of this NPC: turn it in if its conditions hold, else
///    report it still `active`;
/// 2. else a quest never taken: accept it, `active`;
/// 3. else nothing left: `406`.
///
/// Turning in fully heals the player. A quest never hands out an item:
/// items are unique in the world.
///
/// # Errors
///
/// - `404 Player not CONNECTED`: `username` is not in the game.
/// - `404 NPC_NOT_FOUND`: no NPC of the player's room matches `npc_query`.
/// - `406 NO_QUEST_AVAILABLE`: every quest of this NPC is completed, or it
///   gives none.
pub fn quest(gs: &mut GameState, username: &str, npc_query: &str) -> Response {
    // 1. Where is the player, and is the NPC in that room?
    let room = match gs.players.get(username) {
        None => return combat::not_connected(),
        Some(player) => player.current_room.clone(),
    };
    let npc_id = match combat::find_npc_in_room(gs, &room, npc_query) {
        None => return combat::npc_not_found(),
        Some(id) => id,
    };

    // 2. Every quest this NPC gives, sorted so the order never depends on
    //    the HashMap's internal layout.
    let mut quest_ids: Vec<String> = Vec::new();
    for (id, quest) in &gs.world.world.quests {
        if quest.giver == npc_id {
            quest_ids.push(id.clone());
        }
    }
    quest_ids.sort();

    let Some(player) = gs.players.get_mut(username) else {
        return combat::not_connected();
    };

    // 3. A quest already active: turn it in if its conditions hold.
    for id in &quest_ids {
        if player.quests.get(id) != Some(&QuestStatus::Active) {
            continue;
        }
        let quest = &gs.world.world.quests[id];
        let (done, needed) = progress(player, quest);
        if done < needed {
            return Response::Ok(quest_json(id, quest, "active"));
        }
        player.quests.insert(id.clone(), QuestStatus::Completed);
        player.hp = MAX_HP;
        return Response::Ok(quest_json(id, quest, "completed"));
    }

    // 4. A quest never taken: accept it.
    for id in &quest_ids {
        if !player.quests.contains_key(id) {
            player.quests.insert(id.clone(), QuestStatus::Active);
            let quest = &gs.world.world.quests[id];
            return Response::Ok(quest_json(id, quest, "active"));
        }
    }

    // 5. Nothing left from this NPC.
    Response::Err {
        code: 406,
        message: "NO_QUEST_AVAILABLE".to_string(),
    }
}

// MARK: - QUESTS

/// Lists every quest `username` has taken, RFC 5.4.8.
///
/// Reply body: a JSON array sorted by quest id. Active quests carry
/// `"progress": "met/needed"`, completed ones do not:
///
/// ```text
/// [{"quest_id": "free_prisoner", "status": "completed"},
///  {"quest_id": "treasure_quest", "status": "active", "progress": "1/2"}]
/// ```
///
/// # Errors
///
/// - `404 Player not CONNECTED`: `username` is not in the game.
pub fn quests(gs: &GameState, username: &str) -> Response {
    let Some(player) = gs.players.get(username) else {
        return combat::not_connected();
    };

    let mut ids: Vec<&String> = player.quests.keys().collect();
    ids.sort();

    let mut entries: Vec<String> = Vec::new();
    for id in ids {
        let Some(quest) = gs.world.world.quests.get(id) else {
            continue;
        };
        let entry = match player.quests[id] {
            QuestStatus::Active => {
                let (done, needed) = progress(player, quest);
                format!(
                    "{{\"quest_id\": {}, \"status\": \"active\", \
                     \"progress\": \"{}/{}\"}}",
                    json_string(id),
                    done,
                    needed
                )
            }
            QuestStatus::Completed => format!(
                "{{\"quest_id\": {}, \"status\": \"completed\"}}",
                json_string(id)
            ),
        };
        entries.push(entry);
    }

    Response::Ok(format!("[{}]", entries.join(", ")))
}

// MARK: - Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{game_with_player, ok_body};

    #[test]
    fn quest_from_absent_npc_is_404() {
        let mut gs = game_with_player("alice", "entrance");
        let reply = quest(&mut gs, "alice", "prisoner");
        assert!(matches!(reply, Response::Err { code: 404, .. }));
    }

    /// Full cycle of `free_prisoner`: accept, pending, done, nothing left.
    #[test]
    fn free_prisoner_accept_then_complete() {
        let mut gs = game_with_player("alice", "secret_cell");

        // Accepted, then still active without the key.
        let accepted = ok_body(quest(&mut gs, "alice", "Prisoner"));
        assert!(accepted.contains("\"status\": \"active\""));
        let pending = ok_body(quest(&mut gs, "alice", "prisoner"));
        assert!(pending.contains("\"status\": \"active\""));
        let listed = ok_body(quests(&gs, "alice"));
        assert!(listed.contains("\"progress\": \"0/1\""));

        // With the key: completed, then nothing left.
        gs.players
            .get_mut("alice")
            .unwrap()
            .inventory
            .push("cell_key".to_string());
        let done = ok_body(quest(&mut gs, "alice", "prisoner"));
        assert!(done.contains("\"status\": \"completed\""));
        let again = quest(&mut gs, "alice", "prisoner");
        assert!(matches!(again, Response::Err { code: 406, .. }));
        assert!(ok_body(quests(&gs, "alice")).contains("\"completed\""));
    }

    /// `treasure_quest` needs both conditions: the kill alone is 1/2.
    #[test]
    fn treasure_quest_needs_kill_and_item() {
        let mut gs = game_with_player("alice", "entrance");
        quest(&mut gs, "alice", "merchant");

        gs.players
            .get_mut("alice")
            .unwrap()
            .kills
            .push("monster".to_string());
        let listed = ok_body(quests(&gs, "alice"));
        assert!(listed.contains("\"progress\": \"1/2\""));

        gs.players
            .get_mut("alice")
            .unwrap()
            .inventory
            .push("treasure".to_string());
        let done = ok_body(quest(&mut gs, "alice", "merchant"));
        assert!(done.contains("\"status\": \"completed\""));
    }
}
