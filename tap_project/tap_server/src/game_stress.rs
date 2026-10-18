// Edge cases and randomized invariant checks for combat and quests.

use std::collections::HashMap;
use crate::combat::{self, MAX_HP, RESPAWN_HP, RESPAWN_ROOM};
use crate::quest::{self, QuestStatus};
use crate::test_support::{game_with_player, ok_body};
use crate::{GameState, Player, Quest, Response, Spawn};

fn add_player(gs: &mut GameState, username: &str, room: &str) {
    let mut player = Player::new(username);
    player.current_room = room.to_string();
    gs.players.insert(username.to_string(), player);
}

fn give(gs: &mut GameState, username: &str, item: &str) {
    gs.players.get_mut(username).unwrap().inventory.push(item.to_string());
}

#[test]
fn npc_name_ignores_case_and_surrounding_spaces() {
    let mut gs = game_with_player("alice", "monster_room");
    assert!(matches!(combat::attack(&mut gs, "alice", "  MONSTER  "), Response::Ok(_)));
}

#[test]
fn weird_targets_never_panic() {
    let long = "x".repeat(10_000);
    for target in ["", "   ", "monstre", "mon ster", "\u{1F47E}", "é", long.as_str()] {
        let mut gs = game_with_player("alice", "monster_room");
        assert!(matches!(combat::attack(&mut gs, "alice", target), Response::Err { code: 404, .. }));
        assert!(matches!(quest::quest(&mut gs, "alice", target), Response::Err { code: 404, .. }));
    }
}

#[test]
fn unknown_player_is_404_everywhere() {
    let mut gs = game_with_player("alice", "monster_room");
    assert!(matches!(combat::attack(&mut gs, "ghost", "monster"), Response::Err { code: 404, .. }));
    assert!(matches!(combat::status(&gs, "ghost"), Response::Err { code: 404, .. }));
    assert!(matches!(quest::quest(&mut gs, "ghost", "merchant"), Response::Err { code: 404, .. }));
    assert!(matches!(quest::quests(&gs, "ghost"), Response::Err { code: 404, .. }));
}

#[test]
fn spawned_but_undefined_npc_is_404_not_panic() {
    let mut gs = game_with_player("alice", "corridor");
    gs.world.world.locations.get_mut("corridor").unwrap()
        .spawns.push(Spawn { npc_type: "phantom".to_string(), count: 1 });
    assert!(matches!(combat::attack(&mut gs, "alice", "phantom"), Response::Err { code: 404, .. }));
    assert!(matches!(quest::quest(&mut gs, "alice", "phantom"), Response::Err { code: 406, .. }));
}

#[test]
fn monster_hp_is_shared_and_kill_goes_to_finisher() {
    let mut gs = game_with_player("alice", "monster_room");
    add_player(&mut gs, "bob", "monster_room");
    give(&mut gs, "alice", "sword");
    assert!(ok_body(combat::attack(&mut gs, "alice", "monster")).contains("\"target_hp\": 5"));
    assert!(ok_body(combat::attack(&mut gs, "bob", "monster")).contains("\"status\": \"victory\""));
    assert!(gs.players["bob"].kills.contains(&"monster".to_string()));
    assert!(!gs.players["alice"].kills.contains(&"monster".to_string()));
    assert_eq!(gs.npc_hp["monster"], 20);
}

#[test]
fn respawned_player_is_wounded_at_entrance() {
    let mut gs = game_with_player("alice", "monster_room");
    gs.players.get_mut("alice").unwrap().hp = 12;
    assert!(ok_body(combat::attack(&mut gs, "alice", "monster")).contains("\"status\": \"defeated\""));
    let body = ok_body(combat::status(&gs, "alice"));
    assert!(body.contains(&format!("\"hp\": {}", RESPAWN_HP)) && body.contains("\"wounded\""));
    assert_eq!(gs.players["alice"].current_room, RESPAWN_ROOM);
}

#[test]
fn quests_is_empty_array_before_any_quest() {
    let gs = game_with_player("alice", "entrance");
    assert_eq!(ok_body(quest::quests(&gs, "alice")), "[]");
}

#[test]
fn quest_without_conditions_completes_on_second_call() {
    let mut gs = game_with_player("alice", "entrance");
    gs.world.world.quests.insert("chat".to_string(), Quest {
        name: "Chat".to_string(),
        description: "Say \"hi\"\\now".to_string(),
        objectives: vec![],
        rewards: vec![],
        giver: "merchant".to_string(),
        requires_item: None,
        requires_kill: None,
    });
    let first = ok_body(quest::quest(&mut gs, "alice", "merchant"));
    assert!(first.contains("\"quest_id\": \"chat\"") && first.contains("Say \\\"hi\\\"\\\\now"));
    assert!(ok_body(quest::quest(&mut gs, "alice", "merchant")).contains("\"completed\""));
    assert!(ok_body(quest::quest(&mut gs, "alice", "merchant")).contains("\"treasure_quest\""));
}

