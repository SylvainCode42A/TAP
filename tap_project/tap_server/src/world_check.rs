//! World validation: refuse to start on a broken `world.yaml`.
//!
//! The subject asks the server to check that every exit and every reference
//! of the world is correct. A broken world would otherwise surface
//! later, in the middle of a game, as a player stuck in a missing room.
//!
//! Every problem is collected, never only the first, so one run lists all
//! of them.
//!
//! - Owns: reference checks, map shape checks, subject minimums.
//! - Does NOT own: reading or parsing the file (`World::load_world`),
//!   stopping the server (`main`).

use std::collections::{HashMap, HashSet, VecDeque};

use crate::World;
use crate::combat::RESPAWN_ROOM;

// MARK: - Subject minimums

/// At least 8 interconnected rooms.
const MIN_ROOMS: usize = 8;

/// At least 4 distinct items.
const MIN_ITEMS: usize = 4;

/// At least 2 items a player can pick up in a room.
const MIN_OBTAINABLE: usize = 2;

/// At least 3 NPCs, for the 3 roles: dialogue, quest giver, enemy.
const MIN_NPCS: usize = 3;

/// At least 2 quests.
const MIN_QUESTS: usize = 2;

// MARK: - Entry point

/// Checks the whole world and returns every problem found, sorted.
///
/// An empty list means the world is valid. Each entry is one readable line,
/// such as `exit corridor.north -> attic: no such room`.
pub fn validate(world: &World) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();

    check_names(world, &mut errors);
    check_rooms(world, &mut errors);
    check_npcs_and_quests(world, &mut errors);
    check_map_shape(world, &mut errors);
    check_minimums(world, &mut errors);

    errors.sort();
    errors
}

// MARK: - Names

/// Every display name is non-empty and free of control characters, which
/// would break the one-line replies of the protocol.
fn check_names(world: &World, errors: &mut Vec<String>) {
    let data = &world.world;
    let mut named: Vec<(&str, &String, &String)> = Vec::new();
    for (id, room) in &data.locations {
        named.push(("room", id, &room.name));
    }
    for (id, item) in &data.items {
        named.push(("item", id, &item.name));
    }
    for (id, npc) in &data.npcs {
        named.push(("npc", id, &npc.name));
    }

    for (kind, id, name) in named {
        if name.trim().is_empty() {
            errors.push(format!("{kind} {id}: empty name"));
        } else if name.chars().any(char::is_control) {
            errors.push(format!("{kind} {id}: control character in name"));
        }
    }
}

// MARK: - Rooms

/// Exits, items and NPC spawns of every room point to things that exist,
/// and every exit has a way back, so the map stays coherent.
fn check_rooms(world: &World, errors: &mut Vec<String>) {
    let data = &world.world;

    if !data.locations.contains_key(RESPAWN_ROOM) {
        errors.push(format!("start room {RESPAWN_ROOM}: no such room"));
    }

    for (room_id, room) in &data.locations {
        for (direction, target) in &room.exits {
            let Some(target_room) = data.locations.get(target) else {
                errors.push(format!(
                    "exit {room_id}.{direction} -> {target}: no such room"
                ));
                continue;
            };
            let mut leads_back = false;
            for back in target_room.exits.values() {
                if back == room_id {
                    leads_back = true;
                }
            }
            if !leads_back {
                errors.push(format!(
                    "exit {room_id}.{direction} -> {target}: \
                     no exit back to {room_id}"
                ));
            }
        }
        for item in &room.items {
            if !data.items.contains_key(item) {
                errors.push(format!("room {room_id}: unknown item {item}"));
            }
        }
        for spawn in &room.spawns {
            if !data.npcs.contains_key(&spawn.npc_type) {
                errors.push(format!("room {room_id}: unknown npc {}", spawn.npc_type));
            }
        }
    }
}

// MARK: - NPCs and quests

