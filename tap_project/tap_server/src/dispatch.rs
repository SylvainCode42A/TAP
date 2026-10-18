use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use crate::{Command, Response, GameState, Player};

pub fn dispatch(cmd: Command, game_state: &Arc<Mutex<GameState>>, current_username: &Option<String>) -> Response {
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
                    talk_progress: HashMap::new(),
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

        Command::Talk { npc } => {
            match current_username {
                None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
                Some(username) => {
                    let mut gs = game_state.lock().unwrap();
                    match gs.world.world.npcs.get(&npc) {
                        None => Response::Err { code: 404, message: format!("{} not found", npc) },
                        Some(npc_data) => {
                            let dialogue = npc_data.dialogue.clone();
                            match gs.players.get_mut(username) {
                                None => Response::Err { code: 404, message: "Player not CONNECTED".to_string() },
                                Some(player) => {
                                    let index = player.talk_progress.entry(npc.clone()).or_insert(0);
                                    let phrase = dialogue[*index % dialogue.len()].clone();
                                    *index += 1;
                                    Response::Ok(phrase)
                                }
                            }
                        }
                    }
                }
            }
        },

        Command::Attack { target } => {
            Response::Err { code: 404, message: format!("{} not found in room", target) }
        },

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