use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use serde_json::json;
mod combat;
mod dispatch;
mod events;
mod group;
mod items;
mod logging;
mod parsing;
mod quest;
mod world_check;

#[derive(Serialize)]
pub struct RoomInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub exits: HashMap<String, String>,
}

#[derive(Serialize)]
pub struct LookResponse {
    pub room: RoomInfo,
    pub players: Vec<String>,
    pub items: Vec<String>,
    pub npcs: Vec<String>,
}

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

#[derive(Deserialize, Debug, Serialize)]
pub struct Location {
    pub name: String,
    pub description: String,
    pub exits: HashMap<String, String>,
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(default)]
    pub spawns: Vec<Spawn>,
}

#[derive(Deserialize, Serialize)]
pub struct Item {
    pub name: String,
    pub description: String,
    pub obtainable: bool,
}

#[derive(Deserialize, Serialize)]
pub struct Npc {
    pub name: String,
    pub description: String,
    pub dialogue: Vec<String>,
    pub stats: HashMap<String, u32>,
    /// True for enemies: only they accept `ATTACK`. Absent in YAML: false.
    #[serde(default)]
    pub hostile: bool,
}

#[derive(Deserialize, Serialize)]
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

#[derive(Deserialize, Debug, Serialize)]
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
    pub senders: HashMap<String, TcpStream>,
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
        senders: HashMap::new(),
    };

    logging::init(logging::LOG_FILE);
    {
        let data = &game_state.world.world;
        logging::info(
            "server_start",
            json!({
                "address": "127.0.0.1:7878",
                "rooms": data.locations.len(),
                "items": data.items.len(),
                "npcs": data.npcs.len(),
                "quests": data.quests.len(),
            }),
        );
    }

    let game_state = Arc::new(Mutex::new(game_state));

    let listener = TcpListener::bind("127.0.0.1:7878").expect("Erreur de bind");

    // Connections per IP in the last seconds, to spot rapid reconnection.
    let mut connections: HashMap<std::net::IpAddr, logging::RateWatch> = HashMap::new();

    for stream in listener.incoming() {
        // A failed accept (no file descriptor left, aborted handshake)
        // loses one client, never the server.
        let mut stream = match stream {
            Ok(stream) => stream,
            Err(error) => {
                logging::error("accept_failed", json!({"error": error.to_string()}));
                continue;
            }
        };
        if let Ok(peer) = stream.peer_addr() {
            let now = Instant::now();
            connections.retain(|_, watch| !watch.is_idle(now));
            let watch = connections.entry(peer.ip()).or_insert_with(|| {
                logging::RateWatch::new(logging::RAPID_LIMIT, logging::RAPID_WINDOW)
            });
            if watch.hit(now) {
                logging::warn(
                    "abuse_rapid_connections",
                    json!({
                        "ip": peer.ip().to_string(),
                        "limit": logging::RAPID_LIMIT,
                        "window_s": logging::RAPID_WINDOW.as_secs(),
                    }),
                );
            }
        }
        let game_state_clone = Arc::clone(&game_state);

        thread::spawn(move || {
            parsing::handle_(&mut stream, game_state_clone);
        });
    }
}

pub fn send_error_mess(stream: &mut TcpStream, t_error: &str, error: &str) {
    let message = format!("ERR {} {}\n", t_error, error);
    let ip = stream
        .peer_addr()
        .map_or("unknown".to_string(), |peer| peer.to_string());
    logging::warn("reply", json!({"ip": ip, "reply": message.trim_end()}));
    // A lost client is seen by its next read, which ends the session.
    let _ = stream.write_all(message.as_bytes());
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
            senders: HashMap::new(),
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
