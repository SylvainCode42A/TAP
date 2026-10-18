//! Combat: the `ATTACK` and `STATUS` commands.
//!
//! One `ATTACK` is one full round: the player hits, then the NPC hits back
//! if it is still alive. There is no timer and no initiative roll: turns
//! follow the order in which commands reach the server, which the world lock
//! already serialises.
//!
//! - Owns: NPC lookup in a room, damage, death and respawn, the `STATUS`
//!   report.
//! - Does NOT own: parsing, sending the reply, broadcasting to the room.

use crate::{GameState, Response};

// MARK: - Tuning

/// Hit points of a fresh player, and the cap after any heal.
pub const MAX_HP: u32 = 100;

/// Hit points a player gets back after falling to 0.
pub const RESPAWN_HP: u32 = 50;

/// Room where a fallen player respawns: the dungeon entrance, out of reach
/// of the monster.
pub const RESPAWN_ROOM: &str = "entrance";

/// Damage of a bare-handed hit.
const BASE_DAMAGE: u32 = 5;

/// Extra damage while the `sword` item is in the inventory.
const SWORD_BONUS: u32 = 10;

/// Counter-attack damage of an NPC whose `world.yaml` entry has no `attack`
/// stat.
const DEFAULT_NPC_ATTACK: u32 = 5;

// MARK: - NPC lookup

/// Finds the NPC of `room` that `query` names.
///
/// `query` matches an NPC id (`monster`) or its display name (`Monster`),
/// ignoring ASCII case and surrounding spaces. Only NPCs spawned in `room`
/// are candidates: an NPC elsewhere in the world is not found.
///
/// Returns the NPC id, or `None` when no NPC of `room` matches.
pub fn find_npc_in_room(gs: &GameState, room: &str, query: &str)
    -> Option<String>
{
    let location = gs.world.world.locations.get(room)?;
    let query = query.trim();

    for spawn in &location.spawns {
        let id = &spawn.npc_type;
        if id.eq_ignore_ascii_case(query) {
            return Some(id.clone());
        }
        if let Some(npc) = gs.world.world.npcs.get(id)
            && npc.name.eq_ignore_ascii_case(query)
        {
            return Some(id.clone());
        }
    }
    None
}

// MARK: - ATTACK

/// Plays one combat round of `username` against `target`, RFC 5.4.5.
///
/// The player deals [`BASE_DAMAGE`], plus [`SWORD_BONUS`] with the sword.
/// If the NPC survives, it hits back with its `attack` stat.
///
/// Reply body: `{"attacker_hp", "target_hp", "damage", "status"}`, where
/// `status` is one of:
///
/// | status     | meaning                                                   |
/// |------------|-----------------------------------------------------------|
/// | `combat`   | both sides still standing                                 |
/// | `victory`  | the NPC fell; it rises again at full health               |
/// | `defeated` | the player fell and respawned in [`RESPAWN_ROOM`]         |
///
/// # Errors
///
/// - `404 Player not CONNECTED`: `username` is not in the game.
/// - `404 NPC_NOT_FOUND`: no NPC of the player's room matches `target`.
/// - `405 NPC_NOT_HOSTILE`: the NPC exists but is not an enemy.
pub fn attack(gs: &mut GameState, username: &str, target: &str) -> Response {

    // 1. Where is the player?
    let room = match gs.players.get(username) {
        None => return not_connected(),
        Some(player) => player.current_room.clone(),
    };

    // 2. Is the target here, does it exist, is it hostile?
    let npc_id = match find_npc_in_room(gs, &room, target) {
        None => return npc_not_found(),
        Some(id) => id,
    };
    // INVARIANT: never panic here. A room may spawn an NPC that world.yaml
    // forgot to define, and a panic would poison the world lock for every
    // client. Answer 404 instead.
    let Some(npc) = gs.world.world.npcs.get(&npc_id) else {
        return npc_not_found();
    };
    if !npc.hostile {
        return Response::Err {
            code: 405,
            message: "NPC_NOT_HOSTILE".to_string(),
        };
    }
    let npc_max_hp = npc.stats.get("hp").copied().unwrap_or(1);
    let npc_attack = npc.stats.get("attack").copied()
        .unwrap_or(DEFAULT_NPC_ATTACK);

    // 3. The player hits. The sword adds its bonus.
    let Some(player) = gs.players.get_mut(username) else {
        return not_connected();
    };
    let mut damage = BASE_DAMAGE;
    if player.inventory.contains(&"sword".to_string()) {
        damage += SWORD_BONUS;
    }

    // NPC health lives in GameState, not in the static world: the first hit
    // starts it at full health. saturating_sub stops at 0, never wraps
    // around to 4 billion as a plain u32 subtraction would.
    let npc_hp = gs.npc_hp.entry(npc_id.clone()).or_insert(npc_max_hp);
    *npc_hp = npc_hp.saturating_sub(damage);
    let target_hp = *npc_hp;

    // 4. Outcome of the round.
    let status;
    if target_hp == 0 {
        // The NPC rises again at once, so every player can finish the
        // treasure quest, not only the first one to kill it.
        *npc_hp = npc_max_hp;
        player.in_combat = None;
        if !player.kills.contains(&npc_id) {
            player.kills.push(npc_id);
        }
        status = "victory";
    } else {
        // Still standing: the NPC hits back.
        player.hp = player.hp.saturating_sub(npc_attack);
        if player.hp == 0 {
            player.hp = RESPAWN_HP;
            player.current_room = RESPAWN_ROOM.to_string();
            player.in_combat = None;
            status = "defeated";
        } else {
            player.in_combat = Some(npc_id);
            status = "combat";
        }
    }

    Response::Ok(format!(
        "{{\"attacker_hp\": {}, \"target_hp\": {}, \
         \"damage\": {}, \"status\": \"{}\"}}",
        player.hp, target_hp, damage, status
    ))
}

