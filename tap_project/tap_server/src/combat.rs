use crate::{GameState, Response};

pub const MAX_HP: u32 = 100;
pub const RESPAWN_HP: u32 = 50;
pub const RESPAWN_ROOM: &str = "entrance";
const BASE_DAMAGE: u32 = 5;
const SWORD_BONUS: u32 = 10;
const DEFAULT_NPC_ATTACK: u32 = 5;

// NPC spawned in `room` whose id or display name matches `query`, case-insensitive.
pub fn find_npc_in_room(gs: &GameState, room: &str, query: &str) -> Option<String> {
    let location = gs.world.world.locations.get(room)?;
    let query = query.trim();
    location.spawns.iter()
        .map(|spawn| &spawn.npc_type)
        .find(|id| {
            id.eq_ignore_ascii_case(query)
                || gs.world.world.npcs.get(*id).is_some_and(|npc| npc.name.eq_ignore_ascii_case(query))
        })
        .cloned()
}

// One round: the player hits, then the NPC hits back if still alive.
pub fn attack(gs: &mut GameState, username: &str, target: &str) -> Response {
    let room = match gs.players.get(username) {
        None => return Response::Err { code: 404, message: "Player not CONNECTED".to_string() },
        Some(player) => player.current_room.clone(),
    };
    let npc_id = match find_npc_in_room(gs, &room, target) {
        None => return Response::Err { code: 404, message: "NPC_NOT_FOUND".to_string() },
        Some(id) => id,
    };
    let Some(npc) = gs.world.world.npcs.get(&npc_id) else {
        return Response::Err { code: 404, message: "NPC_NOT_FOUND".to_string() };
    };
    if !npc.hostile {
        return Response::Err { code: 405, message: "NPC_NOT_HOSTILE".to_string() };
    }
    let npc_max_hp = npc.stats.get("hp").copied().unwrap_or(1);
    let npc_attack = npc.stats.get("attack").copied().unwrap_or(DEFAULT_NPC_ATTACK);

    let Some(player) = gs.players.get_mut(username) else {
        return Response::Err { code: 404, message: "Player not CONNECTED".to_string() };
    };
    let damage = if player.inventory.iter().any(|item| item == "sword") {
        BASE_DAMAGE + SWORD_BONUS
    } else {
        BASE_DAMAGE
    };
    let npc_hp = gs.npc_hp.entry(npc_id.clone()).or_insert(npc_max_hp);
    *npc_hp = npc_hp.saturating_sub(damage);
    let target_hp = *npc_hp;

    let status = if target_hp == 0 {
        // The NPC rises again at full health, so every player can defeat it.
        *npc_hp = npc_max_hp;
        player.in_combat = None;
        if !player.kills.contains(&npc_id) {
            player.kills.push(npc_id);
        }
        "victory"
    } else {
        player.hp = player.hp.saturating_sub(npc_attack);
        if player.hp == 0 {
            player.hp = RESPAWN_HP;
            player.current_room = RESPAWN_ROOM.to_string();
            player.in_combat = None;
            "defeated"
        } else {
            player.in_combat = Some(npc_id);
            "combat"
        }
    };

    Response::Ok(format!(
        "{{\"attacker_hp\": {}, \"target_hp\": {}, \"damage\": {}, \"status\": \"{}\"}}",
        player.hp, target_hp, damage, status
    ))
}

pub fn status(gs: &GameState, username: &str) -> Response {
    match gs.players.get(username) {
        None => Response::Err { code: 404, message: "Player not CONNECTED".to_string() },
        Some(player) => {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{game_with_player, ok_body};

    #[test]
    fn attack_absent_npc_is_404() {
        let mut gs = game_with_player("alice", "entrance");
        assert!(matches!(attack(&mut gs, "alice", "monster"), Response::Err { code: 404, .. }));
    }

    #[test]
    fn attack_friendly_npc_is_405() {
        let mut gs = game_with_player("alice", "entrance");
        assert!(matches!(attack(&mut gs, "alice", "Merchant"), Response::Err { code: 405, .. }));
    }

    #[test]
    fn sword_kills_monster_in_two_rounds() {
        let mut gs = game_with_player("alice", "monster_room");
        gs.players.get_mut("alice").unwrap().inventory.push("sword".to_string());
        let first = ok_body(attack(&mut gs, "alice", "monster"));
        assert!(first.contains("\"target_hp\": 5") && first.contains("\"status\": \"combat\""));
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
    fn status_reports_wounded_after_hit() {
        let mut gs = game_with_player("alice", "monster_room");
        attack(&mut gs, "alice", "monster");
        assert!(ok_body(status(&gs, "alice")).contains("\"status\": \"combat\""));
    }
}