// xorshift64: deterministic, no external crate.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[(self.next() % items.len() as u64) as usize]
    }
}

// Runs `steps` random actions from `seed`, checks invariants after each, returns the transcript.
fn random_campaign(seed: u64, steps: usize) -> Vec<String> {
    let players = ["alice", "bob", "carol", "dave"];
    let rooms = ["entrance", "corridor", "guard_room", "library", "armory", "monster_room", "treasure", "secret_cell"];
    let names = ["monster", "Monster", " MONSTER ", "merchant", "prisoner", "Prisoner", "ghost", "", "\u{1F47E}"];
    let items = ["sword", "cell_key", "treasure"];

    let mut gs = game_with_player("alice", "entrance");
    for name in &players[1..] {
        add_player(&mut gs, name, "entrance");
    }
    let max_npc_hp: HashMap<String, u32> = gs.world.world.npcs.iter()
        .map(|(id, npc)| (id.clone(), npc.stats.get("hp").copied().unwrap_or(1)))
        .collect();
    let mut rng = Rng(seed);
    let mut completed: Vec<(String, String)> = vec![];
    let mut transcript = vec![];

    for step in 0..steps {
        let who = rng.pick(&players);
        let response = match rng.next() % 8 {
            0 | 1 => combat::attack(&mut gs, who, rng.pick(&names)),
            2 => combat::status(&gs, who),
            3 => quest::quest(&mut gs, who, rng.pick(&names)),
            4 => quest::quests(&gs, who),
            5 => {
                let player = gs.players.get_mut(who).unwrap();
                player.current_room = rng.pick(&rooms).to_string();
                player.in_combat = None;
                continue;
            }
            6 => {
                give(&mut gs, who, rng.pick(&items));
                continue;
            }
            _ => {
                let item = rng.pick(&items);
                gs.players.get_mut(who).unwrap().inventory.retain(|held| held != item);
                continue;
            }
        };

        match &response {
            Response::Ok(body) => {
                let opens = body.matches(['{', '[']).count();
                let closes = body.matches(['}', ']']).count();
                assert_eq!(opens, closes, "step {step}: unbalanced JSON {body}");
                transcript.push(body.clone());
            }
            Response::Err { code, message } => {
                assert!([404, 405, 406].contains(code), "step {step}: unexpected code {code}");
                transcript.push(format!("{code} {message}"));
            }
        }

        for (name, player) in &gs.players {
            assert!((1..=MAX_HP).contains(&player.hp), "step {step}: {name} hp {}", player.hp);
            if let Some(target) = &player.in_combat {
                assert!(gs.world.world.npcs[target].hostile, "step {step}: {name} fights a friendly");
            }
            for kill in &player.kills {
                assert!(gs.world.world.npcs[kill].hostile, "step {step}: {name} killed a friendly");
            }
        }
        for (npc, hp) in &gs.npc_hp {
            assert!((1..=max_npc_hp[npc]).contains(hp), "step {step}: {npc} hp {hp}");
        }
        for (name, quest_id) in &completed {
            assert_eq!(gs.players[name].quests.get(quest_id), Some(&QuestStatus::Completed),
                "step {step}: {name} lost completed {quest_id}");
        }
        for (name, player) in &gs.players {
            for (quest_id, status) in &player.quests {
                let entry = (name.clone(), quest_id.clone());
                if *status == QuestStatus::Completed && !completed.contains(&entry) {
                    completed.push(entry);
                }
            }
        }
    }
    transcript
}

#[test]
fn random_campaign_keeps_invariants() {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for seed in 1..=20 {
        let transcript = random_campaign(seed * 0x9E37_79B9, 20_000);
        for outcome in ["\"victory\"", "\"defeated\"", "\"completed\"", "405 ", "406 "] {
            *seen.entry(outcome).or_default() += transcript.iter().filter(|line| line.contains(outcome)).count();
        }
    }
    println!("{seen:?}");
    assert!(seen.values().all(|&count| count > 0), "campaign never reached some outcome: {seen:?}");
}

#[test]
fn random_campaign_is_deterministic() {
    assert_eq!(random_campaign(42, 5_000), random_campaign(42, 5_000));
}
