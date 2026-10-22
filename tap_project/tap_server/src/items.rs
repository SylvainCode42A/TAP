//! Items: the `TAKE` and `DROP` commands, RFC 5.4.1 and 5.4.2.
//!
//! An item is named by its id (`cell_key`) or its display name
//! (`Cell Key`), RFC 8.3, ignoring ASCII case and surrounding spaces.
//! Replies always give the id. Each item exists once: taking moves it from
//! the room to the inventory, dropping moves it back, RFC 8.1.
//!
//! Rules the RFC leaves open, chosen by the team:
//!
//! - an item whose `world.yaml` entry says `obtainable: false` stays in
//!   its room: `403 ITEM_NOT_OBTAINABLE`, not an RFC code.
//!
//! - Owns: item lookup by id or name, the replies of `TAKE` and `DROP`.
//! - Does NOT own: `INVENTORY` and `LOOK`, which only read the lists.

use crate::{GameState, Response};

// MARK: - Lookup

/// Finds the item of `among` that `query` names, by id or display name.
///
/// Returns the item id, or `None` when no item of `among` matches.
pub fn find_item(gs: &GameState, among: &[String], query: &str) -> Option<String> {
    let query = query.trim();
    for id in among {
        if id.eq_ignore_ascii_case(query) {
            return Some(id.clone());
        }
        if let Some(item) = gs.world.world.items.get(id)
            && item.name.eq_ignore_ascii_case(query)
        {
            return Some(id.clone());
        }
    }
    None
}

// MARK: - TAKE

/// Moves the item `query` names from the room of `username` to its
/// inventory, RFC 5.4.1. Reply body: `taken=<item-id>`.
///
/// # Errors
///
/// - `404 Player not CONNECTED`: `username` is not in the game.
/// - `404 ITEM_NOT_FOUND`: no item of the room matches `query`.
/// - `403 ITEM_NOT_OBTAINABLE`: the item is fixed in the room.
pub fn take_item(gs: &mut GameState, username: &str, query: &str) -> Response {
    let Some(room_id) = gs.players.get(username).map(|p| p.current_room.clone()) else {
        return not_connected();
    };
    let Some(room) = gs.world.world.locations.get(&room_id) else {
        return not_connected();
    };
    let Some(id) = find_item(gs, &room.items, query) else {
        return Response::Err { code: 404, message: "ITEM_NOT_FOUND".to_string() };
    };
    if gs.world.world.items.get(&id).is_some_and(|item| !item.obtainable) {
        return Response::Err { code: 403, message: "ITEM_NOT_OBTAINABLE".to_string() };
    }

    if let Some(room) = gs.world.world.locations.get_mut(&room_id) {
        room.items.retain(|item| item != &id);
    }
    if let Some(player) = gs.players.get_mut(username) {
        player.inventory.push(id.clone());
    }
    Response::Ok(format!("taken={id}"))
}

// MARK: - DROP

/// Moves the item `query` names from the inventory of `username` to its
/// room, RFC 5.4.2. Reply body: `dropped=<item-id>`.
///
/// # Errors
///
/// - `404 Player not CONNECTED`: `username` is not in the game.
/// - `404 ITEM_NOT_IN_INVENTORY`: no item of the inventory matches `query`.
pub fn drop_item(gs: &mut GameState, username: &str, query: &str) -> Response {
    let Some(player) = gs.players.get(username) else {
        return not_connected();
    };
    let room_id = player.current_room.clone();
    let Some(id) = find_item(gs, &player.inventory, query) else {
        return Response::Err { code: 404, message: "ITEM_NOT_IN_INVENTORY".to_string() };
    };

    if let Some(player) = gs.players.get_mut(username) {
        player.inventory.retain(|item| item != &id);
    }
    if let Some(room) = gs.world.world.locations.get_mut(&room_id) {
        room.items.push(id.clone());
    }
    Response::Ok(format!("dropped={id}"))
}

// MARK: - Error replies

fn not_connected() -> Response {
    Response::Err { code: 404, message: "Player not CONNECTED".to_string() }
}

// MARK: - Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{game_with_player, ok_body};

    fn code(reply: Response) -> u16 {
        match reply {
            Response::Ok(_) => 0,
            Response::Err { code, .. } => code,
        }
    }

    /// Room holding the item `id` in the real world.yaml.
    fn room_of(gs: &GameState, id: &str) -> String {
        gs.world.world.locations.iter()
            .find(|(_, room)| room.items.iter().any(|item| item == id))
            .map(|(room_id, _)| room_id.clone())
            .unwrap()
    }

    #[test]
    fn take_by_display_name_replies_the_id() {
        let mut gs = game_with_player("alice", "entrance");
        let room = room_of(&gs, "cell_key");
        gs.players.get_mut("alice").unwrap().current_room = room.clone();

        assert_eq!(ok_body(take_item(&mut gs, "alice", " cell KEY ")), "taken=cell_key");
        assert_eq!(gs.players["alice"].inventory, ["cell_key"]);
        assert!(!gs.world.world.locations[&room].items.contains(&"cell_key".to_string()));
        assert_eq!(code(take_item(&mut gs, "alice", "Cell Key")), 404);
    }

    #[test]
    fn fixed_item_stays_in_its_room() {
        let mut gs = game_with_player("alice", "entrance");
        let room = room_of(&gs, "torch");
        gs.players.get_mut("alice").unwrap().current_room = room.clone();

        assert_eq!(code(take_item(&mut gs, "alice", "Torch")), 403);
        assert!(gs.world.world.locations[&room].items.contains(&"torch".to_string()));
        assert!(gs.players["alice"].inventory.is_empty());
    }

    #[test]
    fn drop_by_name_puts_the_item_back_for_others() {
        let mut gs = game_with_player("alice", "corridor");
        gs.players.get_mut("alice").unwrap().inventory.push("sword".to_string());

        assert_eq!(code(drop_item(&mut gs, "alice", "Treasure")), 404);
        assert_eq!(ok_body(drop_item(&mut gs, "alice", "sword")), "dropped=sword");
        assert!(gs.players["alice"].inventory.is_empty());

        let mut bob = crate::Player::new("bob");
        bob.current_room = "corridor".to_string();
        gs.players.insert("bob".to_string(), bob);
        assert_eq!(ok_body(take_item(&mut gs, "bob", "SWORD")), "taken=sword");
    }
}
