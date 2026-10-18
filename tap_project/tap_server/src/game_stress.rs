//! Stress tests of [`combat`] and [`quest`].
//!
//! Two kinds of tests:
//!
//! - edge cases: odd names, unknown players, shared monster health, ...
//! - a random campaign: thousands of random actions by four players, with
//!   every invariant checked after each action.
//!
//! The campaign moves players and edits inventories by hand: it tests
//! `combat.rs` and `quest.rs`, not `MOVE`, `TAKE` and `DROP`.

use std::collections::HashMap;

use crate::combat::{self, MAX_HP, RESPAWN_HP, RESPAWN_ROOM};
use crate::quest::{self, QuestStatus};
use crate::test_support::{game_with_player, ok_body};
use crate::{GameState, Player, Quest, Response, Spawn};

// MARK: - Helpers

/// Adds a fresh player named `username` in `room`.
fn add_player(gs: &mut GameState, username: &str, room: &str) {
    let mut player = Player::new(username);
    player.current_room = room.to_string();
    gs.players.insert(username.to_string(), player);
}

/// Puts `item` in the inventory of `username`, as `TAKE` would.
fn give(gs: &mut GameState, username: &str, item: &str) {
    let player = gs.players.get_mut(username).unwrap();
    player.inventory.push(item.to_string());
}

/// True when `reply` is an error with code `expected`.
fn is_err(reply: Response, expected: u16) -> bool {
    matches!(reply, Response::Err { code, .. } if code == expected)
}

// MARK: - Edge cases

#[test]
fn npc_name_ignores_case_and_surrounding_spaces() {
    let mut gs = game_with_player("alice", "monster_room");
    let reply = combat::attack(&mut gs, "alice", "  MONSTER  ");
    assert!(matches!(reply, Response::Ok(_)));
}

#[test]
fn weird_targets_never_panic() {
    let long = "x".repeat(10_000);
    let targets = [
        "", "   ", "monstre", "mon ster", "\u{1F47E}", "\u{e9}", &long,
    ];

    for target in targets {
        let mut gs = game_with_player("alice", "monster_room");
        assert!(is_err(combat::attack(&mut gs, "alice", target), 404));
        assert!(is_err(quest::quest(&mut gs, "alice", target), 404));
    }
}

#[test]
fn unknown_player_is_404_everywhere() {
    let mut gs = game_with_player("alice", "monster_room");
    assert!(is_err(combat::attack(&mut gs, "ghost", "monster"), 404));
    assert!(is_err(combat::status(&gs, "ghost"), 404));
    assert!(is_err(quest::quest(&mut gs, "ghost", "merchant"), 404));
    assert!(is_err(quest::quests(&gs, "ghost"), 404));
}

#[test]
fn spawned_but_undefined_npc_is_404_not_panic() {
    let mut gs = game_with_player("alice", "corridor");
    let corridor = gs.world.world.locations.get_mut("corridor").unwrap();
    corridor.spawns.push(Spawn { npc_type: "phantom".to_string(), count: 1 });

    assert!(is_err(combat::attack(&mut gs, "alice", "phantom"), 404));
    assert!(is_err(quest::quest(&mut gs, "alice", "phantom"), 406));
}

#[test]
fn monster_hp_is_shared_and_kill_goes_to_finisher() {
    let mut gs = game_with_player("alice", "monster_room");
    add_player(&mut gs, "bob", "monster_room");
    give(&mut gs, "alice", "sword");

    // alice hits for 15 (20 -> 5), bob finishes with 5.
    let hit = ok_body(combat::attack(&mut gs, "alice", "monster"));
    assert!(hit.contains("\"target_hp\": 5"));
    let kill = ok_body(combat::attack(&mut gs, "bob", "monster"));
    assert!(kill.contains("\"status\": \"victory\""));

    let monster = "monster".to_string();
    assert!(gs.players["bob"].kills.contains(&monster));
    assert!(!gs.players["alice"].kills.contains(&monster));
    assert_eq!(gs.npc_hp["monster"], 20);
}