/// Every quest names a giver that exists and stands in some room, an item
/// that exists, and an enemy to beat that is really hostile. Otherwise the
/// quest could never be taken or never be finished.
fn check_npcs_and_quests(world: &World, errors: &mut Vec<String>) {
    let data = &world.world;

    let mut spawned: HashSet<&String> = HashSet::new();
    for room in data.locations.values() {
        for spawn in &room.spawns {
            spawned.insert(&spawn.npc_type);
        }
    }

    for (quest_id, quest) in &data.quests {
        if !data.npcs.contains_key(&quest.giver) {
            errors.push(format!("quest {quest_id}: unknown giver '{}'", quest.giver));
        } else if !spawned.contains(&quest.giver) {
            errors.push(format!(
                "quest {quest_id}: giver {} is in no room",
                quest.giver
            ));
        }
        if let Some(item) = &quest.requires_item
            && !data.items.contains_key(item)
        {
            errors.push(format!("quest {quest_id}: unknown item {item}"));
        }
        if let Some(npc_id) = &quest.requires_kill {
            match data.npcs.get(npc_id) {
                None => errors.push(format!("quest {quest_id}: unknown enemy {npc_id}")),
                Some(npc) if !npc.hostile => errors.push(format!(
                    "quest {quest_id}: {npc_id} is not hostile, \
                     it can never be beaten"
                )),
                Some(_) => {}
            }
        }
    }
}

// MARK: - Map shape

/// The map is one piece, has a loop and has an optional branch.
///
/// Two readings of the exits, on purpose:
///
/// - one piece: exits are one-way, as a player walks them with `MOVE`.
///   Every room must be reachable from [`RESPAWN_ROOM`].
/// - loop and branch: exits are two-way links, the shape of the map.
///   A group of linked rooms holds a loop when it has as many links as
///   rooms, or more: a group of N rooms without a loop has exactly N - 1.
///   A branch is a dead end, a room linked to a single other room.
///
/// Counting per group matters: on a map in pieces, a triangle next to a
/// lone corridor has 5 rooms and 4 links in total, yet holds a loop.
fn check_map_shape(world: &World, errors: &mut Vec<String>) {
    let rooms = &world.world.locations;

    // One-way exits and two-way links, broken exits and self exits ignored
    // (the broken ones are reported by check_rooms).
    let mut exits_to: HashMap<&String, HashSet<&String>> = HashMap::new();
    let mut linked: HashMap<&String, HashSet<&String>> = HashMap::new();
    for room_id in rooms.keys() {
        exits_to.insert(room_id, HashSet::new());
        linked.insert(room_id, HashSet::new());
    }
    for (room_id, room) in rooms {
        for target in room.exits.values() {
            let Some((target_id, _)) = rooms.get_key_value(target) else {
                continue;
            };
            if target_id == room_id {
                continue;
            }
            exits_to.get_mut(room_id).unwrap().insert(target_id);
            linked.get_mut(room_id).unwrap().insert(target_id);
            linked.get_mut(target_id).unwrap().insert(room_id);
        }
    }

    // One piece: walk the one-way exits from the start room.
    if let Some((start_id, _)) = rooms.get_key_value(RESPAWN_ROOM) {
        let seen = walk(&exits_to, start_id);
        for room_id in rooms.keys() {
            if !seen.contains(room_id) {
                errors.push(format!("room {room_id}: unreachable from {RESPAWN_ROOM}"));
            }
        }
    }

    // Loop: split the map into groups of linked rooms, then count rooms
    // and links inside each group.
    let mut grouped: HashSet<&String> = HashSet::new();
    let mut has_loop = false;
    let mut total_links = 0;
    for room_id in rooms.keys() {
        if grouped.contains(room_id) {
            continue;
        }
        let group = walk(&linked, room_id);
        let mut ends = 0;
        for member in &group {
            ends += linked[member].len();
        }
        let links = ends / 2;
        total_links += links;
        if links >= group.len() {
            has_loop = true;
        }
        grouped.extend(group);
    }
    if !rooms.is_empty() && !has_loop {
        errors.push(format!(
            "map: no loop ({} rooms, {} links, no group of rooms has as \
             many links as rooms)",
            rooms.len(),
            total_links
        ));
    }

    // Branch: a dead end.
    let mut dead_ends = 0;
    for others in linked.values() {
        if others.len() == 1 {
            dead_ends += 1;
        }
    }
    if dead_ends == 0 {
        errors.push("map: no optional branch (no dead-end room)".to_string());
    }
}