// MARK: - STATUS

/// Reports the health of `username`, RFC 5.4.6.
///
/// Reply body: `{"hp", "max_hp", "status"}`, where `status` is `combat`
/// while fighting, `wounded` below [`MAX_HP`], else `healthy`.
///
/// # Errors
///
/// - `404 Player not CONNECTED`: `username` is not in the game.
pub fn status(gs: &GameState, username: &str) -> Response {
    let Some(player) = gs.players.get(username) else {
        return not_connected();
    };

    let status = if player.in_combat.is_some() {
        "combat"
    } else if player.hp < MAX_HP {
        "wounded"
    } else {
        "healthy"
    };

    Response::Ok(format!(
        "{{\"hp\": {}, \"max_hp\": {}, \"status\": \"{}\"}}",
        player.hp, MAX_HP, status
    ))
}

// MARK: - Error replies

/// `404 Player not CONNECTED`, same wording as `dispatch.rs`.
pub fn not_connected() -> Response {
    Response::Err { code: 404, message: "Player not CONNECTED".to_string() }
}

/// `404 NPC_NOT_FOUND`, the RFC 8.2 message for an NPC absent from the room.
pub fn npc_not_found() -> Response {
    Response::Err { code: 404, message: "NPC_NOT_FOUND".to_string() }
}

// MARK: - Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{game_with_player, ok_body};

    #[test]
    fn attack_absent_npc_is_404() {
        let mut gs = game_with_player("alice", "entrance");
        let reply = attack(&mut gs, "alice", "monster");
        assert!(matches!(reply, Response::Err { code: 404, .. }));
    }

    #[test]
    fn attack_friendly_npc_is_405() {
        let mut gs = game_with_player("alice", "entrance");
        let reply = attack(&mut gs, "alice", "Merchant");
        assert!(matches!(reply, Response::Err { code: 405, .. }));
    }

    /// Monster has 20 hp, the sword hits for 15: 20 -> 5 -> dead.
    #[test]
    fn sword_kills_monster_in_two_rounds() {
        let mut gs = game_with_player("alice", "monster_room");
        gs.players.get_mut("alice").unwrap()
            .inventory.push("sword".to_string());

        let first = ok_body(attack(&mut gs, "alice", "monster"));
        assert!(first.contains("\"target_hp\": 5"));
        assert!(first.contains("\"status\": \"combat\""));

        let second = ok_body(attack(&mut gs, "alice", "monster"));
        assert!(second.contains("\"status\": \"victory\""));
        assert!(gs.players["alice"].kills.contains(&"monster".to_string()));
    }

    #[test]
    fn player_at_zero_hp_respawns() {
        let mut gs = game_with_player("alice", "monster_room");
        gs.players.get_mut("alice").unwrap().hp = 1;

        let body = ok_body(attack(&mut gs, "alice", "monster"));
        assert!(body.contains("\"status\": \"defeated\""));
        assert_eq!(gs.players["alice"].hp, RESPAWN_HP);
        assert_eq!(gs.players["alice"].current_room, RESPAWN_ROOM);
    }

    #[test]
    fn status_reports_combat_after_hit() {
        let mut gs = game_with_player("alice", "monster_room");
        attack(&mut gs, "alice", "monster");
        let body = ok_body(status(&gs, "alice"));
        assert!(body.contains("\"status\": \"combat\""));
    }
}
