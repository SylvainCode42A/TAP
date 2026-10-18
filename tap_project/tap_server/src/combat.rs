// -- Combat : ATTACK and STATUS ----------------------------------------------
//
// One ATTACK is one full round: the player hits, then the NPC hits back if it
// is still alive. No timer, no initiative roll: the turn order is the order
// commands reach the server, which the world lock already serialises.
//
// Owns: NPC lookup in a room, damage, death and respawn, the STATUS report.
// Does NOT own: parsing, sending the reply, broadcasting to the room.

use crate::{GameState, Response};

// -- Tuning ------------------------------------------------------------------

pub const MAX_HP: u32 = 100;
pub const RESPAWN_HP: u32 = 50;
pub const RESPAWN_ROOM: &str = "entrance";

const BASE_DAMAGE: u32 = 5;
const SWORD_BONUS: u32 = 10;

// Used when an NPC of world.yaml has no `attack` stat.
const DEFAULT_NPC_ATTACK: u32 = 5;

// -- NPC lookup --------------------------------------------------------------

// Finds an NPC spawned in `room` whose id ("monster") or display name
// ("Monster") matches `query`, ignoring case and surrounding spaces.
// Returns the NPC id, or None when no NPC of that room matches.
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

// -- ATTACK ------------------------------------------------------------------

// Reply: {"attacker_hp", "target_hp", "damage", "status"}, RFC 5.4.5.
// status is one of:
//   combat    both sides still standing
//   victory   the NPC fell; it rises again at full health
//   defeated  the player fell and respawned at the entrance
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
    // A room may spawn an NPC that world.yaml forgot to define: answer 404
    // instead of panicking, a panic here would poison the world lock.
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

    // NPC health lives in GameState, not in the static world: first hit
    // starts it at full health. saturating_sub stops at 0, never wraps.
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

// -- STATUS ------------------------------------------------------------------

// Reply: {"hp", "max_hp", "status"}, RFC 5.4.6.
// status is "combat" while fighting, "wounded" below max hp, else "healthy".
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

// -- Error replies -----------------------------------------------------------

pub fn not_connected() -> Response {
    Response::Err { code: 404, message: "Player not CONNECTED".to_string() }
}

pub fn npc_not_found() -> Response {
    Response::Err { code: 404, message: "NPC_NOT_FOUND".to_string() }
}

// -- Tests -------------------------------------------------------------------

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