/// Every room reachable from `start` along `edges`, `start` included.
fn walk<'a>(
    edges: &HashMap<&'a String, HashSet<&'a String>>,
    start: &'a String,
) -> HashSet<&'a String> {
    let mut seen: HashSet<&String> = HashSet::new();
    let mut queue: VecDeque<&String> = VecDeque::new();
    seen.insert(start);
    queue.push_back(start);
    while let Some(room_id) = queue.pop_front() {
        for next in &edges[room_id] {
            if seen.insert(next) {
                queue.push_back(next);
            }
        }
    }
    seen
}

// MARK: - Minimums

/// The counts the subject sets as a floor.
fn check_minimums(world: &World, errors: &mut Vec<String>) {
    let data = &world.world;

    let mut obtainable_in_rooms: HashSet<&String> = HashSet::new();
    for room in data.locations.values() {
        for item in &room.items {
            if data.items.get(item).is_some_and(|it| it.obtainable) {
                obtainable_in_rooms.insert(item);
            }
        }
    }
    let mut hostile = 0;
    for npc in data.npcs.values() {
        if npc.hostile {
            hostile += 1;
        }
    }

    let counts = [
        ("rooms", data.locations.len(), MIN_ROOMS),
        ("items", data.items.len(), MIN_ITEMS),
        (
            "obtainable items",
            obtainable_in_rooms.len(),
            MIN_OBTAINABLE,
        ),
        ("npcs", data.npcs.len(), MIN_NPCS),
        ("hostile npcs", hostile, 1),
        ("quests", data.quests.len(), MIN_QUESTS),
    ];
    for (what, found, needed) in counts {
        if found < needed {
            errors.push(format!(
                "minimum: {found} {what}, the subject asks for {needed}"
            ));
        }
    }
}

