use std::net::{TcpListener, TcpStream};
use std::io::Write;
use std::thread;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};
mod dispatch;
mod parsing;

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
}

#[derive(Deserialize)]
pub struct Quest {
    pub name: String,
    pub description: String,
    pub objectives: Vec<String>,
    pub rewards: Vec<String>,
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
}

pub struct GameState {
    pub world: World,
    pub players: HashMap<String, Player>,
}

impl World {
    fn load_world() -> World {
        let content = fs::read_to_string("data/world.yaml").expect("Impossible to read world.yaml");
        serde_yaml::from_str(&content).expect("Parsing error")
    }
}

fn main() {
    let game_state = GameState {
        world: World::load_world(),
        players: HashMap::new(),
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