#[test]
fn respawned_player_is_wounded_at_entrance() {
    let mut gs = game_with_player("alice", "monster_room");
    gs.players.get_mut("alice").unwrap().hp = 12;

    let fight = ok_body(combat::attack(&mut gs, "alice", "monster"));
    assert!(fight.contains("\"status\": \"defeated\""));

    let body = ok_body(combat::status(&gs, "alice"));
    assert!(body.contains(&format!("\"hp\": {}", RESPAWN_HP)));
    assert!(body.contains("\"wounded\""));
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
    let chat = Quest {
        name: "Chat".to_string(),
        description: "Say \"hi\"\\now".to_string(),
        objectives: vec![],
        rewards: vec![],
        giver: "merchant".to_string(),
        requires_item: None,
        requires_kill: None,
    };
    gs.world.world.quests.insert("chat".to_string(), chat);

    // "chat" sorts before "treasure_quest", so it comes first.
    let first = ok_body(quest::quest(&mut gs, "alice", "merchant"));
    assert!(first.contains("\"quest_id\": \"chat\""));
    assert!(first.contains("Say \\\"hi\\\"\\\\now"));

    let second = ok_body(quest::quest(&mut gs, "alice", "merchant"));
    assert!(second.contains("\"completed\""));

    let third = ok_body(quest::quest(&mut gs, "alice", "merchant"));
    assert!(third.contains("\"treasure_quest\""));
}

// MARK: - Random campaign

/// Tiny deterministic random generator (xorshift64), no external crate.
///
/// Same seed, same sequence: a failing campaign can be replayed exactly.
struct Rng(u64);

impl Rng {
    /// Next pseudo-random number. The seed must not be 0.
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// Picks one element of `items`, uniformly enough for a test.
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        let index = self.next() % items.len() as u64;
        items[index as usize]
    }
}

/// The four players of every campaign.
const PLAYERS: [&str; 4] = ["alice", "bob", "carol", "dave"];
/// The 8 rooms of `world.yaml`, where a campaign teleports players.
const ROOMS: [&str; 8] = [
    "entrance", "corridor", "guard_room", "library",
    "armory", "monster_room", "treasure", "secret_cell",
];
/// Targets of `ATTACK` and `QUEST`: valid ids and names in several cases,
/// plus an unknown NPC, an empty name and an emoji.
const NAMES: [&str; 9] = [
    "monster", "Monster", " MONSTER ", "merchant", "prisoner",
    "Prisoner", "ghost", "", "\u{1F47E}",
];
/// Items a campaign gives and takes away by hand.
const ITEMS: [&str; 3] = ["sword", "cell_key", "treasure"];