// MARK: - Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Spawn, World};

    fn world() -> World {
        World::load_world()
    }

    /// True when exactly one error mentions `fragment`.
    fn reports(world: &World, fragment: &str) -> bool {
        let errors = validate(world);
        errors.iter().filter(|e| e.contains(fragment)).count() == 1
    }

    #[test]
    fn shipped_world_is_valid() {
        assert_eq!(validate(&world()), Vec::<String>::new());
    }

    #[test]
    fn exit_to_missing_room() {
        let mut w = world();
        let corridor = w.world.locations.get_mut("corridor").unwrap();
        corridor.exits.insert("up".to_string(), "attic".to_string());
        assert!(reports(&w, "exit corridor.up -> attic: no such room"));
    }

    #[test]
    fn exit_without_way_back() {
        let mut w = world();
        let cell = w.world.locations.get_mut("secret_cell").unwrap();
        cell.exits
            .insert("east".to_string(), "treasure".to_string());
        assert!(reports(&w, "secret_cell.east -> treasure: no exit back"));
    }

    #[test]
    fn unknown_item_and_npc_in_room() {
        let mut w = world();
        let library = w.world.locations.get_mut("library").unwrap();
        library.items.push("grail".to_string());
        library.spawns.push(Spawn {
            npc_type: "ghost".to_string(),
            count: 1,
        });
        assert!(reports(&w, "room library: unknown item grail"));
        assert!(reports(&w, "room library: unknown npc ghost"));
    }

    #[test]
    fn quest_with_bad_references() {
        let mut w = world();
        let quest = w.world.quests.get_mut("treasure_quest").unwrap();
        quest.giver = "king".to_string();
        quest.requires_item = Some("crown".to_string());
        quest.requires_kill = Some("merchant".to_string());
        assert!(reports(&w, "unknown giver 'king'"));
        assert!(reports(&w, "unknown item crown"));
        assert!(reports(&w, "merchant is not hostile"));
    }

    #[test]
    fn giver_standing_in_no_room() {
        let mut w = world();
        let cell = w.world.locations.get_mut("secret_cell").unwrap();
        cell.spawns.clear();
        assert!(reports(&w, "giver prisoner is in no room"));
    }

    /// Cutting the treasure room off also removes the only way back to it.
    #[test]
    fn unreachable_room() {
        let mut w = world();
        let lair = w.world.locations.get_mut("monster_room").unwrap();
        lair.exits.remove("east");
        assert!(reports(&w, "room treasure: unreachable"));
    }

    /// Removing one side of the loop leaves a tree: 8 rooms, 7 links.
    #[test]
    fn map_without_loop() {
        let mut w = world();
        let lair = w.world.locations.get_mut("monster_room").unwrap();
        lair.exits.remove("south_alt");
        let armory = w.world.locations.get_mut("armory").unwrap();
        armory.exits.remove("north");
        assert!(reports(&w, "map: no loop (8 rooms, 7 links"));
    }

    /// Linking the two dead ends (secret cell, treasure) closes every
    /// branch: all rooms then sit on a loop.
    #[test]
    fn map_without_branch() {
        let mut w = world();
        let cell = w.world.locations.get_mut("secret_cell").unwrap();
        cell.exits
            .insert("east".to_string(), "treasure".to_string());
        let treasure = w.world.locations.get_mut("treasure").unwrap();
        treasure
            .exits
            .insert("south".to_string(), "secret_cell".to_string());
        assert!(reports(&w, "map: no optional branch"));
    }

    /// A room with one exit out but two rooms around it is no dead end:
    /// the treasure room only exits west, yet the secret cell leads in.
    #[test]
    fn one_way_exit_is_not_a_dead_end() {
        let mut w = world();
        let cell = w.world.locations.get_mut("secret_cell").unwrap();
        cell.exits
            .insert("east".to_string(), "treasure".to_string());
        assert!(reports(&w, "map: no optional branch"));
    }

    #[test]
    fn too_few_quests() {
        let mut w = world();
        w.world.quests.remove("free_prisoner");
        assert!(reports(&w, "minimum: 1 quests, the subject asks for 2"));
    }

    #[test]
    fn empty_and_control_names() {
        let mut w = world();
        w.world.items.get_mut("sword").unwrap().name = "  ".to_string();
        let monster = w.world.npcs.get_mut("monster").unwrap();
        monster.name = "Mon\nster".to_string();
        assert!(reports(&w, "item sword: empty name"));
        assert!(reports(&w, "npc monster: control character in name"));
    }

    #[test]
    fn random_breakage_never_panics() {
        random_breakage(0x2545_F491_4F6C_DD1D, 2_000);
    }

    /// 50 000 random breakages. Too slow for every run:
    /// `cargo test --release -- --ignored`.
    #[test]
    #[ignore]
    fn heavy_random_breakage() {
        for seed in 1..=25 {
            random_breakage(seed * 0x9E37_79B9, 2_000);
        }
    }

    /// Breaks the world at random `rounds` times from `seed`: validation
    /// must never panic, and every break must be reported.
    fn random_breakage(seed: u64, rounds: usize) {
        let rooms = [
            "entrance",
            "corridor",
            "guard_room",
            "library",
            "armory",
            "monster_room",
            "treasure",
            "secret_cell",
        ];
        let mut seed = seed;
        for _ in 0..rounds {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let mut w = world();
            let room = rooms[(seed % 8) as usize];
            match (seed >> 8) % 4 {
                0 => {
                    w.world.locations.remove(room);
                }
                1 => {
                    let target = w.world.locations.get_mut(room).unwrap();
                    let first = target.exits.keys().min().cloned().unwrap();
                    target.exits.insert(first, "nowhere".to_string());
                    let errors = validate(&w);
                    assert!(errors.iter().any(|e| e.contains("nowhere")));
                }
                2 => w.world.locations.get_mut(room).unwrap().exits.clear(),
                _ => {
                    w.world.npcs.clear();
                    w.world.items.clear();
                }
            }
            let errors = validate(&w);
            assert!(!errors.is_empty(), "breakage went unnoticed in {room}");
        }
    }
}
