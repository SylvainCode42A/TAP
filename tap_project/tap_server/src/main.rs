use std::net::{TcpListener, TcpStream};
use std::io::{BufRead, BufReader, Write};
use std::thread;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};

#[derive(Deserialize)]
struct World {
    world: WorldData,
}

#[derive(Deserialize)]
struct WorldData {
    locations: HashMap<String, Location>,
    items: HashMap<String, Item>,
    npcs: HashMap<String, Npc>,
    quests: HashMap<String, Quest>,
}

#[derive(Deserialize, Debug)]
struct Location {
    name: String,
    description: String,
    exits: HashMap<String, String>,
    #[serde(default)]
    items: Vec<String>,
    #[serde(default)]
    spawns: Vec<Spawn>,
}

#[derive(Deserialize)]
struct Item {
    name: String,
    description: String,
    obtainable: bool,
}

#[derive(Deserialize)]
struct Npc {
    name: String,
    description: String,
    dialogue: Vec<String>,
    stats: HashMap<String, u32>,
}

#[derive(Deserialize)]
struct Quest {
    name: String,
    description: String,
    objectives: Vec<String>,
    rewards: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct Spawn {
    npc_type: String,
    count: u32,
}

enum Command {
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

enum Response {
    Ok(String),
    Err { code: u16, message: String },
}

struct Player {
    username: String,
    current_room: String,
    inventory: Vec<String>,
    hp: u32,
}

struct GameState {
    world: World,
    players: HashMap<String, Player>,
}

impl World {
    fn load_world() -> World {
        let content = fs::read_to_string("data/world.yaml").expect("Impossible to read world.yaml");
        serde_yaml::from_str(&content).expect("Parsing error")
    }
}

fn dispatch(cmd: Command, game_state: &Arc<Mutex<GameState>>, current_username: &Option<String>) -> Response {
    match cmd {
        Command::Connect { username } => {
            let mut gs = game_state.lock().unwrap();
            if gs.players.contains_key(&username) {
                Response::Err { code: 401, message: format!("{} already connected", username) }
            } else {
                let player = Player {
                    username: username.clone(),
                    current_room: "entrance".to_string(),
                    inventory: vec![],
                    hp: 100,
                };
                gs.players.insert(username.clone(), player);
                Response::Ok(format!("{} connected", username))
            }
        },

        Command::Look => {
            match current_username {
                Some(username) => {
                    let gs = game_state.lock().unwrap();
                    match gs.players.get(username) {
                        Some(player) => match gs.world.world.locations.get(&player.current_room) {
                            Some(room) => Response::Ok(format!("{}_room = {:?}", player.username, room)),
                            None => Response::Err { code: 404, message: "Current room not found".to_string() }
                        }
                        None => Response::Err { code: 404, message: "Current player not found".to_string() }
                    }
                }
                None => {
                    Response::Err { code: 404, message: "Try with first CONNECTED".to_string() }
                }
            }
        },

        Command::Move { direction } => {
            match current_username {
                None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
                Some(username) => {
                    let mut gs =  game_state.lock().unwrap();
                    match gs.players.get(username) {
                        None => Response::Err { code: 404, message: "Current room not found".to_string() },
                        Some(player) => {
                            match gs.world.world.locations.get(&player.current_room) {
                                None => Response::Err { code: 404, message: "Current room not found".to_string() },
                                Some(room) => {
                                    match room.exits.get(&direction.to_lowercase()).cloned() {
                                        None => Response::Err { code: 406, message: format!("No exit {} from here", direction) },
                                        Some(new_room) => {
                                            if let Some(p) = gs.players.get_mut(username) {
                                                p.current_room = new_room.clone();
                                            }
                                            Response::Ok(format!("room=next_is_{}", new_room))
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },

        Command::Take { item } => {
            match current_username {
                None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
                Some(username) => {
                    let mut gs = game_state.lock().unwrap();
                    let current_room = match gs.players.get(username) {
                        None => return Response::Err { code: 404, message: "Player not connected".to_string() },
                        Some(player) => player.current_room.clone(),
                    };
                    match gs.world.world.locations.get_mut(&current_room) {
                        None => Response::Err { code: 404, message: "Current room not found".to_string() },
                        Some(room) => {
                            if !room.items.contains(&item) {
                                Response::Err { code: 404, message: format!("{} not in this room", item) }
                            } else {
                                room.items.retain(|i| i != &item);
                                if let Some(p) = gs.players.get_mut(username) {
                                    p.inventory.push(item.clone());
                                }
                                Response::Ok(format!("taken={}", item))
                            }
                        }
                    }
                }
            }
        },

        Command::Drop { item } => {
            match current_username {
                None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
                Some(username) => {
                    let mut gs = game_state.lock().unwrap();
                    let current_room = match gs.players.get_mut(username) {
                        None => return Response::Err { code: 404, message: "Player not connected".to_string() },
                        Some(player) => player.current_room.clone(),
                    };
                    let has_item = match gs.players.get(username) {
                        None => return Response::Err { code: 404, message: "Player not in room".to_string() },
                        Some(player) => player.inventory.contains(&item)
                    };
                    if !has_item {
                        Response::Err { code: 404, message: format!("{} not in inventory", item) }
                    } else {
                        if let Some(player) = gs.players.get_mut(username) {
                            player.inventory.retain(|i| i != &item)
                        }
                        if let Some(room) = gs.world.world.locations.get_mut(&current_room) {
                            room.items.push(item.clone());
                        }
                        Response::Ok(format!("dropped={}", item))
                    }
                }
            }
        },

        Command::Inventory => {
            match current_username {
                None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
                Some(username) => {
                    let gs = game_state.lock().unwrap();
                    match gs.players.get(username) {
                        None => Response::Err { code: 404, message: "Player not CONNECTED".to_string() },
                        Some(player) => Response::Ok(format!("{:?}", player.inventory)),
                    }
                }
            }
        },

        Command::Talk { npc } => Response::Err { code: 404, message: format!("{} not found in room", npc) },
        Command::Attack { target } => Response::Err { code: 404, message: format!("{} not found in room", target) },
        Command::Status => Response::Err { code: 404, message: "Status can't check".to_string() },
        Command::Quest { quest_id } => Response::Err { code: 404, message: format!("{} not found in room", quest_id) },
        Command::Quests => Response::Err { code: 404, message: "Quests not set at the time".to_string() },
        Command::Who => Response::Err { code: 404, message: "Nobody in world".to_string() },
        Command::GroupInvite { username } => Response::Err { code: 404, message: format!("Group not found to invit {}", username) },
        Command::GroupJoin {group_id} => Response::Err { code: 404, message: format!("{} not found", group_id) },
        Command::GroupLeave => Response::Err { code: 404, message: "Can't leave this group".to_string() },
        Command::ChatGlobal { message } => Response::Err { code: 404, message: format!("ChatGlobal not found to send {}", message) },
        Command::ChatRoom { message } => Response::Err { code: 404, message: format!("ChatRoom not found to send {}", message) },
        Command::ChatGroup { message } => Response::Err { code: 404, message: format!("ChatGroup not found to send {}", message) },
        Command::Quit => Response::Ok("OK goodbye".to_string()),
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
            handle_(&mut stream, game_state_clone);
        });
    }
}

fn handle_(stream: &mut TcpStream, game_state: Arc<Mutex<GameState>>) {
    let mut current_username: Option<String> = None;

    loop {
        let mut reader = BufReader::new(&mut *stream);
        let mut line = String::new();

        match reader.read_line(&mut line) {

            Ok(0) => break,
            Ok(_) => {
                let mut matched = false;
                let mut should_quit = false;
                let mut cmd: Option<Command> = None;
                let actions: Vec<&str> = line.split_whitespace().collect();

                if actions.get(0) == Some(&"CONNECT") {
                    matched = true;
                    if actions.len() == 2 {
                        current_username = Some(actions[1].to_string());
                        cmd = Some(Command::Connect { username: actions[1].to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CONNECT");
                    }
                }

                if line == "LOOK\n" {
                    matched = true;
                    cmd = Some(Command::Look);
                }

                if actions.get(0) == Some(&"MOVE") {
                    matched = true;
                    if actions.len() == 2 {
                        let upper = actions[1].trim().to_uppercase();
                        if upper == "NORTH" {
                            cmd = Some(Command::Move { direction: actions[1].to_string() });
                        }
                        else if upper == "SOUTH" {
                            cmd = Some(Command::Move { direction: actions[1].to_string() });
                        }
                        else if upper == "WEST" {
                            cmd = Some(Command::Move { direction: actions[1].to_string() });
                        }
                        else if upper == "EAST" {
                            cmd = Some(Command::Move { direction: actions[1].to_string() });
                        }
                        else {
                            send_error_mess(stream, "406", "Invalid direction for MOVE");
                        }
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for MOVE");
                    }
                }

                if actions.get(0) == Some(&"TAKE") {
                    matched = true;
                    if actions.len() >= 2 {
                        let item = actions[1..].join(" ");
                        cmd = Some(Command::Take { item: item.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for TAKE");
                    }
                }

                if actions.get(0) == Some(&"DROP") {
                    matched = true;
                    if actions.len() >= 2 {
                        let item = actions[1..].join(" ");
                        cmd = Some(Command::Drop { item: item.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for DROP");
                    }
                }

                if line == "INVENTORY\n" {
                    matched = true;
                    cmd = Some(Command::Inventory);
                }

                if actions.get(0) == Some(&"TALK") {
                    matched = true;
                    if actions.len() >= 2 {
                        let item = actions[1..].join(" ");
                        cmd = Some(Command::Talk { npc: item.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for TALK");
                    }
                }

                if actions.get(0) == Some(&"ATTACK") {
                    matched = true;
                    if actions.len() >= 2 {
                        let item = actions[1..].join(" ");
                        cmd = Some(Command::Attack { target: item.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for ATTACK");
                    }
                }

                if line == "STATUS\n" {
                    matched = true;
                    cmd = Some(Command::Status);
                }

                if actions.get(0) == Some(&"QUEST") {
                    matched = true;
                    if actions.len() >= 2 {
                        let item = actions[1..].join(" ");
                        cmd = Some(Command::Quest { quest_id: item.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for QUEST");
                    }
                }

                if line == "QUESTS\n" {
                    matched = true;
                    cmd = Some(Command::Quests);
                }

                if line == "WHO\n" {
                    matched = true;
                    cmd = Some(Command::Who);
                }

                if actions.get(0) == Some(&"GROUP") && actions.get(1) == Some(&"INVITE") {
                    matched = true;
                    if actions.len() == 3 {
                        cmd = Some(Command::GroupInvite { username: actions[2].to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for GROUP INVITE");
                    }
                }

                if actions.get(0) == Some(&"GROUP") && actions.get(1) == Some(&"JOIN") {
                    matched = true;
                    if actions.len() == 3 {
                        cmd = Some(Command::GroupJoin { group_id: actions[2].to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for GROUP JOIN");
                    }
                }

                if line == "GROUP LEAVE\n" {
                    matched = true;
                    cmd = Some(Command::GroupLeave);
                }

                if actions.get(0) == Some(&"CHAT") && actions.get(1) == Some(&"GLOBAL") {
                    matched = true;
                    if actions.len() > 2 {
                        let rest = actions[2..].join(" ");
                        cmd = Some(Command::ChatGlobal { message: rest.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CHAT GLOBAL");
                    }
                }

                if actions.get(0) == Some(&"CHAT") && actions.get(1) == Some(&"ROOM") {
                    matched = true;
                    if actions.len() > 2 {
                        let rest = actions[2..].join(" ");
                        cmd = Some(Command::ChatRoom { message: rest.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CHAT ROOM");
                    }
                }

                if actions.get(0) == Some(&"CHAT") && actions.get(1) == Some(&"GROUP") {
                    matched = true;
                    if actions.len() > 2 {
                        let rest = actions[2..].join(" ");
                        cmd = Some(Command::ChatGroup { message: rest.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CHAT GROUP");
                    }
                }

                if line == "QUIT\n" {
                    matched = true;
                    cmd = Some(Command::Quit);
                    should_quit = true;
                }

                if !matched {
                    send_error_mess(stream, "401", "Unknown command");
                }

                if let Some(c) = cmd {
                    let resp = dispatch(c, &game_state, &current_username);
                    match resp {
                        Response::Ok(msg) => {
                            let message = format!("OK: {}", msg);
                            println!("{}", message);
                            stream.write_all(message.as_bytes()).expect("Failed to send response");
                        }
                        Response::Err { code, message } => {
                            send_error_mess(stream, &code.to_string(), &message);
                        }
                    }
                }

                if should_quit {
                    break;
                }
            },

            Err(e) => {
                println!("ERROR: {}", e);
                break;
            },
        }
    }
}

fn send_error_mess(stream: &mut TcpStream, t_error: &str, error: &str) {
    let message = format!("ERR {} {}\n", t_error, error);
    println!("{}", message.trim());
    stream.write_all(message.as_bytes()).expect("Failed to send message error");
}