/// Plays `steps` random actions from `seed` and returns every reply in order.
///
/// After each action, checks:
///
/// 1. the reply is a balanced JSON body or an error 404, 405 or 406;
/// 2. every player has 1 to [`MAX_HP`] hp, and fights or has killed only
///    hostile NPCs;
/// 3. every stored NPC hp is 1 to its max: 0 is never stored;
/// 4. a completed quest stays completed forever.
///
/// # Panics
///
/// On the first broken invariant, naming the step that broke it.
fn random_campaign(seed: u64, steps: usize) -> Vec<String> {
    let mut gs = game_with_player("alice", "entrance");
    for name in &PLAYERS[1..] {
        add_player(&mut gs, name, "entrance");
    }

    let mut max_npc_hp: HashMap<String, u32> = HashMap::new();
    for (id, npc) in &gs.world.world.npcs {
        let hp = npc.stats.get("hp").copied().unwrap_or(1);
        max_npc_hp.insert(id.clone(), hp);
    }

    let mut rng = Rng(seed);
    let mut completed: Vec<(String, String)> = Vec::new();
    let mut transcript: Vec<String> = Vec::new();

    for step in 0..steps {

        // 1. One random action by one random player.
        let who = rng.pick(&PLAYERS);
        let reply = match rng.next() % 8 {
            0 | 1 => combat::attack(&mut gs, who, rng.pick(&NAMES)),
            2 => combat::status(&gs, who),
            3 => quest::quest(&mut gs, who, rng.pick(&NAMES)),
            4 => quest::quests(&gs, who),
            5 => {
                let player = gs.players.get_mut(who).unwrap();
                player.current_room = rng.pick(&ROOMS).to_string();
                player.in_combat = None;
                continue;
            }
            6 => {
                give(&mut gs, who, rng.pick(&ITEMS));
                continue;
            }
            _ => {
                let item = rng.pick(&ITEMS);
                let player = gs.players.get_mut(who).unwrap();
                player.inventory.retain(|held| held != item);
                continue;
            }
        };

        // 2. The reply is well formed.
        match &reply {
            Response::Ok(body) => {
                let opens = body.matches(['{', '[']).count();
                let closes = body.matches(['}', ']']).count();
                assert_eq!(opens, closes, "step {step}: unbalanced {body}");
                transcript.push(body.clone());
            }
            Response::Err { code, message } => {
                assert!([404, 405, 406].contains(code),
                    "step {step}: unexpected code {code}");
                transcript.push(format!("{code} {message}"));
            }
        }

        // 3. Players: hp in bounds, fights and kills only hostile NPCs.
        for (name, player) in &gs.players {
            assert!((1..=MAX_HP).contains(&player.hp),
                "step {step}: {name} hp {}", player.hp);
            if let Some(target) = &player.in_combat {
                assert!(gs.world.world.npcs[target].hostile,
                    "step {step}: {name} fights a friendly NPC");
            }
            for kill in &player.kills {
                assert!(gs.world.world.npcs[kill].hostile,
                    "step {step}: {name} killed a friendly NPC");
            }
        }

        // 4. NPC hp in bounds: never 0 stored, never above max.
        for (npc, hp) in &gs.npc_hp {
            assert!((1..=max_npc_hp[npc]).contains(hp),
                "step {step}: {npc} hp {hp}");
        }

        // 5. A completed quest stays completed forever.
        for (name, quest_id) in &completed {
            let status = gs.players[name].quests.get(quest_id);
            assert_eq!(status, Some(&QuestStatus::Completed),
                "step {step}: {name} lost completed {quest_id}");
        }
        for (name, player) in &gs.players {
            for (quest_id, status) in &player.quests {
                let entry = (name.clone(), quest_id.clone());
                if *status == QuestStatus::Completed
                    && !completed.contains(&entry)
                {
                    completed.push(entry);
                }
            }
        }
    }
    transcript
}

/// 20 campaigns of 20 000 actions.
///
/// Also checks the campaign really reached every outcome: a run that never
/// kills anything would prove nothing.
#[test]
fn random_campaign_keeps_invariants() {
    let outcomes = [
        "\"victory\"", "\"defeated\"", "\"completed\"", "405 ", "406 ",
    ];
    let mut seen: HashMap<&str, usize> = HashMap::new();

    for seed in 1..=20 {
        let transcript = random_campaign(seed * 0x9E37_79B9, 20_000);
        for outcome in outcomes {
            for line in &transcript {
                if line.contains(outcome) {
                    *seen.entry(outcome).or_default() += 1;
                }
            }
        }
    }

    println!("{seen:?}");
    for outcome in outcomes {
        assert!(seen.get(outcome).copied().unwrap_or(0) > 0,
            "campaign never reached {outcome}: {seen:?}");
    }
}

#[test]
fn random_campaign_is_deterministic() {
    assert_eq!(random_campaign(42, 5_000), random_campaign(42, 5_000));
}

// MARK: - WHO

