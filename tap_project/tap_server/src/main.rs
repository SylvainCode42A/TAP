use std::net::{TcpListener, TcpStream};
use std::io::Write;
use std::thread;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};
mod combat;
mod dispatch;
mod group;
mod parsing;
mod quest;
mod world_check;

#[derive(Deserialize)]
pub struct World {
    pub world: WorldData,
}

#[derive(Deserialize)]
pub struct WorldData {
    pub locations: HashMap<String, Location>,
    pub items: HashMap<String, Item>,
    pub npcs: HashMap<String, Npc>,
    pub quests: HashMap<String, Quest>,
}

#[derive(Deserialize, Debug)]
pub struct Location {
    pub name: String,
    pub description: String,
    pub exits: HashMap<String, String>,
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(default)]
    pub spawns: Vec<Spawn>,
}

#[derive(Deserialize)]
pub struct Item {
    pub name: String,
    pub description: String,
    pub obtainable: bool,
}

#[derive(Deserialize)]
pub struct Npc {
    pub name: String,
    pub description: String,
    pub dialogue: Vec<String>,
    pub stats: HashMap<String, u32>,
    /// True for enemies: only they accept `ATTACK`. Absent in YAML: false.
    #[serde(default)]
    pub hostile: bool,
}

#[derive(Deserialize)]
pub struct Quest {
    pub name: String,
    pub description: String,
    pub objectives: Vec<String>,
    pub rewards: Vec<String>,
    /// Id of the NPC that gives and takes back this quest.
    #[serde(default)]
    pub giver: String,
    /// Item that must be in the inventory to turn the quest in.
    #[serde(default)]
    pub requires_item: Option<String>,
    /// NPC id the player must have beaten to turn the quest in.
    #[serde(default)]
    pub requires_kill: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct Spawn {
    pub npc_type: String,
    pub count: u32,
}

pub enum Command {
    Connect { username: String },
    Look,
    Move { direction: String },
    Take { item: String },
    Drop { item: String },
    Inventory,
    Talk { npc: String },
    Attack { target: String },
    Status,
    Quest { quest_id: String },
    Quests,
    Who,
    GroupCreate,
    GroupInvite { username: String },
    GroupJoin { group_id: String },
    GroupLeave,
    ChatGlobal { message: String },
    ChatRoom { message: String },
    ChatGroup { message: String },
    Quit,
}

pub enum Response {
    Ok(String),
    Err { code: u16, message: String },
}

pub struct Player {
    pub username: String,
    pub current_room: String,
    pub inventory: Vec<String>,
    pub hp: u32,
    pub talk_progress: HashMap<String, usize>,
    /// NPC id fought in the last round, None outside combat.
    pub in_combat: Option<String>,
    /// NPC ids this player has beaten at least once.
    pub kills: Vec<String>,
    /// Quests taken, by quest id. A quest never taken has no entry.
    pub quests: HashMap<String, quest::QuestStatus>,
}

impl Player {
    /// A fresh player: full health, empty inventory, at the entrance.
    pub fn new(username: &str) -> Player {
        Player {
            username: username.to_string(),
            current_room: combat::RESPAWN_ROOM.to_string(),
            inventory: vec![],
            hp: combat::MAX_HP,
            talk_progress: HashMap::new(),
            in_combat: None,
            kills: vec![],
            quests: HashMap::new(),
        }
    }
}

pub struct GameState {
    pub world: World,
    pub players: HashMap<String, Player>,
    /// Current hp of each NPC hit at least once, by NPC id. An NPC never
    /// hit has no entry: its hp is still the `hp` stat of world.yaml.
    pub npc_hp: HashMap<String, u32>,
    /// Every group, by group id (`grp.<founder>`).
    pub groups: HashMap<String, group::Group>,
}

impl World {
    fn load_world() -> World {
        let content = fs::read_to_string("data/world.yaml").expect("Impossible to read world.yaml");
        serde_yaml::from_str(&content).expect("Parsing error")
    }
}

fn main() {
    let world = World::load_world();

    // Refuse to start on a broken world, listing every problem at once.
    let errors = world_check::validate(&world);
    if !errors.is_empty() {
        for error in &errors {
            eprintln!("ERROR world.yaml: {}", error);
        }
        eprintln!(
            "ERROR world.yaml: {} problem(s), server not started",
            errors.len()
        );
        std::process::exit(1);
    }

    let game_state = GameState {
        world,
        players: HashMap::new(),
        npc_hp: HashMap::new(),
        groups: HashMap::new(),
    };

    let game_state = Arc::new(Mutex::new(game_state));

    let listener = TcpListener::bind("127.0.0.1:7878").expect("Erreur de bind");

    for stream in listener.incoming() {
        let mut stream = stream.expect("Erreur lors de la récupération du stream");
        let game_state_clone = Arc::clone(&game_state);

        thread::spawn(move || {
            parsing::handle_(&mut stream, game_state_clone);
        });
    }
}

pub fn send_error_mess(stream: &mut TcpStream, t_error: &str, error: &str) {
    let message = format!("ERR {} {}\n", t_error, error);
    println!("{}", message.trim());
    stream.write_all(message.as_bytes()).expect("Failed to send message error");
}

#[cfg(test)]
mod game_stress;

#[cfg(test)]
mod test_support {
    use super::*;

    /// A game on the real world.yaml with one fresh player in `room`.
    pub fn game_with_player(username: &str, room: &str) -> GameState {
        let mut player = Player::new(username);
        player.current_room = room.to_string();
        GameState {
            world: World::load_world(),
            players: HashMap::from([(username.to_string(), player)]),
            npc_hp: HashMap::new(),
            groups: HashMap::new(),
        }
    }

    /// Body of an `OK` reply. Panics on `ERR`, failing the test.
    pub fn ok_body(response: Response) -> String {
        match response {
            Response::Ok(body) => body,
            Response::Err { code, message } => panic!("expected OK, got ERR {} {}", code, message),
        }
    }
}
