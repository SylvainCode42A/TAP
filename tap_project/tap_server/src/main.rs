//! TAP server: checks `data/world.yaml`, then serves RFC 42TAP on
//! port 7878 of every interface, with one thread per client.
//!
//! This file holds the types every module shares: the world as read from
//! YAML, the live `GameState`, and the `Command` / `Response` pair that
//! links `parsing.rs` to `dispatch.rs`.

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

/// Every interface: clients of other machines, other groups' included,
/// reach the server.
const ADDRESS: &str = "0.0.0.0:7878";

/// Room part of a LOOK reply, RFC 5.1.2.
#[derive(Serialize)]
pub struct RoomInfo {
    /// Key of the room in `WorldData::locations`: a `Location` does not
    /// carry its own id.
    pub id: String,
    pub name: String,
    pub description: String,
    pub exits: HashMap<String, String>,
}

/// JSON body of a LOOK reply, RFC 5.1.2: field names and order are the
/// RFC's, clients parse them.
#[derive(Serialize)]
pub struct LookResponse {
    pub room: RoomInfo,
    /// Usernames in the room, the asking player included.
    pub players: Vec<String>,
    /// Item ids lying in the room.
    pub items: Vec<String>,
    /// NPC ids spawned in the room.
    pub npcs: Vec<String>,
}

/// Root of `world.yaml`, whose content sits under one top-level `world:`
/// key.
#[derive(Deserialize)]
pub struct World {
    pub world: WorldData,
}

/// The world, every table keyed by id: the ids clients send and replies
/// give back. Read once at startup; only room `items` change afterwards.
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
    /// Direction -> room id. Any word is a direction (`south_alt`), but
    /// keys must be lowercase: MOVE looks them up lowercased.
    pub exits: HashMap<String, String>,
    /// Item ids lying here now: TAKE removes them, DROP adds them.
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(default)]
    pub spawns: Vec<Spawn>,
}

#[derive(Deserialize, Serialize)]
pub struct Item {
    pub name: String,
    pub description: String,
    /// False for scenery: LOOK shows it, TAKE refuses it with 403.
    pub obtainable: bool,
}

#[derive(Deserialize, Serialize)]
pub struct Npc {
    pub name: String,
    pub description: String,
    /// Lines TALK gives in turn, cycling, counted per player.
    pub dialogue: Vec<String>,
    /// `hp` (1 when absent) and `attack`, the counter-attack damage (5 when
    /// absent). Other keys are ignored.
    pub stats: HashMap<String, u32>,
    /// True for enemies: only they accept `ATTACK`. Absent in YAML: false.
    #[serde(default)]
    pub hostile: bool,
}

#[derive(Deserialize, Serialize)]
pub struct Quest {
    pub name: String,
    pub description: String,
    /// For the world author only: no command sends it. The server checks
    /// `requires_item` and `requires_kill`, not this text.
    pub objectives: Vec<String>,
    /// Text of the `reward` field of the QUEST reply. The actual reward,
    /// a full heal, is given by `quest.rs` whatever this says.
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

/// An NPC placed in a room.
#[derive(Deserialize, Debug, Serialize)]
pub struct Spawn {
    /// NPC id in `WorldData::npcs`.
    pub npc_type: String,
    /// Read but unused: there is one instance per NPC id, whose health
    /// every player shares.
    pub count: u32,
}

/// One client line once parsed, RFC 5: keywords already matched ignoring
/// case, arguments kept as typed.
///
/// - `Take` and `Drop` name an item by id or display name, RFC 8.3;
///   `Talk` and `Attack` name an NPC the same way.
/// - Despite their field names, `Quest { quest_id }` holds the NPC asked
///   (`QUEST <npc>`, RFC 5.4.7) and `GroupJoin { group_id }` the leader's
///   name (RFC 5.3.3).
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

/// Outcome of one command. `parsing.rs` writes `Ok(body)` as `OK <body>`,
/// or a bare `OK` when the body is empty, and `Err` as
/// `ERR <code> <message>`.
pub enum Response {
    Ok(String),
    Err { code: u16, message: String },
}

pub struct Player {
    pub username: String,
    /// Room id.
    pub current_room: String,
    /// Item ids, in pickup order.
    pub inventory: Vec<String>,
    pub hp: u32,
    /// Index of the next TALK line, by NPC id.
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

/// Everything that changes while the server runs, shared by every client
/// thread behind one `Arc<Mutex<_>>`. A command holds the lock from start
/// to end: commands, and so combat turns, apply one at a time.
pub struct GameState {
    pub world: World,
    /// Players in the game by username, from CONNECT to departure.
    pub players: HashMap<String, Player>,
    /// Current hp of each NPC hit at least once, by NPC id. An NPC never
    /// hit has no entry: its hp is still the `hp` stat of world.yaml.
    pub npc_hp: HashMap<String, u32>,
    /// Every group, by group id (`grp.<founder>`).
    pub groups: HashMap<String, group::Group>,
    /// A clone of each player's socket, by username, to push `EVT` lines.
    /// Added after a successful CONNECT, removed on departure.
    pub senders: HashMap<String, TcpStream>,
}

impl World {
    /// Reads `data/world.yaml` relative to the working directory, so the
    /// server must start from `tap_server/` (`make run-server` does).
    /// Panics on a missing or malformed file: no game runs without a world.
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
                "address": ADDRESS,
                "rooms": data.locations.len(),
                "items": data.items.len(),
                "npcs": data.npcs.len(),
                "quests": data.quests.len(),
            }),
        );
    }

    let game_state = Arc::new(Mutex::new(game_state));

    let listener = TcpListener::bind(ADDRESS).expect("Erreur de bind");

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

/// Writes `ERR <t_error> <error>` to `stream` and logs it as a WARN.
/// Never fails: a write error means the client left, which its next read
/// sees.
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