/// `WHO` counts every connected player, wherever they stand (RFC 5.2.2).
#[test]
fn who_counts_every_player() {
    use std::sync::{Arc, Mutex};
    use crate::{dispatch, Command};

    let mut gs = game_with_player("alice", "entrance");
    add_player(&mut gs, "bob", "treasure");
    add_player(&mut gs, "carol", "secret_cell");
    let shared = Arc::new(Mutex::new(gs));
    let caller = Some("alice".to_string());

    let reply = dispatch::dispatch(Command::Who, &shared, &caller);
    assert_eq!(ok_body(reply), "players=3");
}

// MARK: - Heavy campaigns

/// 500 campaigns of 20 000 actions: 10 million actions. Too slow for every
/// run: `cargo test --release -- --ignored`.
#[test]
#[ignore]
fn heavy_random_campaign() {
    for seed in 1..=500 {
        random_campaign(seed * 0x9E37_79B9 + 7, 20_000);
    }
}

// MARK: - Group campaign

/// Plays `steps` random group commands by six players from `seed`, and
/// checks after each one:
///
/// 1. errors are only 401, 402, 403 or 404;
/// 2. a player is in one group at most, and appears once in it;
/// 3. a group is never empty, and its leader is one of its members;
/// 4. a member is never also invited to the group it is in.
///
/// Returns how many times each command succeeded.
fn group_campaign(seed: u64, steps: usize) -> HashMap<&'static str, usize> {
    use crate::group;

    let names = ["alice", "bob", "carol", "dave", "erin", "frank"];
    let targets = ["alice", "bob", "carol", "dave", "erin", "frank", "ghost",
        "", " bob "];
    let mut gs = game_with_player("alice", "entrance");
    for name in &names[1..] {
        add_player(&mut gs, name, "entrance");
    }
    let mut rng = Rng(seed);
    let mut successes: HashMap<&'static str, usize> = HashMap::new();

    for step in 0..steps {
        let who = rng.pick(&names);
        let (verb, reply) = match rng.next() % 4 {
            0 => ("create", group::create(&mut gs, who)),
            1 => ("invite", group::invite(&mut gs, who, rng.pick(&targets))),
            2 => ("join", group::join(&mut gs, who, rng.pick(&targets))),
            _ => ("leave", group::leave(&mut gs, who)),
        };
        match reply {
            Response::Ok(_) => *successes.entry(verb).or_default() += 1,
            Response::Err { code, .. } => assert!(
                [401, 402, 403, 404].contains(&code),
                "step {step}: {verb} gave {code}"
            ),
        }

        let mut seen: Vec<&String> = Vec::new();
        for (id, grp) in &gs.groups {
            assert!(!grp.members.is_empty(), "step {step}: {id} is empty");
            assert!(grp.members.contains(&grp.leader),
                "step {step}: leader of {id} is not a member");
            for member in &grp.members {
                assert!(!seen.contains(&member),
                    "step {step}: {member} is in two groups or twice");
                seen.push(member);
                assert!(!grp.invited.contains(member),
                    "step {step}: {member} is both member and invited");
            }
        }
    }
    successes
}

/// 20 campaigns of 20 000 group commands, each command reached.
#[test]
fn group_campaign_keeps_invariants() {
    let mut total: HashMap<&str, usize> = HashMap::new();
    for seed in 1..=20 {
        for (verb, count) in group_campaign(seed * 0x2545_F491, 20_000) {
            *total.entry(verb).or_default() += count;
        }
    }
    println!("{total:?}");
    for verb in ["create", "invite", "join", "leave"] {
        assert!(total.get(verb).copied().unwrap_or(0) > 0,
            "{verb} never succeeded: {total:?}");
    }
}

/// 500 group campaigns of 20 000 commands: 10 million commands.
#[test]
#[ignore]
fn heavy_group_campaign() {
    for seed in 1..=500 {
        group_campaign(seed * 0x9E37_79B9 + 3, 20_000);
    }
}
